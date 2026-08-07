//! M10-VIDEO 集成测试：分类/出站/HLS/CSP/西瓜（纯函数）与
//! resolve→create→get→patch→delete→refresh 状态机（SQLite 真库）。
//!
//! 全部走线上代码真实入口（`bblbb_backend::video::*`），不重实现被测逻辑。
//! 网络访问一律经 [`bblbb_backend::video::egress::FetchClient`] 抽象用 mock
//! 注入，不发起真实外部请求。

use std::net::IpAddr;
use std::path::{Path, PathBuf};

use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use bblbb_backend::video::classify::{classify, is_allowed_host, validate_url_shape};
use bblbb_backend::video::csp::render_for;
use bblbb_backend::video::egress::{
    egress_validate, EgressLimits, FetchClient, FetchError, FetchRequest, FetchedResponse,
};
use bblbb_backend::video::hls::{check_totals, initial_budget, parse_playlist, HlsLimits};
use bblbb_backend::video::policy::validate_host_list;
use bblbb_backend::video::xigua::{extract_video_id, iframe_url, is_xigua_host, XiguaHosts};
use bblbb_backend::video::{
    create_embed, delete_embed, get_embed, load_policy, recheck_references, refresh_embed,
    resolve_source, update_provider_policy, Provider, VideoError, VideoTarget,
};
use serde_json::json;
use sqlx::Either;

// ─────────────────────────── 纯函数：分类（M10-VIDEO-02/03/04）───────────

#[test]
fn classifies_mp4_webm_ogv_mov_hls_and_xigua() {
    let mp4 = classify("https://cdn.example.com/a.mp4").unwrap();
    assert_eq!(mp4.provider, Provider::Direct);
    assert_eq!(mp4.media_type.as_deref(), Some("video/mp4"));

    let webm = classify("https://cdn.example.com/a.webm").unwrap();
    assert_eq!(webm.provider, Provider::Direct);
    assert_eq!(webm.media_type.as_deref(), Some("video/webm"));

    let ogv = classify("https://cdn.example.com/a.ogv").unwrap();
    assert_eq!(ogv.provider, Provider::Direct);
    assert_eq!(ogv.media_type.as_deref(), Some("video/ogg"));

    let mov = classify("https://cdn.example.com/a.mov").unwrap();
    assert_eq!(mov.provider, Provider::Direct);

    let hls = classify("https://cdn.example.com/live/index.m3u8").unwrap();
    assert_eq!(hls.provider, Provider::Hls);

    let xigua = classify("https://www.ixigua.com/video/7301234567890123456").unwrap();
    assert_eq!(xigua.provider, Provider::Xigua);
    assert_eq!(xigua.external_id.as_deref(), Some("7301234567890123456"));
    // 官方公开页面规范化，不保留多余路径。
    assert_eq!(
        xigua.official_url,
        "https://www.ixigua.com/video/7301234567890123456"
    );
}

#[test]
fn rejects_signed_urls_userinfo_private_ip_and_bad_ports() {
    // 签名/凭证 URL 一律拒绝且永不回显（M10-VIDEO-03）。
    assert!(classify("https://cdn.example.com/a.mp4?X-Amz-Signature=deadbeef").is_err());
    assert!(classify("https://cdn.example.com/a.mp4?token=abc").is_err());
    assert!(classify("https://cdn.example.com/a.mp4?Expires=1&Signature=x").is_err());
    // userinfo / 非 443 端口 / fragment。
    assert!(classify("https://u:p@cdn.example.com/a.mp4").is_err());
    assert!(classify("https://cdn.example.com:8443/a.mp4").is_err());
    assert!(classify("https://cdn.example.com/a.mp4#frag").is_err());
    // 私网 IPv4/IPv6 与数字混淆（M10-VIDEO-04）。
    assert!(classify("https://10.0.0.5/a.mp4").is_err());
    assert!(classify("https://[::1]/a.mp4").is_err());
    assert!(classify("https://[::ffff:192.168.1.1]/a.mp4").is_err());
    assert!(classify("https://2130706433/a.mp4").is_err());
    // 非 https scheme。
    assert!(classify("http://cdn.example.com/a.mp4").is_err());
    assert!(classify("data:text/html;base64,xx").is_err());
}

