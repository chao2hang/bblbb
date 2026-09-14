//! GET /api/v1/recommendations — 兴趣推荐流（v1：画像 + 热度 + 新鲜度）。
//!
//! 产品决策（2026-09）：发现页改为算法推送，去掉手动排序 tab 与标签入口；
//! 本端点是推荐流的数据源。评分在 Rust 侧完成（纯算术，不使用方言函数，
//! MySQL/SQLite 双池通用）：
//!
//! ```text
//! interest  = 3×关注板块 + 2×互动过(赞/回)的板块 + 1.5×min(兴趣标签命中数, 3)
//! engage    = (reply×2 + like×4) / (reply×2 + like×4 + 40)   // 饱和 0..1
//! freshness = 1 / (1 + age_hours / 72)                       // 72h 半衰
//! score     = 4×interest + 3×engage + 2×fresh
//! ```
//!
//! - 候选池：published、未删除、**排除本人/已点赞/已回复**的帖子，按
//!   `created_at DESC` 取最近 [`RECOMMEND_CANDIDATE_POOL`] 条（新鲜优先，
//!   旧帖不无限回流；质量召回交给后续迭代）；
//! - 冷启动（匿名 / 无任何互动信号）：interest = 0，退化为「热度×新鲜度」
//!   排序（比纯 view_count 更抗刷），响应 `strategy` 字段向调用方暴露；
//! - `reason`：你关注的板块 > 相关标签 > 你互动过的板块 > 社区热门——
//!   给前端行徽标用，让"为什么推给我"可解释；
//! - 隐私：只消费公开互动（点赞/回复/关注），不读浏览历史（无逐用户浏览表）。

use std::collections::HashMap;

use axum::{
    extract::{Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    app::AppState,
    auth::session::AuthSession,
    db::DatabasePool,
    error::AppError,
    outbox::now_millis,
    routes::posts::{post_summary_json, PostListRow},
};

/// 候选池大小：按 created_at 取最近的 N 条参与评分。
pub const RECOMMEND_CANDIDATE_POOL: i64 = 240;

/// 默认返回条数（与发现页 SSR 的 limit 对齐）。
fn default_limit() -> i64 {
    8
}

#[derive(Deserialize)]
struct ListRecommendationsQuery {
    #[serde(default = "default_limit")]
    limit: i64,
}

/// 用户兴趣画像（全部由公开互动推导，无浏览历史）。
#[derive(Default, Debug, Clone)]
pub struct InterestProfile {
    /// 关注的板块（board_follows）——最强信号。
    pub followed_boards: std::collections::HashSet<String>,
    /// 点赞/回复过的帖子所属板块——中强信号。
    pub interacted_boards: std::collections::HashSet<String>,
    /// 互动过的帖子上的标签（tag_id → 命中的帖子数）。
    pub interacted_tags: HashMap<String, i64>,
}

impl InterestProfile {
    /// 无任何信号（匿名或新用户）→ 冷启动，退化为热度+新鲜度。
    pub fn is_empty(&self) -> bool {
        self.followed_boards.is_empty()
            && self.interacted_boards.is_empty()
            && self.interacted_tags.is_empty()
    }
}

/// 推荐结果行：帖子投影 + 评分 + 推荐理由。
pub(crate) struct RecommendedPost {
    pub row: PostListRow,
    pub score: f64,
    pub reason: &'static str,
}

/// 推荐理由（按信号强度优先级取其一）。
const REASON_FOLLOWED_BOARD: &str = "你关注的板块";
const REASON_TAG_MATCH: &str = "相关标签";
const REASON_INTERACTED_BOARD: &str = "你互动过的板块";
const REASON_TRENDING: &str = "社区热门";

/// 路由：GET /api/v1/recommendations（匿名可读；登录后个性化）。
pub fn router() -> Router<AppState> {
    Router::new().route("/api/v1/recommendations", get(list_recommendations))
}

async fn list_recommendations(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(query): Query<ListRecommendationsQuery>,
) -> Result<Response, AppError> {
    let request_id = "list_recommendations";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let limit = query.limit.clamp(1, 50);
    let viewer_id = auth.user.as_ref().map(|u| u.id.clone());
    let now = now_millis();

    let (items, strategy) =
        recommend_posts(pool, viewer_id.as_deref(), limit, now, request_id).await?;

    // 缓存：匿名推荐为全局相同结果（public 短缓存）；登录后为个性化结果
    // （private no-store，禁止跨 persona 复用）。
    let mut resp = (
        StatusCode::OK,
        Json(json!({ "items": items, "strategy": strategy })),
    )
        .into_response();
    if viewer_id.is_some() {
        resp.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("private, no-store"),
        );
    } else {
        resp.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=30"),
        );
    }
    Ok(resp)
}

