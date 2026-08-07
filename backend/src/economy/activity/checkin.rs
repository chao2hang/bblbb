//! M07-LEVELS-03/04/05：签到事件判定与用户时区日界线。
//!
//! - [`validate_visit`]：每日首次有效业务页面访问判定（M07-LEVELS-03）——
//!   排除匿名（路由层 401）、静态资源、预取（`Sec-Purpose: prefetch`、
//!   `Sec-Fetch-Dest: empty` 等）、已知爬虫/健康检查 UA 与健康检查路径；
//!   失败请求（4xx/5xx）由调用方保证不进入领取路径。
//! - 日界线（M07-LEVELS-04）：`activity_day = 本地日期 YYYY-MM-DD`，本地时区
//!   优先取 `users.timezone`；缺失/非法时回退站点时区（管理员配置），再缺省
//!   UTC，并返回时区版本 `TIMEZONE_VERSION`（固定偏移解析，`chrono_tz` 未引入，
//!   符合「如已依赖否则固定 UTC+偏移配置」的约定）。
//! - 幂等键（M07-LEVELS-05）：`{user_id}:{activity_day}:check_in`（见 service）。

use std::collections::HashSet;

use chrono::{DateTime, Days, FixedOffset, NaiveDate};
use sqlx::Either;

use crate::db::DatabasePool;

/// 时区解析版本（固定偏移；升级解析器时递增）。
pub const TIMEZONE_VERSION: &str = "tz-v1-fixed-offset";

/// 时区来源（用于「缺省回退站点时区并记录时区版本」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TzSource {
    /// 用户 `users.timezone`。
    User,
    /// 回退站点时区（管理员配置）。
    Site,
    /// 最终缺省 UTC。
    Default,
}

impl TzSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Site => "site",
            Self::Default => "default",
        }
    }
}

/// 时区解析结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TzResolution {
    /// 东八区偏移秒数（east of UTC）。
    pub offset_secs: i32,
    pub source: TzSource,
}

impl TzResolution {
    /// 时区版本（解析器版本 + 来源；响应与审计记录用）。
    pub fn version(&self) -> String {
        format!("{}:{}", TIMEZONE_VERSION, self.source.as_str())
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::json!({
            "offset_secs": self.offset_secs,
            "source": self.source.as_str(),
            "version": self.version(),
        })
    }
}

/// 常见 IANA 时区 → 固定偏移秒数（`chrono_tz` 未引入，覆盖主要站点时区）。
/// `Etc/GMT±N` 走 POSIX 反转分支单独处理。
const TZ_ALIASES: &[(&str, i32)] = &[
    ("asia/shanghai", 8 * 3600),
    ("asia/chongqing", 8 * 3600),
    ("asia/harbin", 8 * 3600),
    ("asia/urumqi", 6 * 3600),
    ("asia/hong_kong", 8 * 3600),
    ("asia/macau", 8 * 3600),
    ("asia/taipei", 8 * 3600),
    ("asia/singapore", 8 * 3600),
    ("asia/kuala_lumpur", 8 * 3600),
    ("asia/manila", 8 * 3600),
    ("asia/tokyo", 9 * 3600),
    ("asia/seoul", 9 * 3600),
    ("asia/kolkata", 5 * 3600 + 1800),
    ("asia/dhaka", 6 * 3600),
    ("asia/bangkok", 7 * 3600),
    ("asia/jakarta", 7 * 3600),
    ("asia/ho_chi_minh", 7 * 3600),
    ("asia/dubai", 4 * 3600),
    ("asia/tehran", 3 * 3600 + 1800),
    ("asia/jerusalem", 2 * 3600),
    ("europe/berlin", 3600),
    ("europe/paris", 3600),
    ("europe/amsterdam", 3600),
    ("europe/rome", 3600),
    ("europe/madrid", 3600),
    ("europe/london", 0),
    ("europe/dublin", 0),
    ("europe/lisbon", 0),
    ("europe/athens", 2 * 3600),
    ("europe/istanbul", 3 * 3600),
    ("europe/moscow", 3 * 3600),
    ("america/new_york", -5 * 3600),
    ("america/toronto", -5 * 3600),
    ("america/chicago", -6 * 3600),
    ("america/denver", -7 * 3600),
    ("america/los_angeles", -8 * 3600),
    ("america/vancouver", -8 * 3600),
    ("america/mexico_city", -6 * 3600),
    ("america/sao_paulo", -3 * 3600),
    ("america/argentina/buenos_aires", -3 * 3600),
    ("australia/sydney", 10 * 3600),
    ("australia/melbourne", 10 * 3600),
    ("australia/brisbane", 10 * 3600),
    ("australia/perth", 8 * 3600),
    ("pacific/auckland", 12 * 3600),
    ("pacific/honolulu", -10 * 3600),
];