#[test]
fn idn_hosts_normalize_to_punycode() {
    let url = validate_url_shape("https://例え.jp/休憩/小.mp4").unwrap();
    assert!(url.host.contains("xn--"), "IDN 必须归一化为 punycode");
    let c = classify("https://例え.jp/休憩/小.mp4").unwrap();
    assert_eq!(c.host, url.host);
}

#[test]
fn allowlist_matches_subdomains_and_blocks_others() {
    let list = validate_host_list(&["example.com".into(), "*.cdn.example.net".into()]).unwrap();
    assert!(is_allowed_host("example.com", &list));
    assert!(is_allowed_host("sub.example.com", &list));
    assert!(is_allowed_host("cdn.example.net", &list));
    assert!(!is_allowed_host("evil.com", &list));
    assert!(!is_allowed_host("example.net", &list));
}

// ─────────────────────────── 纯函数：出站（M10-VIDEO-04/10）───────────

fn limits() -> EgressLimits {
    EgressLimits {
        max_redirects: 3,
        max_response_bytes: 1024,
        timeout_ms: 15_000,
    }
}

#[test]
fn egress_rejects_redirects_private_ip_and_oversize() {
    // 开放重定向（跳数超限）。
    let mut resp = FetchedResponse {
        status: 200,
        final_url: "https://cdn.example.com/a.mp4".into(),
        resolved_ips: vec![IpAddr::from([8, 8, 8, 8])],
        content_type: Some("video/mp4".into()),
        content_length: None,
        body: vec![0; 16],
        hop_count: 4,
    };
    assert_eq!(
        egress_validate(&limits(), &resp),
        Err(bblbb_backend::video::egress::EgressError::TooManyRedirects)
    );

    // DNS 重绑定：任一解析 IP 私网即拒（M10-VIDEO-10）。
    resp.hop_count = 0;
    resp.resolved_ips = vec![IpAddr::from([8, 8, 8, 8]), IpAddr::from([10, 0, 0, 1])];
    assert!(matches!(
        egress_validate(&limits(), &resp),
        Err(bblbb_backend::video::egress::EgressError::PrivateIp(_))
    ));

    // 超大响应（MIME 欺骗防线之后的传输级兜底）。
    resp.resolved_ips = vec![IpAddr::from([8, 8, 8, 8])];
    resp.body = vec![0u8; 2048];
    assert!(matches!(
        egress_validate(&limits(), &resp),
        Err(bblbb_backend::video::egress::EgressError::ResponseTooLarge(
            _
        ))
    ));

    // 正常响应通过。
    resp.body = vec![0u8; 16];
    assert!(egress_validate(&limits(), &resp).is_ok());
}

// ─────────────────────────── 纯函数：HLS 解析预算（M10-VIDEO-05/10）───────────

fn hls_limits() -> HlsLimits {
    HlsLimits {
        max_depth: 3,
        max_segments: 4,
        max_playlist_bytes: 64 * 1024,
        max_duration_ms: 60_000,
        allow_cross_origin: false,
    }
}

fn base_url() -> &'static str {
    "https://cdn.example.com/live/index.m3u8"
}