/// 推荐核心：画像 → 候选池 → 评分排序。`viewer_id` 为 None 时冷启动。
///
/// 返回 (items, strategy)；`strategy` = `interest-v1`（有画像）|
/// `trending-fallback`（冷启动兜底）。
pub async fn recommend_posts(
    pool: &DatabasePool,
    viewer_id: Option<&str>,
    limit: i64,
    now: i64,
    request_id: &'static str,
) -> Result<(Vec<Value>, &'static str), AppError> {
    let limit = limit.clamp(1, 50);
    let profile = match viewer_id {
        Some(id) => load_interest_profile(pool, id, request_id).await?,
        None => InterestProfile::default(),
    };
    let strategy = if profile.is_empty() {
        "trending-fallback"
    } else {
        "interest-v1"
    };

    let rows = fetch_candidates(pool, viewer_id, RECOMMEND_CANDIDATE_POOL, request_id).await?;
    let tag_index = load_candidate_tags(pool, &rows, request_id).await?;

    let mut scored: Vec<RecommendedPost> = rows
        .into_iter()
        .map(|row| {
            let (score, reason) = score_candidate(&row, &profile, &tag_index, now);
            RecommendedPost { row, score, reason }
        })
        .collect();
    // 稳定排序：分数降序 → created_at 降序 → id 降序（确定性，防抖动）。
    scored.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then(b.row.created_at.cmp(&a.row.created_at))
            .then(b.row.id.cmp(&a.row.id))
    });
    scored.truncate(limit as usize);

    let items = {
        // 参与者预览（一页一查询）：楼主之外已发布回复的不同作者，每帖 ≤2 个。
        let post_ids: Vec<String> = scored.iter().map(|s| s.row.id.clone()).collect();
        let participants =
            crate::routes::posts::fetch_post_participants(pool, &post_ids, request_id).await?;
        // 作者装扮投影（作者去重后逐个查询）：与帖子列表行同构。
        let author_ids: Vec<String> = scored.iter().map(|s| s.row.author_id.clone()).collect();
        let author_tokens =
            crate::routes::posts::fetch_author_presentation_tokens(pool, &author_ids).await;
        let empty_participants: Vec<crate::routes::posts::PostParticipantRow> = Vec::new();
        scored
            .iter()
            .map(|s| {
                let mut v = post_summary_json(
                    &s.row,
                    participants
                        .get(&s.row.id)
                        .map(|v| v.as_slice())
                        .unwrap_or(&empty_participants),
                    author_tokens.get(&s.row.author_id),
                );
                if let Value::Object(ref mut m) = v {
                    m.insert("reason".to_owned(), Value::String(s.reason.to_owned()));
                    m.insert(
                        "score".to_owned(),
                        json!((s.score * 1000.0).round() / 1000.0),
                    );
                }
                v
            })
            .collect()
    };
    Ok((items, strategy))
}

