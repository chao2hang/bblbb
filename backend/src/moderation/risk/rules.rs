//! 风险规则实现（M05-RISK-02）：新用户前 N 帖、链接数、重复内容、
//! 敏感词与频率规则。
//!
//! 每条规则返回命中类别（`Option<ReasonCategory>`）；`run_rules` 按固定顺序
//! 执行并返回首个命中，保证测试与线上行为一致。重复内容指纹基于归一化正文
//! 的 SHA-256，只比较特征不存储正文。

use sha2::{Digest, Sha256};
use sqlx::Either;

use crate::db::DatabasePool;

use super::policy::{ReasonCategory, RiskInput, Thresholds};

/// 归一化正文指纹（小写 + 仅保留字母数字，用于重复内容比较）。
pub fn body_fingerprint(markdown: &str) -> String {
    let normalized: String = markdown
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect();
    let digest = Sha256::digest(normalized.as_bytes());
    format!("{:x}", digest)
}

/// 统计正文中的链接数（Markdown 链接 + 裸 URL）。
pub fn count_links(markdown: &str) -> u32 {
    let mut count = 0u32;
    // [text](url) 形式
    let mut rest = markdown;
    while let Some(start) = rest.find("](") {
        count += 1;
        rest = &rest[start + 2..];
    }
    // 裸 URL（http/https 开头，且不在前一种匹配的残留里重复计数）
    for token in markdown.split_whitespace() {
        let lowered = token.to_lowercase();
        if (lowered.starts_with("http://") || lowered.starts_with("https://"))
            && !lowered.contains("](")
        {
            count += 1;
        }
    }
    count
}

/// 命中任一敏感词 → `Sensitive`（只给类别，不暴露命中的词）。
pub fn sensitive_word_rule(thresholds: &Thresholds, input: &RiskInput) -> Option<ReasonCategory> {
    if thresholds.sensitive_words.is_empty() {
        return None;
    }
    let body = input.body_markdown.to_lowercase();
    let title = input.title.to_lowercase();
    let hit = thresholds
        .sensitive_words
        .iter()
        .any(|w| !w.is_empty() && (body.contains(w) || title.contains(w)));
    hit.then_some(ReasonCategory::Sensitive)
}

/// 链接数超过阈值 → `LinkHeavy`。
pub fn link_rule(thresholds: &Thresholds, input: &RiskInput) -> Option<ReasonCategory> {
    (count_links(&input.body_markdown) > thresholds.max_links).then_some(ReasonCategory::LinkHeavy)
}

/// 新用户（账号存在时长 < 窗口）累计发帖 ≥ 阈值 → `NewUser`。
pub async fn new_user_rule(
    pool: &DatabasePool,
    thresholds: &Thresholds,
    input: &RiskInput,
) -> Result<Option<ReasonCategory>, sqlx::Error> {
    let Some(created_at) = input.author_created_at else {
        return Ok(None);
    };
    let age_ms = input.now.saturating_sub(created_at);
    if age_ms > thresholds.new_user_grace_secs.saturating_mul(1_000) {
        return Ok(None);
    }
    let count: i64 = match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM posts WHERE author_id = ? AND status = 'published' AND deleted_at IS NULL",
        )
        .bind(&input.author_id)
        .fetch_one(p)
        .await?,
        Either::Right(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM posts WHERE author_id = ? AND status = 'published' AND deleted_at IS NULL",
        )
        .bind(&input.author_id)
        .fetch_one(p)
        .await?,
    };
    Ok((count >= i64::from(thresholds.new_user_max_posts)).then_some(ReasonCategory::NewUser))
}

/// 频率窗口内发帖 ≥ 阈值 → `Frequency`。
pub async fn frequency_rule(
    pool: &DatabasePool,
    thresholds: &Thresholds,
    input: &RiskInput,
) -> Result<Option<ReasonCategory>, sqlx::Error> {
    let window_start = input
        .now
        .saturating_sub(thresholds.frequency_window_secs.saturating_mul(1_000));
    let count: i64 = match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM posts WHERE author_id = ? AND status = 'published' AND deleted_at IS NULL AND created_at >= ?",
        )
        .bind(&input.author_id)
        .bind(window_start)
        .fetch_one(p)
        .await?,
        Either::Right(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM posts WHERE author_id = ? AND status = 'published' AND deleted_at IS NULL AND created_at >= ?",
        )
        .bind(&input.author_id)
        .bind(window_start)
        .fetch_one(p)
        .await?,
    };
    Ok((count >= i64::from(thresholds.max_frequency_posts)).then_some(ReasonCategory::Frequency))
}