#[test]
fn hls_bounds_segments_duration_and_depth() {
    // 分片总数超限：解析过程中预算耗尽即拒绝（M10-VIDEO-05）。
    let mut b = initial_budget(&hls_limits());
    let text = "#EXTM3U\n#EXTINF:10,\na1.ts\na2.ts\na3.ts\na4.ts\na5.ts\n";
    assert!(matches!(
        parse_playlist(text, base_url(), &hls_limits(), &mut b),
        Err(bblbb_backend::video::hls::HlsError::SegmentCountExceeded)
    ));

    // 累计时长超限：单分片超过总时长预算即拒绝。
    let mut b = initial_budget(&hls_limits());
    let text = "#EXTM3U\n#EXTINF:70000,\na.ts\n";
    assert!(matches!(
        parse_playlist(text, base_url(), &hls_limits(), &mut b),
        Err(bblbb_backend::video::hls::HlsError::DurationExceeded)
    ));

    // master 树深度超限：超过深度预算的 variant 触发 DepthExceeded。
    let mut b = initial_budget(&hls_limits());
    let master = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1280000\nv1.m3u8\n#EXT-X-STREAM-INF:BANDWIDTH=2560000\nv2.m3u8\n#EXT-X-STREAM-INF:BANDWIDTH=5120000\nv3.m3u8\n#EXT-X-STREAM-INF:BANDWIDTH=1024000\nv4.m3u8\n";
    assert!(matches!(
        parse_playlist(master, base_url(), &hls_limits(), &mut b),
        Err(bblbb_backend::video::hls::HlsError::DepthExceeded)
    ));

    // 合法媒体 playlist 的累计计数可复核（check_totals 通过）。
    let mut b = initial_budget(&hls_limits());
    let text = "#EXTM3U\n#EXTINF:5,\na.ts\n#EXTINF:5,\nb.ts\n#EXT-X-ENDLIST\n";
    let parsed = parse_playlist(text, base_url(), &hls_limits(), &mut b).unwrap();
    assert!(check_totals(parsed.segments.len(), parsed.duration_ms, &hls_limits()).is_ok());
}

#[test]
fn hls_rejects_external_key_map_cross_origin_and_signed_segment() {
    // EXT-X-KEY 非 NONE：外部密钥 URI 不落库不转发。
    let mut b = initial_budget(&hls_limits());
    let text =
        "#EXTM3U\n#EXT-X-KEY:METHOD=AES-128,URI=\"https://keys.example/k\"\n#EXTINF:1,\na.ts\n";
    assert!(matches!(
        parse_playlist(text, base_url(), &hls_limits(), &mut b),
        Err(bblbb_backend::video::hls::HlsError::KeyNotAllowed)
    ));

    // 跨源分片（默认禁止）。
    let mut b = initial_budget(&hls_limits());
    let text = "#EXTM3U\n#EXTINF:1,\nhttps://evil.example.com/a.ts\n";
    assert!(matches!(
        parse_playlist(text, base_url(), &hls_limits(), &mut b),
        Err(bblbb_backend::video::hls::HlsError::CrossOriginSegment)
    ));

    // 分片 URI 带签名/凭证参数（M10-VIDEO-05 签名泄漏防线）。
    let mut b = initial_budget(&hls_limits());
    let text = "#EXTM3U\n#EXTINF:1,\nseg.ts?token=abc\n";
    assert!(matches!(
        parse_playlist(text, base_url(), &hls_limits(), &mut b),
        Err(bblbb_backend::video::hls::HlsError::SignedUri)
    ));

    // 合法媒体 playlist 通过。
    let mut b = initial_budget(&hls_limits());
    let text = "#EXTM3U\n#EXTINF:5,\na.ts\n#EXTINF:5,\nb.ts\n#EXT-X-ENDLIST\n";
    let parsed = parse_playlist(text, base_url(), &hls_limits(), &mut b).unwrap();
    assert_eq!(parsed.segments.len(), 2);
    assert_eq!(parsed.duration_ms, 10_000);
}

// ─────────────────────────── 纯函数：西瓜（M10-VIDEO-06）───────────

#[test]
fn xigua_only_official_hosts_and_public_pages() {
    assert!(is_xigua_host("www.ixigua.com"));
    assert!(is_xigua_host("m.ixigua.com"));
    assert!(!is_xigua_host("ixigua.example.com"));
    assert!(!is_xigua_host("evil.com"));

    // 官方嵌入 Host 生成 iframe URL（`/iframe/{id}`，非 /i{id} 猜测）。
    assert_eq!(
        iframe_url("7301234567890123456"),
        format!(
            "https://{}/iframe/7301234567890123456",
            XiguaHosts::EMBED_HOST
        )
    );
    // 视频 id 提取只接受纯数字公开页面。
    assert_eq!(
        extract_video_id("/video/7301234567890123456"),
        Some("7301234567890123456".into())
    );
    assert_eq!(extract_video_id("/video/abc"), None);
}

// ─────────────────────────── 纯函数：CSP 渲染（M10-VIDEO-07）───────────

