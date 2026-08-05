//! 匿名预认证 CSRF 状态（M02-SESSION-08）
//!
//! 为注册/登录等预认证写端点（无会话 Cookie）提供服务端可回溯校验的 CSRF
//! 状态，防止 login CSRF（SECURITY.md §4：匿名登录/注册流程使用独立的预认证
//! CSRF Cookie/状态，不能借此获得用户身份）。
//!
//! 机制与 Session 绑定 synchronizer token 同构：
//! - `GET /api/v1/auth/csrf`（未认证）签发匿名令牌并写入
//!   `__Host-bblbb_csrf` cookie，同时返回由 `(记录 id, csrf_secret_hash)`
//!   确定性派生的 CSRF token；
//! - 预认证写请求必须同时携带该 cookie 与匹配的 `X-CSRF-Token`，
//!   由 CSRF 中间件校验（`middleware::csrf::csrf_protection`）；
//! - 匿名令牌只在数据库保存 SHA-256（`token_hash`），TTL 10 分钟，
//!   过期行在签发新状态时顺带清理。

use axum_extra::extract::cookie::{Cookie, SameSite};
use sqlx::Either;
use uuid::Uuid;

use crate::{
    auth::{
        session::generate_csrf_token,
        token::{generate_token, hash_token},
    },
    db::pool::DatabasePool,
};

/// 预认证 CSRF cookie 名称（独立于 `__Host-bblbb_session`，HttpOnly 可防 JS 读取；
/// token 本身通过 JSON 响应下发，cookie 仅用于服务端回溯绑定）。
pub const PREAUTH_COOKIE_NAME: &str = "__Host-bblbb_csrf";

/// 预认证 CSRF 状态 TTL：10 分钟（匿名状态短期有效，过期即失效）。
pub const PREAUTH_TTL_MS: i64 = 10 * 60 * 1000;

/// 签发结果：写入 cookie 的匿名令牌 + 返回给客户端的派生 CSRF token。
pub struct PreauthIssue {
    /// 写入 `__Host-bblbb_csrf` cookie 的匿名令牌（客户端不可读、不可逆推）。
    pub cookie_token: String,
    /// 返回给客户端的 CSRF token（记录 id + secret 确定性派生，与 Session 同构）。
    pub csrf_token: String,
}

/// 签发新的预认证 CSRF 状态：插入数据库行并返回 cookie 令牌与派生 CSRF token。
///
/// 顺带清理已过期的旧状态（低频率操作，防表膨胀）。
pub async fn issue_preauth(pool: &DatabasePool) -> Result<PreauthIssue, sqlx::Error> {
    let token = generate_token();
    let token_hash = hash_token(&token);
    let id = Uuid::now_v7().to_string();
    let now = crate::outbox::now_millis();
    let expires_at = now + PREAUTH_TTL_MS;

    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO preauth_csrf_tokens (id, token_hash, csrf_secret_hash, created_at, expires_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&token_hash)
            .bind(&token_hash) // csrf_secret_hash 与 user_sessions 一致取 token_hash
            .bind(now)
            .bind(expires_at)
            .execute(p)
            .await?;
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO preauth_csrf_tokens (id, token_hash, csrf_secret_hash, created_at, expires_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&token_hash)
            .bind(&token_hash)
            .bind(now)
            .bind(expires_at)
            .execute(p)
            .await?;
        }
    }

    let _ = cleanup_expired(pool).await;

    Ok(PreauthIssue {
        cookie_token: token,
        csrf_token: generate_csrf_token(&id, &token_hash),
    })
}

/// 根据 `__Host-bblbb_csrf` cookie 令牌解析预认证状态。
///
/// 返回 `(记录 id, csrf_secret_hash)` 供派生期望 CSRF token；令牌不存在或
/// 已过期返回 `Ok(None)`。
pub async fn resolve_preauth(
    pool: &DatabasePool,
    token: &str,
) -> Result<Option<(String, String)>, sqlx::Error> {
    let token_hash = hash_token(token);
    let now = crate::outbox::now_millis();

    let row: Option<PreauthRow> = match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, PreauthRow>(
                "SELECT id, csrf_secret_hash FROM preauth_csrf_tokens
                 WHERE token_hash = ? AND expires_at > ?",
            )
            .bind(&token_hash)
            .bind(now)
            .fetch_optional(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_as::<_, PreauthRow>(
                "SELECT id, csrf_secret_hash FROM preauth_csrf_tokens
                 WHERE token_hash = ? AND expires_at > ?",
            )
            .bind(&token_hash)
            .bind(now)
            .fetch_optional(p)
            .await?
        }
    };

    Ok(row.map(|r| (r.id, r.csrf_secret_hash)))
}