/// 评分：interest（画像命中）+ engage（互动饱和）+ fresh（时间衰减）。
/// 纯函数（单元测试覆盖权重行为）。
pub(crate) fn score_candidate(
    row: &PostListRow,
    profile: &InterestProfile,
    tag_index: &HashMap<String, Vec<String>>,
    now: i64,
) -> (f64, &'static str) {
    let followed = profile.followed_boards.contains(&row.board_id);
    let interacted = profile.interacted_boards.contains(&row.board_id);
    let tag_hits = tag_index
        .get(&row.id)
        .map(|tags| {
            tags.iter()
                .filter(|t| profile.interacted_tags.contains_key(*t))
                .count() as i64
        })
        .unwrap_or(0);

    let mut interest = 0.0_f64;
    let mut reason = REASON_TRENDING;
    if followed {
        interest += 3.0;
        reason = REASON_FOLLOWED_BOARD;
    }
    if interacted {
        interest += 2.0;
        if !followed {
            reason = REASON_INTERACTED_BOARD;
        }
    }
    if tag_hits > 0 {
        interest += 1.5 * (tag_hits.min(3) as f64);
        if !followed {
            reason = REASON_TAG_MATCH;
        }
    }

    let engagement = row.reply_count as f64 * 2.0 + row.like_count as f64 * 4.0;
    let engage_sat = engagement / (engagement + 40.0);
    let age_hours = ((now - row.created_at).max(0)) as f64 / 3_600_000.0;
    let fresh = 1.0 / (1.0 + age_hours / 72.0);

    let score = 4.0 * interest + 3.0 * engage_sat + 2.0 * fresh;
    (score, reason)
}

/// 双池通用的元组行查询：同一 SQL 在 SQLite/MySQL 上各自实例化
/// （`sqlx::query_as` 对元组行双库都实现 FromRow，两臂返回类型一致）。
macro_rules! fetch_rows {
    ($pool:expr, $sql:expr $(, $bind:expr)* ) => {
        match $pool {
            sqlx::Either::Left(p) => sqlx::query_as($sql)
                $(.bind($bind))*
                .fetch_all(p)
                .await,
            sqlx::Either::Right(p) => sqlx::query_as($sql)
                $(.bind($bind))*
                .fetch_all(p)
                .await,
        }
    };
}

/// 加载兴趣画像：关注板块 + 互动板块（赞/回）+ 互动标签（tag → 命中帖数）。
async fn load_interest_profile(
    pool: &DatabasePool,
    viewer_id: &str,
    request_id: &'static str,
) -> Result<InterestProfile, AppError> {
    let mut profile = InterestProfile::default();

    // 1) 关注的板块（最强信号）。
    let rows: Vec<(String,)> = fetch_rows!(
        pool,
        "SELECT board_id FROM board_follows WHERE user_id = ?",
        viewer_id
    )
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    for (board_id,) in rows {
        profile.followed_boards.insert(board_id);
    }

    // 2) 点赞过的帖子所属板块。
    let rows: Vec<(String,)> = fetch_rows!(
        pool,
        "SELECT DISTINCT p.board_id FROM post_reactions r
         JOIN posts p ON p.id = r.post_id
         WHERE r.user_id = ? AND r.reaction = 'like'",
        viewer_id
    )
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    for (board_id,) in rows {
        profile.interacted_boards.insert(board_id);
    }

    // 3) 回复过的帖子所属板块。
    let rows: Vec<(String,)> = fetch_rows!(
        pool,
        "SELECT DISTINCT p.board_id FROM comments c
         JOIN posts p ON p.id = c.post_id
         WHERE c.author_id = ? AND c.status = 'published'",
        viewer_id
    )
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    for (board_id,) in rows {
        profile.interacted_boards.insert(board_id);
    }

    // 4) 互动过的帖子上的标签（tag_id → 命中帖数；UNION 去重后聚合）。
    let rows: Vec<(String, i64)> = fetch_rows!(
        pool,
        "SELECT pt.tag_id, COUNT(*) AS hits FROM
            (SELECT DISTINCT post_id FROM post_reactions WHERE user_id = ? AND reaction = 'like'
             UNION
             SELECT DISTINCT post_id FROM comments WHERE author_id = ? AND status = 'published') e
         JOIN post_tags pt ON pt.post_id = e.post_id
         GROUP BY pt.tag_id",
        viewer_id,
        viewer_id
    )
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    for (tag_id, hits) in rows {
        profile.interacted_tags.insert(tag_id, hits);
    }

    Ok(profile)
}