#[test]
fn csp_renders_direct_hls_and_xigua_with_least_privilege() {
    // direct：只放行来源 origin 的 media/connect。
    let p = render_for(
        "ready",
        &Provider::Direct,
        Some("cdn.example.com"),
        None,
        true,
        true,
        true,
    );
    assert_eq!(p.mode, "direct_player");
    assert_eq!(p.csp.media_src, vec!["https://cdn.example.com"]);
    assert!(p.csp.frame_src.is_empty() && p.csp.script_src.is_empty());
    assert_eq!(p.csp.referrer_policy, "no-referrer");

    // xigua：只允许官方嵌入 Host 的 frame-src + 最小 sandbox，无 autoplay/camera/mic。
    let p = render_for(
        "ready",
        &Provider::Xigua,
        Some("www.ixigua.com"),
        Some("7301234567890123456"),
        true,
        true,
        true,
    );
    assert_eq!(p.mode, "xigua_iframe");
    assert_eq!(
        p.csp.frame_src,
        vec![format!("https://{}", XiguaHosts::EMBED_HOST)]
    );
    assert!(
        p.csp
            .sandbox
            .iter()
            .all(|t| t != "allow-autoplay" && t != "allow-camera" && t != "allow-microphone"),
        "sandbox 不得含 autoplay/camera/mic"
    );

    // 目标不可见（隐藏/审核/删除）或 blocked → mode=none，无任何 URL。
    for status in ["blocked", "removed", "pending", "error"] {
        let p = render_for(
            status,
            &Provider::Direct,
            Some("cdn.example.com"),
            None,
            true,
            true,
            false,
        );
        assert_eq!(p.mode, "none", "status={status}");
        assert!(p.csp.media_src.is_empty() && p.iframe_url.is_none());
    }
    let p = render_for(
        "ready",
        &Provider::Xigua,
        Some("www.ixigua.com"),
        Some("id"),
        true,
        false,
        true,
    );
    assert_eq!(p.mode, "external_link", "禁止 iframe 嵌入时降级官方外链");
}

// ─────────────────────────── 状态机（SQLite 真库，M10-VIDEO-08/09/11）───────────

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-video-{}", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", dir.display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(
        &Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../migrations/sqlite"),
    )
    .unwrap();
    run_migrations(&pool, &files).await.unwrap();
    bblbb_backend::authz::roles::seed_builtin_roles(&pool)
        .await
        .unwrap();
    (pool, dir)
}

fn cleanup(dir: &Path) {
    let _ = std::fs::remove_file(dir);
    let _ = std::fs::remove_file(format!("{}-wal", dir.display()));
    let _ = std::fs::remove_file(format!("{}-shm", dir.display()));
}

async fn close_pool(pool: &DatabasePool) {
    match pool {
        Either::Left(p) => p.close().await,
        Either::Right(p) => p.close().await,
    }
}

async fn insert_user(pool: &DatabasePool, tag: &str) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    let sql = "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, level, email_verified, email_verified_at, created_at, updated_at)
               VALUES (?, ?, ?, 'dummy', 'active', 1, 1, ?, ?, ?)";
    match pool {
        Either::Left(p) => {
            sqlx::query(sql)
                .bind(&user_id)
                .bind(format!("{tag}_{}", uuid::Uuid::now_v7().simple()))
                .bind(format!(
                    "{tag}_{}@example.com",
                    uuid::Uuid::now_v7().simple()
                ))
                .bind(now - 30 * 86_400 * 1000)
                .bind(now)
                .bind(now)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(p) => {
            sqlx::query(sql)
                .bind(&user_id)
                .bind(format!("{tag}_{}", uuid::Uuid::now_v7().simple()))
                .bind(format!(
                    "{tag}_{}@example.com",
                    uuid::Uuid::now_v7().simple()
                ))
                .bind(now - 30 * 86_400 * 1000)
                .bind(now)
                .bind(now)
                .execute(p)
                .await
                .unwrap();
        }
    }
    user_id
}