/// 解析时区字符串 → 距 UTC 偏移秒数（east）。
///
/// 支持：`UTC`/`GMT`/`Z`、`Etc/GMT±N`（POSIX 反号）、`±HH:MM`/`±HHMM`/`±HH`、
/// `UTC±N`/`GMT±N` 与常见 IANA 名称（固定偏移别名表）。
pub fn parse_timezone_offset(tz: &str) -> Option<i32> {
    let t = tz.trim();
    if t.is_empty() {
        return None;
    }
    let lower = t.to_ascii_lowercase();
    match lower.as_str() {
        "utc" | "gmt" | "z" | "etc/utc" | "etc/gmt" | "greenwich" | "universal" => return Some(0),
        _ => {}
    }
    // Etc/GMT±N：POSIX 符号反转（Etc/GMT-8 == UTC+8）。
    if let Some(rest) = lower.strip_prefix("etc/gmt") {
        let sign: i32 = if rest.starts_with('-') { 1 } else { -1 };
        let num: i32 = rest.trim_start_matches(['+', '-']).parse().ok()?;
        if !(0..=14).contains(&num) {
            return None;
        }
        return Some(sign * num * 3600);
    }
    if let Some((_, offset)) = TZ_ALIASES.iter().find(|(alias, _)| *alias == lower) {
        return Some(*offset);
    }
    // `UTC±N` / `GMT±N` 前缀或裸 `±HH[:MM]`。
    let body = lower
        .strip_prefix("utc")
        .or_else(|| lower.strip_prefix("gmt"))
        .unwrap_or(&lower);
    let (sign, digits) = if let Some(rest) = body.strip_prefix('+') {
        (1i32, rest)
    } else {
        let rest = body.strip_prefix('-')?;
        (-1i32, rest)
    };
    let digits = digits.trim();
    let (hours, minutes) = if let Some((h, m)) = digits.split_once(':') {
        (h.parse::<i32>().ok()?, m.parse::<i32>().ok()?)
    } else if digits.len() == 4 {
        (
            digits[..2].parse::<i32>().ok()?,
            digits[2..].parse::<i32>().ok()?,
        )
    } else {
        (digits.parse::<i32>().ok()?, 0)
    };
    if !(0..=14).contains(&hours) || !(0..=59).contains(&minutes) {
        return None;
    }
    Some(sign * (hours * 3600 + minutes * 60))
}

/// 解析用户时区：用户 `users.timezone` → 站点时区 → UTC（M07-LEVELS-04）。
pub async fn resolve_user_timezone(
    pool: &DatabasePool,
    user_id: &str,
    site_timezone: &str,
) -> Result<TzResolution, sqlx::Error> {
    let user_tz: Option<Option<String>> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT timezone FROM users WHERE id = ?")
                .bind(user_id)
                .fetch_optional(p)
                .await?
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT timezone FROM users WHERE id = ?")
                .bind(user_id)
                .fetch_optional(p)
                .await?
        }
    };
    if let Some(Some(tz)) = user_tz {
        if let Some(offset) = parse_timezone_offset(&tz) {
            return Ok(TzResolution {
                offset_secs: offset,
                source: TzSource::User,
            });
        }
    }
    if let Some(offset) = parse_timezone_offset(site_timezone) {
        return Ok(TzResolution {
            offset_secs: offset,
            source: TzSource::Site,
        });
    }
    Ok(TzResolution {
        offset_secs: 0,
        source: TzSource::Default,
    })
}

/// 本地日期（YYYY-MM-DD）：`now_ms` 按 `offset_secs` 折算。
pub fn activity_day_for(offset_secs: i32, now_ms: i64) -> String {
    let offset = FixedOffset::east_opt(offset_secs)
        .unwrap_or_else(|| FixedOffset::east_opt(0).expect("fixed offset 0 is always valid"));
    let dt = DateTime::from_timestamp(now_ms / 1000, 0)
        .map(|d| d.with_timezone(&offset))
        .unwrap_or_else(|| {
            let _ = offset;
            DateTime::from_timestamp(0, 0)
                .expect("epoch timestamp is always valid")
                .with_timezone(&offset)
        });
    dt.format("%Y-%m-%d").to_string()
}