/// 候选池：published、未删除、排除本人/已点赞/已回复，按 created_at DESC。
async fn fetch_candidates(
    pool: &DatabasePool,
    viewer_id: Option<&str>,
    pool_size: i64,
    request_id: &'static str,
) -> Result<Vec<PostListRow>, AppError> {
    let sql = "SELECT p.id, p.board_id, p.author_id, p.post_type, p.title, p.status,
                p.reply_count, p.view_count, p.created_at, p.updated_at, p.last_reply_at,
                p.pinned_at, p.pinned, p.featured_at, u.username_normalized as author_name,
                u.display_name as author_display_name,
                u.avatar_attachment_id as author_avatar_attachment_id,
                p.summary,
                (SELECT COUNT(*) FROM post_reactions pr WHERE pr.post_id = p.id AND pr.reaction = 'like') AS like_count
         FROM posts p
         LEFT JOIN users u ON u.id = p.author_id
         WHERE p.status = 'published' AND p.deleted_at IS NULL
           AND (? IS NULL OR p.author_id <> ?)
           AND NOT EXISTS (SELECT 1 FROM post_reactions x
                           WHERE x.post_id = p.id AND x.user_id = ? AND x.reaction = 'like')
           AND NOT EXISTS (SELECT 1 FROM comments c
                           WHERE c.post_id = p.id AND c.author_id = ? AND c.status = 'published')
         ORDER BY p.created_at DESC, p.id DESC
         LIMIT ?";
    let rows = match pool {
        sqlx::Either::Left(p) => {
            sqlx::query_as::<_, PostListRow>(sql)
                .bind(viewer_id)
                .bind(viewer_id)
                .bind(viewer_id)
                .bind(viewer_id)
                .bind(pool_size)
                .fetch_all(p)
                .await
        }
        sqlx::Either::Right(p) => {
            sqlx::query_as::<_, PostListRow>(sql)
                .bind(viewer_id)
                .bind(viewer_id)
                .bind(viewer_id)
                .bind(viewer_id)
                .bind(pool_size)
                .fetch_all(p)
                .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    Ok(rows)
}

/// 候选帖的标签索引（post_id → [tag_id]）：一次 IN 查询，评分用。
async fn load_candidate_tags(
    pool: &DatabasePool,
    rows: &[PostListRow],
    request_id: &'static str,
) -> Result<HashMap<String, Vec<String>>, AppError> {
    if rows.is_empty() {
        return Ok(HashMap::new());
    }
    let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
    let placeholders = vec!["?"; ids.len()].join(", ");
    let sql = format!("SELECT post_id, tag_id FROM post_tags WHERE post_id IN ({placeholders})");
    let pairs: Vec<(String, String)> = match pool {
        sqlx::Either::Left(p) => {
            let mut q = sqlx::query_as::<_, (String, String)>(&sql);
            for id in &ids {
                q = q.bind(*id);
            }
            q.fetch_all(p).await
        }
        sqlx::Either::Right(p) => {
            let mut q = sqlx::query_as::<_, (String, String)>(&sql);
            for id in &ids {
                q = q.bind(*id);
            }
            q.fetch_all(p).await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let mut index: HashMap<String, Vec<String>> = HashMap::new();
    for (post_id, tag_id) in pairs {
        index.entry(post_id).or_default().push(tag_id);
    }
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造 PostListRow 的最小工厂（非展示字段填默认）。
    fn row(
        board_id: &str,
        author_id: &str,
        created_at: i64,
        replies: i64,
        likes: i64,
    ) -> PostListRow {
        PostListRow {
            id: uuid::Uuid::now_v7().to_string(),
            board_id: board_id.to_owned(),
            author_id: author_id.to_owned(),
            post_type: "discussion".to_owned(),
            title: "t".to_owned(),
            status: "published".to_owned(),
            reply_count: replies,
            view_count: 0,
            created_at,
            updated_at: created_at,
            last_reply_at: None,
            pinned_at: None,
            pinned: 0,
            featured_at: None,
            author_name: Some("u".to_owned()),
            author_display_name: None,
            author_avatar_attachment_id: None,
            summary: None,
            like_count: likes,
        }
    }

    const NOW: i64 = 1_800_000_000_000;
    const HOUR: i64 = 3_600_000;

    fn profile_with(
        followed: &[&str],
        interacted: &[&str],
        tags: &[(&str, i64)],
    ) -> InterestProfile {
        InterestProfile {
            followed_boards: followed.iter().map(|s| s.to_string()).collect(),
            interacted_boards: interacted.iter().map(|s| s.to_string()).collect(),
            interacted_tags: tags.iter().map(|(t, h)| (t.to_string(), *h)).collect(),
        }
    }

    #[test]
    fn followed_board_beats_pure_engagement() {
        // 关注板块的旧帖/低互动 vs 无关板块的高互动新帖：interest 主导。
        let profile = profile_with(&["b-follow"], &[], &[]);
        let old_followed = row("b-follow", "alice", NOW - 7 * 24 * HOUR, 0, 0);
        let hot_new = row("b-other", "bob", NOW - 2 * HOUR, 20, 30);
        let (s_follow, r_follow) = score_candidate(&old_followed, &profile, &HashMap::new(), NOW);
        let (s_hot, _) = score_candidate(&hot_new, &profile, &HashMap::new(), NOW);
        assert!(
            s_follow > s_hot,
            "关注板块 {s_follow} 必须压过高热度 {s_hot}"
        );
        assert_eq!(r_follow, REASON_FOLLOWED_BOARD);
    }

    #[test]
    fn tag_match_beats_trending_and_reason_priority() {
        let profile = profile_with(&[], &[], &[("t1", 2)]);
        let tagged = row("b-x", "alice", NOW - HOUR, 0, 0);
        let mut index = HashMap::new();
        index.insert(tagged.id.clone(), vec!["t1".to_owned()]);
        let (s_tag, r_tag) = score_candidate(&tagged, &profile, &index, NOW);
        let plain = row("b-x", "bob", NOW - HOUR, 0, 0);
        let (s_plain, _) = score_candidate(&plain, &profile, &HashMap::new(), NOW);
        assert!(s_tag > s_plain);
        assert_eq!(r_tag, REASON_TAG_MATCH);
        // 关注板块 + 标签同时命中：理由取更强的「关注板块」。
        let profile2 = profile_with(&["b-x"], &[], &[("t1", 2)]);
        let (_, r_both) = score_candidate(&tagged, &profile2, &index, NOW);
        assert_eq!(r_both, REASON_FOLLOWED_BOARD);
    }

    #[test]
    fn freshness_decays_and_interacted_board_scores() {
        let profile = profile_with(&[], &["b-x"], &[]);
        let fresh = row("b-x", "alice", NOW - HOUR, 0, 0);
        let stale = row("b-x", "alice", NOW - 30 * 24 * HOUR, 0, 0);
        let (s_fresh, _) = score_candidate(&fresh, &profile, &HashMap::new(), NOW);
        let (s_stale, _) = score_candidate(&stale, &profile, &HashMap::new(), NOW);
        assert!(s_fresh > s_stale, "同一板块下新帖必须排在旧帖前");
        let (_, r) = score_candidate(&fresh, &profile, &HashMap::new(), NOW);
        assert_eq!(r, REASON_INTERACTED_BOARD);
    }

    #[test]
    fn cold_start_is_trending_only() {
        let profile = InterestProfile::default();
        let (s, r) = score_candidate(
            &row("b-x", "a", NOW - HOUR, 0, 0),
            &profile,
            &HashMap::new(),
            NOW,
        );
        // 纯热度+新鲜度：无画像时仍有非零分（engage_sat + fresh）。
        assert!(s > 0.0);
        assert_eq!(r, REASON_TRENDING);
    }
}