async fn insert_post(pool: &DatabasePool, author_id: &str) -> String {
    let post_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    let board_id: String = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT id FROM boards WHERE slug = 'general'")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(p) => sqlx::query_scalar("SELECT id FROM boards WHERE slug = 'general'")
            .fetch_one(p)
            .await
            .unwrap(),
    };
    let sql = "INSERT INTO posts (id, board_id, author_id, post_type, title, content, status, visibility, version, published_at, created_at, updated_at)
               VALUES (?, ?, ?, 'article', 't', '正文', 'published', 'public', 1, ?, ?, ?)";
    match pool {
        Either::Left(p) => {
            sqlx::query(sql)
                .bind(&post_id)
                .bind(board_id)
                .bind(author_id)
                .bind(now)
                .bind(now)
                .bind(now)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(p) => {
            sqlx::query(sql)
                .bind(&post_id)
                .bind(board_id)
                .bind(author_id)
                .bind(now)
                .bind(now)
                .bind(now)
                .execute(p)
                .await
                .unwrap();
        }
    }
    post_id
}

/// 启用 Direct Provider 策略（allow_hosts 放行 cdn.example.com）。
async fn enable_direct_policy(pool: &DatabasePool) -> i64 {
    let current = load_policy(pool, Provider::Direct).await.unwrap();
    let (_, _) = update_provider_policy(
        pool,
        Provider::Direct,
        &json!({
            "enabled": true,
            "allow_hosts": ["example.com", "*.cdn.example.com"],
            "max_redirects": 3,
            "max_response_bytes": 5 * 1024 * 1024,
        }),
        current.version,
        now_millis(),
    )
    .await
    .unwrap();
    load_policy(pool, Provider::Direct).await.unwrap().version
}

/// 模拟 egress 客户端：按 URL 返回可配置响应（MIME 欺骗/Range/超时/下架）。
struct MockClient {
    status: u16,
    content_type: Option<&'static str>,
    body: Vec<u8>,
    resolved_ips: Vec<IpAddr>,
}

impl FetchClient for MockClient {
    fn fetch(
        &self,
        _req: FetchRequest,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<FetchedResponse, FetchError>> + Send + '_>,
    > {
        let status = self.status;
        let content_type = self.content_type;
        let body = self.body.clone();
        let resolved_ips = self.resolved_ips.clone();
        Box::pin(async move {
            Ok(FetchedResponse {
                status,
                final_url: _req.url.clone(),
                resolved_ips,
                content_type: content_type.map(str::to_string),
                content_length: Some(body.len() as i64),
                body,
                hop_count: 0,
            })
        })
    }
}

#[tokio::test]
async fn resolve_create_get_delete_lifecycle() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user_id = insert_user(&pool, "lifecycle").await;
    let post_id = insert_post(&pool, &user_id).await;
    let policy_version = enable_direct_policy(&pool).await;
    let now = now_millis();

    // resolve：只返回短效 resolution_id 与安全元数据，绝不回显 source 之外的
    // 敏感字段（M10-VIDEO-03）。
    let resolved = resolve_source(
        &pool,
        &user_id,
        "https://cdn.example.com/vid/a.mp4",
        &VideoTarget {
            target_type: "post".into(),
            target_id: post_id.clone(),
        },
        now,
    )
    .await
    .unwrap();
    assert_eq!(resolved.provider, "direct");
    assert_eq!(resolved.policy_version, policy_version);
    assert_eq!(resolved.official_url, "https://cdn.example.com/vid/a.mp4");

    // create：消费 resolution_id 创建 pending embed。
    let created = create_embed(
        &pool,
        &user_id,
        &resolved.resolution_id,
        &VideoTarget {
            target_type: "post".into(),
            target_id: post_id.clone(),
        },
        policy_version,
        now,
    )
    .await
    .unwrap();
    assert_eq!(created.status, "pending");
    assert_eq!(created.user_id, user_id);
    assert_eq!(created.target.target_id, post_id);

    // get：当前请求方可见投影。
    let view = get_embed(&pool, &user_id, &created.id, now).await.unwrap();
    assert_eq!(view.id, created.id);
    assert_eq!(view.provider, "direct");

    // resolution_id 一次性：重复创建 → ResolutionExpired。
    let again = create_embed(
        &pool,
        &user_id,
        &resolved.resolution_id,
        &VideoTarget {
            target_type: "post".into(),
            target_id: post_id.clone(),
        },
        policy_version,
        now,
    )
    .await;
    assert!(matches!(again, Err(VideoError::ResolutionExpired)));