/// 前一天（YYYY-MM-DD）。
pub fn prev_day(day: &str) -> Option<String> {
    NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .ok()?
        .checked_sub_days(Days::new(1))
        .map(|d| d.format("%Y-%m-%d").to_string())
}

// ─── 访问判定（M07-LEVELS-03）──────────────────────────────────────────

/// 访问拒绝原因（安全原因对外统一为「不可领取」，不暴露风控细节）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisitRejection {
    /// 缺少/非法 path。
    Malformed,
    /// 非业务页面。
    NotBusinessPage,
    /// 静态资源/预取/爬虫/健康检查路径/健康检查 UA。
    NotEligible,
}

/// 访问上下文（路由层从请求头提取）。
#[derive(Debug, Clone)]
pub struct VisitContext<'a> {
    pub path: &'a str,
    pub user_agent: Option<&'a str>,
    /// `Sec-Purpose` 头（如 `prefetch`/`preview`）。
    pub sec_purpose: Option<&'a str>,
    /// `Purpose`/`X-Purpose` 头。
    pub purpose: Option<&'a str>,
    /// `Sec-Fetch-Dest` 头（`empty` 常见于预取）。
    pub sec_fetch_dest: Option<&'a str>,
}

/// 业务页面路径前缀（命中即视为正常业务页面；其余前缀按非业务页面拒绝）。
/// 认证流程页（login/register/password-reset/verify-email）不属于签到业务页面。
const BUSINESS_PAGE_PREFIXES: &[&str] = &[
    "/boards",
    "/posts",
    "/users",
    "/tags",
    "/search",
    "/notifications",
    "/me",
    "/shop",
    "/activity",
    "/settings",
    "/editor",
    "/moderation",
    "/admin",
    "/appeals",
];

/// 已知爬虫/健康检查 UA 标记（小写子串匹配；命中即不可领取签到）。
const CRAWLER_UA_TOKENS: &[&str] = &[
    "bot",
    "spider",
    "crawler",
    "crawl",
    "slurp",
    "googlebot",
    "bingbot",
    "baiduspider",
    "bytespider",
    "yandexbot",
    "duckduckbot",
    "sogou",
    "360spider",
    "ia_archiver",
    "facebot",
    "applebot",
    "twitterbot",
    "linkedinbot",
    "telegrambot",
    "discordbot",
    "whatsapp",
    "slackbot",
    "gptbot",
    "chatgpt",
    "claudebot",
    "perplexitybot",
    "curl/",
    "wget",
    "python-requests",
    "go-http-client",
    "okhttp",
    "java/",
    "httpclient",
    "headlesschrome",
    "phantomjs",
    "puppeteer",
    "playwright",
    "selenium",
    "screenshot",
    "healthcheck",
    "uptimecheck",
    "pingdom",
    "monitoring",
    "newspaper",
    "feedfetcher",
    "mediapartners",
    "adsbot",
];

const STATIC_EXTENSIONS: &[&str] = &[
    ".js",
    ".css",
    ".png",
    ".jpg",
    ".jpeg",
    ".gif",
    ".svg",
    ".webp",
    ".ico",
    ".woff",
    ".woff2",
    ".ttf",
    ".eot",
    ".otf",
    ".mp4",
    ".webm",
    ".pdf",
    ".map",
    ".br",
    ".gz",
    ".txt",
    ".xml",
    ".json",
    ".webmanifest",
];

const STATIC_PATH_PREFIXES: &[&str] = &[
    "/api/",
    "/healthz",
    "/readyz",
    "/assets/",
    "/static/",
    "/_app",
    "/images/",
    "/fonts/",
    "/favicon.ico",
    "/robots.txt",
    "/sitemap",
    "/manifest",
    "/service-worker.js",
];