/// 清理已过期的预认证状态，返回删除条数。
async fn cleanup_expired(pool: &DatabasePool) -> Result<u64, sqlx::Error> {
    let now = crate::outbox::now_millis();
    match pool {
        Either::Left(p) => sqlx::query("DELETE FROM preauth_csrf_tokens WHERE expires_at <= ?")
            .bind(now)
            .execute(p)
            .await
            .map(|r| r.rows_affected()),
        Either::Right(p) => sqlx::query("DELETE FROM preauth_csrf_tokens WHERE expires_at <= ?")
            .bind(now)
            .execute(p)
            .await
            .map(|r| r.rows_affected()),
    }
}

/// 构建预认证 CSRF cookie（`__Host-` + Secure/HttpOnly/SameSite=Lax/Path=/，
/// 与 Session cookie 属性一致，防子域伪造与跨站携带）。
pub fn build_preauth_cookie(token: &str) -> Cookie<'static> {
    Cookie::build((PREAUTH_COOKIE_NAME, token.to_string()))
        .path("/")
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::milliseconds(PREAUTH_TTL_MS))
        .build()
}

/// 构建清除预认证 CSRF cookie（与签发属性一致，否则 `__Host-` cookie 无法清除）。
pub fn build_clear_preauth_cookie() -> Cookie<'static> {
    Cookie::build((PREAUTH_COOKIE_NAME, ""))
        .path("/")
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::seconds(0))
        .build()
}

/// 数据库行结构
#[derive(sqlx::FromRow)]
struct PreauthRow {
    id: String,
    csrf_secret_hash: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum_extra::extract::cookie::SameSite;

    /// `__Host-` 前缀 Cookie 必须带 Secure、Path=/ 且无 Domain（防子域伪造）。
    #[test]
    fn preauth_cookie_uses_host_prefix_with_secure_attributes() {
        let cookie = build_preauth_cookie("tok");
        assert_eq!(cookie.name(), PREAUTH_COOKIE_NAME);
        assert!(
            cookie.name().starts_with("__Host-"),
            "必须使用 __Host- 前缀"
        );
        assert_eq!(cookie.path().unwrap_or(""), "/");
        assert!(cookie.secure().unwrap_or(false), "__Host- 要求 Secure");
        assert!(
            cookie.http_only().unwrap_or(false),
            "预认证 cookie 必须 HttpOnly（token 经 JSON 下发）"
        );
        match cookie.same_site() {
            Some(same) => assert_eq!(same, SameSite::Lax, "SameSite=Lax 防跨站携带"),
            None => panic!("必须显式设置 SameSite"),
        }
        assert!(
            cookie.domain().is_none(),
            "__Host- 前缀禁止 Domain 属性（防子域伪造）"
        );
        assert!(cookie.max_age().is_some(), "Cookie 必须有 max-age（TTL）");
    }

    /// 清除 cookie 与签发 cookie 属性一致（否则 __Host- cookie 无法清除）。
    #[test]
    fn clear_preauth_cookie_matches_issue_attributes() {
        let clear = build_clear_preauth_cookie();
        assert_eq!(clear.name(), PREAUTH_COOKIE_NAME);
        assert_eq!(clear.path().unwrap_or(""), "/");
        assert!(clear.secure().unwrap_or(false));
        assert!(clear.http_only().unwrap_or(false));
        match clear.same_site() {
            Some(same) => assert_eq!(same, SameSite::Lax),
            None => panic!("必须显式设置 SameSite"),
        }
        assert!(clear.domain().is_none());
        assert_eq!(clear.max_age().unwrap_or_default(), time::Duration::ZERO);
    }
}