    // 已发布内容引用的 embed 不可删除（M10-VIDEO-11 历史引用防线）。
    let blocked = delete_embed(&pool, &user_id, &created.id, now).await;
    assert!(matches!(blocked, Err(VideoError::EmbedReferenced)));

    // 目标改为未发布（draft）后允许删除 → removed；再 get → EmbedNotFound。
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE posts SET status = 'draft' WHERE id = ?")
                .bind(&post_id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(p) => {
            sqlx::query("UPDATE posts SET status = 'draft' WHERE id = ?")
                .bind(&post_id)
                .execute(p)
                .await
                .unwrap();
        }
    }
    delete_embed(&pool, &user_id, &created.id, now)
        .await
        .unwrap();
    assert!(matches!(
        get_embed(&pool, &user_id, &created.id, now).await,
        Err(VideoError::EmbedNotFound)
    ));

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn create_enforces_owner_provider_and_policy_gates() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user_id = insert_user(&pool, "gates").await;
    let other_id = insert_user(&pool, "other").await;
    let post_id = insert_post(&pool, &user_id).await;
    let now = now_millis();

    // Provider 未启用：resolve 直接拒绝（M10-VIDEO-09 降级语义的入站一侧）。
    let disabled = resolve_source(
        &pool,
        &user_id,
        "https://cdn.example.com/a.mp4",
        &VideoTarget {
            target_type: "post".into(),
            target_id: post_id.clone(),
        },
        now,
    )
    .await;
    assert!(matches!(disabled, Err(VideoError::ProviderDisabled)));

    let policy_version = enable_direct_policy(&pool).await;

    // 目标不存在/不属于当前用户 → 拒绝（M10-VIDEO-11 帖子权限）。
    let resolved = resolve_source(
        &pool,
        &user_id,
        "https://cdn.example.com/a.mp4",
        &VideoTarget {
            target_type: "post".into(),
            target_id: post_id.clone(),
        },
        now,
    )
    .await
    .unwrap();
    let not_found = create_embed(
        &pool,
        &user_id,
        &resolved.resolution_id,
        &VideoTarget {
            target_type: "post".into(),
            target_id: uuid::Uuid::now_v7().to_string(),
        },
        policy_version,
        now,
    )
    .await;
    assert!(matches!(not_found, Err(VideoError::TargetNotFound)));

    let _resolved2 = resolve_source(
        &pool,
        &user_id,
        "https://cdn.example.com/a.mp4",
        &VideoTarget {
            target_type: "post".into(),
            target_id: post_id.clone(),
        },
        now,
    )
    .await
    .unwrap();
    // 其他用户用自己的 resolution 创建到不属于自己的帖 → TargetForbidden。
    let other_resolved = resolve_source(
        &pool,
        &other_id,
        "https://cdn.example.com/a.mp4",
        &VideoTarget {
            target_type: "post".into(),
            target_id: post_id.clone(),
        },
        now,
    )
    .await
    .unwrap();
    let forbidden = create_embed(
        &pool,
        &other_id,
        &other_resolved.resolution_id,
        &VideoTarget {
            target_type: "post".into(),
            target_id: post_id.clone(),
        },
        policy_version,
        now,
    )
    .await;
    assert!(matches!(forbidden, Err(VideoError::TargetForbidden)));

    // 创建时策略版本冲突 → 必须重新 resolve（M10-VIDEO-08 PolicyChanged）。
    let resolved3 = resolve_source(
        &pool,
        &user_id,
        "https://cdn.example.com/a.mp4",
        &VideoTarget {
            target_type: "post".into(),
            target_id: post_id.clone(),
        },
        now,
    )
    .await
    .unwrap();
    let conflict = create_embed(
        &pool,
        &user_id,
        &resolved3.resolution_id,
        &VideoTarget {
            target_type: "post".into(),
            target_id: post_id.clone(),
        },
        policy_version - 1,
        now,
    )
    .await;
    assert!(matches!(
        conflict,
        Err(VideoError::PolicyVersionConflict { .. })
    ));