/// 判定是否为有效业务页面访问（M07-LEVELS-03）。
///
/// 排除：非业务页面路径、静态资源、预取（`Sec-Purpose: prefetch/preview`、
/// `Purpose: prefetch`、`Sec-Fetch-Dest: empty`）、已知爬虫/健康检查 UA 与
/// 健康检查路径。失败请求（HTTP ≥400）由路由层保证不进入领取路径。
pub fn validate_visit(ctx: &VisitContext<'_>) -> Result<(), VisitRejection> {
    let path = ctx.path.trim();
    if path.is_empty() || !path.starts_with('/') {
        return Err(VisitRejection::Malformed);
    }
    // 去掉 query/fragment。
    let path = path
        .split_once('?')
        .map(|(p, _)| p)
        .or_else(|| path.split_once('#').map(|(p, _)| p))
        .unwrap_or(path);

    // 静态资源 / 健康检查路径。
    for prefix in STATIC_PATH_PREFIXES {
        if path.starts_with(prefix) || path == prefix.trim_end_matches('/') {
            return Err(VisitRejection::NotEligible);
        }
    }
    for ext in STATIC_EXTENSIONS {
        if path.ends_with(ext) {
            return Err(VisitRejection::NotEligible);
        }
    }

    // 业务页面前缀。
    let is_business = BUSINESS_PAGE_PREFIXES
        .iter()
        .any(|prefix| path.starts_with(prefix));
    // 根路径（首页）视为业务页面。
    if path != "/" && !is_business {
        return Err(VisitRejection::NotBusinessPage);
    }

    // 预取。
    for v in [ctx.sec_purpose, ctx.purpose].into_iter().flatten() {
        let lower = v.to_ascii_lowercase();
        if lower.contains("prefetch") || lower.contains("preview") {
            return Err(VisitRejection::NotEligible);
        }
    }
    if let Some(dest) = ctx.sec_fetch_dest {
        if dest.eq_ignore_ascii_case("empty") {
            return Err(VisitRejection::NotEligible);
        }
    }

    // 爬虫/健康检查 UA。
    if let Some(ua) = ctx.user_agent {
        let lower = ua.to_ascii_lowercase();
        if CRAWLER_UA_TOKENS.iter().any(|t| lower.contains(t)) {
            return Err(VisitRejection::NotEligible);
        }
    }

    Ok(())
}

/// 连续签到天数（M07-LEVELS-04/08 断签规则）：
/// 从今天（今天未签则从昨天）起向前数连续有 `check_in` 领取的天数。
pub async fn streak_days(
    pool: &DatabasePool,
    user_id: &str,
    today: &str,
) -> Result<i64, sqlx::Error> {
    let days: Vec<String> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT DISTINCT ac.activity_day
             FROM activity_claims ac
             JOIN activity_rules ar ON ar.id = ac.rule_id
             WHERE ac.user_id = ? AND ar.kind = 'check_in' AND ac.status = 'granted'
               AND ac.point_operation_id NOT LIKE 'pending:%'
             ORDER BY ac.activity_day DESC",
            )
            .bind(user_id)
            .fetch_all(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT DISTINCT ac.activity_day
             FROM activity_claims ac
             JOIN activity_rules ar ON ar.id = ac.rule_id
             WHERE ac.user_id = ? AND ar.kind = 'check_in' AND ac.status = 'granted'
               AND ac.point_operation_id NOT LIKE 'pending:%'
             ORDER BY ac.activity_day DESC",
            )
            .bind(user_id)
            .fetch_all(p)
            .await?
        }
    };
    let set: HashSet<String> = days.into_iter().collect();
    if !set.contains(today) {
        let Some(yesterday) = prev_day(today) else {
            return Ok(0);
        };
        if !set.contains(&yesterday) {
            return Ok(0);
        }
    }
    let mut streak = 0i64;
    let mut cur = today.to_string();
    if !set.contains(&cur) {
        cur = prev_day(&cur).unwrap_or(cur);
    }
    loop {
        if set.contains(&cur) {
            streak += 1;
            let Some(prev) = prev_day(&cur) else {
                break;
            };
            cur = prev;
        } else {
            break;
        }
    }
    Ok(streak)
}

/// 今日是否已签到（存在 granted 且非 pending 占位的 check_in 领取）。
pub async fn claimed_on_day(
    pool: &DatabasePool,
    user_id: &str,
    activity_day: &str,
) -> Result<bool, sqlx::Error> {
    let count: Option<i64> = match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT 1
             FROM activity_claims ac
             JOIN activity_rules ar ON ar.id = ac.rule_id
             WHERE ac.user_id = ? AND ac.activity_day = ? AND ar.kind = 'check_in' AND ac.status = 'granted'
               AND ac.point_operation_id NOT LIKE 'pending:%'
             LIMIT 1",
        )
        .bind(user_id)
        .bind(activity_day)
        .fetch_optional(p)
        .await?,
        Either::Right(p) => sqlx::query_scalar(
            "SELECT 1
             FROM activity_claims ac
             JOIN activity_rules ar ON ar.id = ac.rule_id
             WHERE ac.user_id = ? AND ac.activity_day = ? AND ar.kind = 'check_in' AND ac.status = 'granted'
               AND ac.point_operation_id NOT LIKE 'pending:%'
             LIMIT 1",
        )
        .bind(user_id)
        .bind(activity_day)
        .fetch_optional(p)
        .await?,
    };
    Ok(count.is_some())
}