/// 重复内容：窗口内存在其他作者的相同正文指纹 → `Duplicate`。
///
/// 有界扫描（最多 500 行），指纹在内存比较，不存储正文。
pub async fn duplicate_rule(
    pool: &DatabasePool,
    thresholds: &Thresholds,
    input: &RiskInput,
) -> Result<Option<ReasonCategory>, sqlx::Error> {
    let window_start = input
        .now
        .saturating_sub(thresholds.duplicate_window_secs.saturating_mul(1_000));
    let mine = body_fingerprint(&input.body_markdown);
    let rows: Vec<(String, String)> = match pool {
        Either::Left(p) => {
            sqlx::query_as(
                "SELECT p.author_id, c.body_markdown
             FROM posts p
             JOIN post_contents c ON c.post_id = p.id
             WHERE p.status = 'published' AND p.deleted_at IS NULL
               AND p.created_at >= ?
             ORDER BY p.created_at DESC LIMIT 500",
            )
            .bind(window_start)
            .fetch_all(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_as(
                "SELECT p.author_id, c.body_markdown
             FROM posts p
             JOIN post_contents c ON c.post_id = p.id
             WHERE p.status = 'published' AND p.deleted_at IS NULL
               AND p.created_at >= ?
             ORDER BY p.created_at DESC LIMIT 500",
            )
            .bind(window_start)
            .fetch_all(p)
            .await?
        }
    };
    let dup = rows
        .into_iter()
        .any(|(author_id, body)| author_id != input.author_id && body_fingerprint(&body) == mine);
    Ok(dup.then_some(ReasonCategory::Duplicate))
}

/// 按固定顺序执行全部规则，返回首个命中类别。
pub async fn run_rules(
    pool: &DatabasePool,
    thresholds: &Thresholds,
    input: &RiskInput,
) -> Result<Option<ReasonCategory>, sqlx::Error> {
    if let Some(cat) = new_user_rule(pool, thresholds, input).await? {
        return Ok(Some(cat));
    }
    if let Some(cat) = link_rule(thresholds, input) {
        return Ok(Some(cat));
    }
    if let Some(cat) = sensitive_word_rule(thresholds, input) {
        return Ok(Some(cat));
    }
    if let Some(cat) = frequency_rule(pool, thresholds, input).await? {
        return Ok(Some(cat));
    }
    duplicate_rule(pool, thresholds, input).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::moderation::risk::policy::Thresholds;

    fn base_thresholds() -> Thresholds {
        Thresholds {
            new_user_max_posts: 3,
            new_user_grace_secs: 7 * 86_400,
            max_links: 3,
            sensitive_words: vec![],
            max_frequency_posts: 10,
            frequency_window_secs: 3_600,
            duplicate_window_secs: 7 * 86_400,
        }
    }

    #[test]
    fn fingerprint_normalizes_case_and_punctuation() {
        assert_eq!(
            body_fingerprint("Hello, World!!"),
            body_fingerprint("hello world")
        );
        assert_ne!(
            body_fingerprint("alpha beta"),
            body_fingerprint("beta gamma")
        );
    }

    #[test]
    fn link_count_markdown_and_bare() {
        assert_eq!(count_links("[a](https://x.com) [b](http://y.io)"), 2);
        assert_eq!(count_links("see https://x.com and http://y.io now"), 2);
        assert_eq!(count_links("no links here"), 0);
    }

    #[test]
    fn link_rule_flags_only_above_threshold() {
        let t = base_thresholds();
        let input = RiskInput {
            author_id: "a".into(),
            author_created_at: None,
            author_level: 1,
            board_id: "b".into(),
            title: String::new(),
            body_markdown:
                "[1](https://a.com) [2](https://b.com) [3](https://c.com) [4](https://d.com)".into(),
            now: 1_000,
        };
        assert_eq!(link_rule(&t, &input), Some(ReasonCategory::LinkHeavy));
        let input2 = RiskInput {
            body_markdown: "[1](https://a.com)".into(),
            ..input
        };
        assert_eq!(link_rule(&t, &input2), None);
    }

    #[test]
    fn sensitive_rule_matches_word_but_exposes_only_category() {
        let t = Thresholds {
            sensitive_words: vec!["banned-term".into()],
            ..base_thresholds()
        };
        let input = RiskInput {
            author_id: "a".into(),
            author_created_at: None,
            author_level: 1,
            board_id: "b".into(),
            title: String::new(),
            body_markdown: "contains banned-term here".into(),
            now: 1_000,
        };
        assert_eq!(
            sensitive_word_rule(&t, &input),
            Some(ReasonCategory::Sensitive)
        );
        let clean = RiskInput {
            body_markdown: "fine text".into(),
            ..input
        };
        assert_eq!(sensitive_word_rule(&t, &clean), None);
    }
}