    // Host 不在 allowlist → resolve 拒绝（M10-VIDEO-04）。
    let host_blocked = resolve_source(
        &pool,
        &user_id,
        "https://evil.com/a.mp4",
        &VideoTarget {
            target_type: "post".into(),
            target_id: post_id.clone(),
        },
        now,
    )
    .await;
    assert!(matches!(host_blocked, Err(VideoError::HostNotAllowed(_))));

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn refresh_mime_spoof_sets_error_and_keeps_external_link() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user_id = insert_user(&pool, "mime").await;
    let post_id = insert_post(&pool, &user_id).await;
    let policy_version = enable_direct_policy(&pool).await;
    let now = now_millis();

    let resolved = resolve_source(
        &pool,
        &user_id,
        "https://cdn.example.com/a.mp4",
        &VideoTarget {
            target_type: "post".into(),
            target_id: post_id.clone(),
        },
        now,
    )
    .await
    .unwrap();
    let created = create_embed(
        &pool,
        &user_id,
        &resolved.resolution_id,
        &VideoTarget {
            target_type: "post".into(),
            target_id: post_id.clone(),
        },
        policy_version,
        now,
    )
    .await
    .unwrap();

    // MIME 欺骗：.mp4 由 text/html 提供 → refresh 失败但不删除 embed，降级外链。
    let spoof = MockClient {
        status: 200,
        content_type: Some("text/html"),
        body: b"<html>not a video</html>".to_vec(),
        resolved_ips: vec![IpAddr::from([8, 8, 8, 8])],
    };
    refresh_embed(&pool, &user_id, &created.id, &spoof, now)
        .await
        .unwrap();
    let after = get_embed(&pool, &user_id, &created.id, now).await.unwrap();
    assert_eq!(after.error_class.as_deref(), Some("video_mime_mismatch"));
    // 官方外链保留（降级卡片仍可跳转），不渲染播放器。
    assert!(after.render.mode != "direct_player");
    assert!(after.official_url.is_some());

    // Provider 下架（404/410）→ 同样降级为 takedown。
    let takedown = MockClient {
        status: 404,
        content_type: Some("text/html"),
        body: Vec::new(),
        resolved_ips: vec![IpAddr::from([8, 8, 8, 8])],
    };
    refresh_embed(&pool, &user_id, &created.id, &takedown, now)
        .await
        .unwrap();
    let after2 = get_embed(&pool, &user_id, &created.id, now).await.unwrap();
    assert_eq!(after2.error_class.as_deref(), Some("video_takedown"));

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn policy_change_rechecks_references_and_degrades() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user_id = insert_user(&pool, "recheck").await;
    let post_id = insert_post(&pool, &user_id).await;
    let policy_version = enable_direct_policy(&pool).await;
    let now = now_millis();

    let resolved = resolve_source(
        &pool,
        &user_id,
        "https://cdn.example.com/a.mp4",
        &VideoTarget {
            target_type: "post".into(),
            target_id: post_id.clone(),
        },
        now,
    )
    .await
    .unwrap();
    let created = create_embed(
        &pool,
        &user_id,
        &resolved.resolution_id,
        &VideoTarget {
            target_type: "post".into(),
            target_id: post_id.clone(),
        },
        policy_version,
        now,
    )
    .await
    .unwrap();
    assert_eq!(created.status, "pending");

    // 管理员禁用 Provider → recheck 将历史引用降级为外链（不阻塞发帖）。
    let current = load_policy(&pool, Provider::Direct).await.unwrap();
    update_provider_policy(
        &pool,
        Provider::Direct,
        &json!({ "enabled": false }),
        current.version,
        now,
    )
    .await
    .unwrap();
    let changed = recheck_references(&pool, Provider::Direct, now)
        .await
        .unwrap();
    assert!(changed >= 1, "禁用后历史引用必须被重检降级");

    let after = get_embed(&pool, &user_id, &created.id, now).await.unwrap();
    assert_eq!(
        after.error_class.as_deref(),
        Some("video_provider_disabled")
    );
    assert!(after.render.mode != "direct_player");

    close_pool(&pool).await;
    cleanup(&dir);
}
