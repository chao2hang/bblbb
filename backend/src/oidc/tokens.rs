//! 授权码 / opaque Token 生命周期（M11-PROTOCOL-04/06/07/08）。
//!
//! - 授权码与 Access/Refresh Token 都是高熵随机值，数据库只存 SHA-256 hash
//!   （`crate::auth::token::hash_token`），绝不落日志；
//! - 授权码一次性消费 + 过期 + client/redirect/PKCE/request hash 绑定；
//! - opaque Access Token（默认 10 分钟）+ Refresh Token（默认 30 天）；
//! - Refresh Token Rotation：每次使用签发新 token，旧 token 再出现视为
//!   泄漏 → 撤销整个 family 并通知用户；
//! - 撤销支持 access/refresh，响应不泄漏 token 存在性。

use serde::Serialize;
use serde_json::{json, Value};
use sqlx::Either;

use super::clients::OAuthClient;
use super::protocol::{pairwise_subject, scope_is_subset, split_scopes};
use super::{hash_token, now_millis, OidcError};

/// 授权码行（`oauth_authorization_codes`）。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CodeRow {
    pub id: String,
    pub code_hash: String,
    pub client_id: String,
    pub user_id: String,
    pub redirect_uri: String,
    pub scope: String,
    pub nonce: Option<String>,
    pub state_hash: Option<String>,
    pub request_hash: Option<String>,
    pub code_challenge: Option<String>,
    pub code_challenge_method: String,
    pub expires_at: i64,
    pub consumed_at: Option<i64>,
    pub created_at: i64,
}

/// Token family 行（`oauth_token_families`）。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FamilyRow {
    pub id: String,
    pub client_id: String,
    pub user_id: String,
    pub scope: String,
    pub created_at: i64,
    pub revoked_at: Option<i64>,
    pub revoke_reason: Option<String>,
}

/// Token 对行（`oauth_tokens`：一个 access + 一个 refresh 共享一行）。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TokenRow {
    pub id: String,
    pub family_id: String,
    pub access_token_hash: String,
    pub refresh_token_hash: Option<String>,
    pub id_token_jti: Option<String>,
    pub client_id: String,
    pub user_id: String,
    pub scope: String,
    pub issued_at: i64,
    pub expires_at: i64,
    pub revoked_at: Option<i64>,
    pub revoke_reason: Option<String>,
    pub last_used_at: Option<i64>,
}

/// 用户投影（token 签发 / userinfo / ID Token 使用）。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserProjection {
    pub id: String,
    pub status: String,
    pub username_normalized: String,
    pub display_name: Option<String>,
    pub email_normalized: String,
    pub email_verified: i64,
    pub updated_at: i64,
}

impl UserProjection {
    pub fn is_active(&self) -> bool {
        self.status == "active"
    }
}

/// Token 端点响应（OAuth 2.0 §4.1.4 / OIDC Core §3.1.3.3）。
#[derive(Debug, Clone, Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub refresh_token: String,
    pub id_token: Option<String>,
}

impl TokenResponse {
    pub fn to_json(&self) -> Value {
        let mut v = serde_json::Map::new();
        v.insert("access_token".into(), json!(self.access_token));
        v.insert("token_type".into(), json!("Bearer"));
        v.insert("expires_in".into(), json!(self.expires_in));
        v.insert("refresh_token".into(), json!(self.refresh_token));
        if let Some(id_token) = &self.id_token {
            v.insert("id_token".into(), json!(id_token));
        }
        Value::Object(v)
    }
}

/// 查询用户（token 签发前检查状态与 claim 投影）。
pub async fn fetch_user(
    pool: &crate::db::DatabasePool,
    user_id: &str,
) -> Result<Option<UserProjection>, OidcError> {
    let row: Option<UserProjection> = match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, UserProjection>(
                "SELECT id, status, username_normalized, display_name, email_normalized,
                        COALESCE(email_verified, 0) AS email_verified, updated_at
                 FROM users WHERE id = ?",
            )
            .bind(user_id)
            .fetch_optional(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_as::<_, UserProjection>(
                "SELECT id, status, username_normalized, display_name, email_normalized,
                        COALESCE(email_verified, 0) AS email_verified, updated_at
                 FROM users WHERE id = ?",
            )
            .bind(user_id)
            .fetch_optional(p)
            .await?
        }
    };
    Ok(row)
}

/// 创建授权码：返回明文 code（仅此一次），数据库只存 hash。
#[allow(clippy::too_many_arguments)]
pub async fn create_authorization_code(
    pool: &crate::db::DatabasePool,
    client_internal_id: &str,
    user_id: &str,
    redirect_uri: &str,
    scope: &str,
    nonce: Option<&str>,
    state: Option<&str>,
    request_hash: &str,
    code_challenge: Option<&str>,
    now: i64,
) -> Result<String, OidcError> {
    let code = crate::auth::token::generate_token();
    let code_hash = hash_token(&code);
    let state_hash = state.map(hash_token);
    let expires_at = now + super::AUTH_CODE_TTL_MS;
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO oauth_authorization_codes
                    (id, code_hash, client_id, user_id, redirect_uri, scope, nonce, state_hash,
                     request_hash, code_challenge, code_challenge_method, expires_at, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'S256', ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(&code_hash)
            .bind(client_internal_id)
            .bind(user_id)
            .bind(redirect_uri)
            .bind(scope)
            .bind(nonce)
            .bind(&state_hash)
            .bind(request_hash)
            .bind(code_challenge)
            .bind(expires_at)
            .bind(now)
            .execute(p)
            .await?;
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO oauth_authorization_codes
                    (id, code_hash, client_id, user_id, redirect_uri, scope, nonce, state_hash,
                     request_hash, code_challenge, code_challenge_method, expires_at, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'S256', ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(&code_hash)
            .bind(client_internal_id)
            .bind(user_id)
            .bind(redirect_uri)
            .bind(scope)
            .bind(nonce)
            .bind(&state_hash)
            .bind(request_hash)
            .bind(code_challenge)
            .bind(expires_at)
            .bind(now)
            .execute(p)
            .await?;
        }
    }
    Ok(code)
}

/// 按 code hash 查授权码。
pub async fn lookup_code(
    pool: &crate::db::DatabasePool,
    code_hash: &str,
) -> Result<Option<CodeRow>, OidcError> {
    let row: Option<CodeRow> = match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, CodeRow>(
                "SELECT id, code_hash, client_id, user_id, redirect_uri, scope, nonce, state_hash,
                        request_hash, code_challenge, code_challenge_method, expires_at, consumed_at, created_at
                 FROM oauth_authorization_codes WHERE code_hash = ?",
            )
            .bind(code_hash)
            .fetch_optional(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_as::<_, CodeRow>(
                "SELECT id, code_hash, client_id, user_id, redirect_uri, scope, nonce, state_hash,
                        request_hash, code_challenge, code_challenge_method, expires_at, consumed_at, created_at
                 FROM oauth_authorization_codes WHERE code_hash = ?",
            )
            .bind(code_hash)
            .fetch_optional(p)
            .await?
        }
    };
    Ok(row)
}

/// 签发 Token 对：在事务内校验后原子消费授权码并创建 family + token 行。
///
/// 失败路径（重放/过期/PKCE 失败/绑定不匹配）返回标准 `invalid_grant`，
/// 不泄漏更多信息（docs/AUTH-OIDC.md §8/§14）。
#[allow(clippy::too_many_arguments)]
pub async fn exchange_authorization_code(
    pool: &crate::db::DatabasePool,
    client: &OAuthClient,
    raw_code: &str,
    redirect_uri: &str,
    code_verifier: &str,
    issuer: &str,
    master_key: &[u8],
    now: i64,
) -> Result<TokenResponse, OidcError> {
    let code_hash = hash_token(raw_code);
    let code = lookup_code(pool, &code_hash).await?.ok_or_else(|| {
        OidcError::InvalidGrant("authorization code is invalid or has expired".into())
    })?;

    if code.consumed_at.is_some() {
        return Err(OidcError::InvalidGrant(
            "authorization code is invalid or has expired".into(),
        ));
    }
    if code.expires_at < now {
        return Err(OidcError::InvalidGrant(
            "authorization code is invalid or has expired".into(),
        ));
    }
    if code.client_id != client.id {
        return Err(OidcError::InvalidGrant(
            "authorization code was issued to a different client".into(),
        ));
    }
    if code.redirect_uri != redirect_uri {
        return Err(OidcError::InvalidGrant(
            "redirect_uri does not match the authorization request".into(),
        ));
    }
    if !super::protocol::is_valid_code_verifier(code_verifier) {
        return Err(OidcError::InvalidGrant("PKCE verification failed".into()));
    }
    let challenge = code
        .code_challenge
        .as_deref()
        .ok_or_else(|| OidcError::InvalidGrant("PKCE verification failed".into()))?;
    if !super::protocol::verify_pkce(challenge, code_verifier) {
        return Err(OidcError::InvalidGrant("PKCE verification failed".into()));
    }
    if !client.is_active() {
        return Err(OidcError::InvalidGrant(
            "authorization code is invalid or has expired".into(),
        ));
    }
    let user = fetch_user(pool, &code.user_id).await?.ok_or_else(|| {
        OidcError::InvalidGrant("authorization code is invalid or has expired".into())
    })?;
    if !user.is_active() {
        return Err(OidcError::InvalidGrant(
            "authorization code is invalid or has expired".into(),
        ));
    }
    // 同意必须仍有效（撤销后不得换 token）。
    let scopes = split_scopes(&code.scope);
    if !super::consent::consents_are_active(pool, &code.user_id, &client.id, &scopes, now).await? {
        return Err(OidcError::InvalidGrant(
            "user consent for the requested scope is no longer active".into(),
        ));
    }

    let issued_at = now;
    let refresh_expires = now + super::REFRESH_TOKEN_TTL_MS;
    let access_token = crate::auth::token::generate_token();
    let refresh_token = crate::auth::token::generate_token();
    let family_id = uuid::Uuid::now_v7().to_string();
    let token_id = uuid::Uuid::now_v7().to_string();
    let id_token_jti = uuid::Uuid::now_v7().to_string();

    match pool {
        Either::Left(p) => {
            let mut tx = p.begin().await?;
            let consumed = sqlx::query(
                "UPDATE oauth_authorization_codes SET consumed_at = ?
                 WHERE id = ? AND consumed_at IS NULL",
            )
            .bind(now)
            .bind(&code.id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if consumed != 1 {
                tx.rollback().await?;
                return Err(OidcError::InvalidGrant(
                    "authorization code is invalid or has expired".into(),
                ));
            }
            sqlx::query(
                "INSERT INTO oauth_token_families (id, client_id, user_id, scope, created_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&family_id)
            .bind(&client.id)
            .bind(&code.user_id)
            .bind(&code.scope)
            .bind(issued_at)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "INSERT INTO oauth_tokens
                    (id, family_id, access_token_hash, refresh_token_hash, id_token_jti, client_id,
                     user_id, scope, issued_at, expires_at, revoked_at, revoke_reason, last_used_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL, NULL)",
            )
            .bind(&token_id)
            .bind(&family_id)
            .bind(hash_token(&access_token))
            .bind(hash_token(&refresh_token))
            .bind(&id_token_jti)
            .bind(&client.id)
            .bind(&code.user_id)
            .bind(&code.scope)
            .bind(issued_at)
            .bind(refresh_expires)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
        }
        Either::Right(p) => {
            let mut tx = p.begin().await?;
            let consumed = sqlx::query(
                "UPDATE oauth_authorization_codes SET consumed_at = ?
                 WHERE id = ? AND consumed_at IS NULL",
            )
            .bind(now)
            .bind(&code.id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if consumed != 1 {
                tx.rollback().await?;
                return Err(OidcError::InvalidGrant(
                    "authorization code is invalid or has expired".into(),
                ));
            }
            sqlx::query(
                "INSERT INTO oauth_token_families (id, client_id, user_id, scope, created_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&family_id)
            .bind(&client.id)
            .bind(&code.user_id)
            .bind(&code.scope)
            .bind(issued_at)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "INSERT INTO oauth_tokens
                    (id, family_id, access_token_hash, refresh_token_hash, id_token_jti, client_id,
                     user_id, scope, issued_at, expires_at, revoked_at, revoke_reason, last_used_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL, NULL)",
            )
            .bind(&token_id)
            .bind(&family_id)
            .bind(hash_token(&access_token))
            .bind(hash_token(&refresh_token))
            .bind(&id_token_jti)
            .bind(&client.id)
            .bind(&code.user_id)
            .bind(&code.scope)
            .bind(issued_at)
            .bind(refresh_expires)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
        }
    }

    // 签发 ID Token（active 密钥；scope 投影身份 claim）。
    let scopes = split_scopes(&code.scope);
    let id_token = if scopes.iter().any(|s| s == "openid") {
        let claims = super::protocol::IdTokenClaims {
            iss: issuer.to_string(),
            sub: pairwise_subject(issuer, &code.user_id, &client.client_id),
            aud: client.client_id.clone(),
            exp: now / 1000 + super::ID_TOKEN_TTL_SECS,
            iat: now / 1000,
            auth_time: Some(code.created_at / 1000),
            nonce: code.nonce.clone(),
            azp: None,
            jti: id_token_jti.clone(),
            name: None,
            preferred_username: None,
            picture: None,
            updated_at: None,
            email: None,
            email_verified: None,
        }
        .with_user_projection(
            &scopes,
            Some(user.username_normalized.clone()),
            user.display_name.clone(),
            Some(user.updated_at / 1000),
            Some(user.email_normalized.clone()),
            user.email_verified != 0,
        );
        let (token, _kid) = super::keys::sign_id_token(pool, master_key, &claims).await?;
        Some(token)
    } else {
        None
    };

    Ok(TokenResponse {
        access_token,
        token_type: "Bearer".to_string(),
        expires_in: super::ACCESS_TOKEN_TTL_SECS,
        refresh_token,
        id_token,
    })
}

/// 按 refresh token 查找行（未撤销）。
async fn lookup_refresh_token(
    pool: &crate::db::DatabasePool,
    refresh_hash: &str,
) -> Result<Option<TokenRow>, OidcError> {
    let row: Option<TokenRow> = match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, TokenRow>(
                "SELECT id, family_id, access_token_hash, refresh_token_hash, id_token_jti, client_id,
                        user_id, scope, issued_at, expires_at, revoked_at, revoke_reason, last_used_at
                 FROM oauth_tokens WHERE refresh_token_hash = ? AND revoked_at IS NULL",
            )
            .bind(refresh_hash)
            .fetch_optional(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_as::<_, TokenRow>(
                "SELECT id, family_id, access_token_hash, refresh_token_hash, id_token_jti, client_id,
                        user_id, scope, issued_at, expires_at, revoked_at, revoke_reason, last_used_at
                 FROM oauth_tokens WHERE refresh_token_hash = ? AND revoked_at IS NULL",
            )
            .bind(refresh_hash)
            .fetch_optional(p)
            .await?
        }
    };
    Ok(row)
}

/// 按 access token hash 查找行。
pub async fn lookup_access_token(
    pool: &crate::db::DatabasePool,
    access_hash: &str,
) -> Result<Option<TokenRow>, OidcError> {
    let row: Option<TokenRow> = match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, TokenRow>(
                "SELECT id, family_id, access_token_hash, refresh_token_hash, id_token_jti, client_id,
                        user_id, scope, issued_at, expires_at, revoked_at, revoke_reason, last_used_at
                 FROM oauth_tokens WHERE access_token_hash = ?",
            )
            .bind(access_hash)
            .fetch_optional(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_as::<_, TokenRow>(
                "SELECT id, family_id, access_token_hash, refresh_token_hash, id_token_jti, client_id,
                        user_id, scope, issued_at, expires_at, revoked_at, revoke_reason, last_used_at
                 FROM oauth_tokens WHERE access_token_hash = ?",
            )
            .bind(access_hash)
            .fetch_optional(p)
            .await?
        }
    };
    Ok(row)
}

/// 取 family 行。
async fn lookup_family(
    pool: &crate::db::DatabasePool,
    family_id: &str,
) -> Result<Option<FamilyRow>, OidcError> {
    let row: Option<FamilyRow> = match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, FamilyRow>(
                "SELECT id, client_id, user_id, scope, created_at, revoked_at, revoke_reason
                 FROM oauth_token_families WHERE id = ?",
            )
            .bind(family_id)
            .fetch_optional(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_as::<_, FamilyRow>(
                "SELECT id, client_id, user_id, scope, created_at, revoked_at, revoke_reason
                 FROM oauth_token_families WHERE id = ?",
            )
            .bind(family_id)
            .fetch_optional(p)
            .await?
        }
    };
    Ok(row)
}

/// family 内最新（未被换掉的）refresh token 行 id。
async fn latest_refresh_in_family(
    pool: &crate::db::DatabasePool,
    family_id: &str,
) -> Result<Option<String>, OidcError> {
    let id: Option<String> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT id FROM oauth_tokens
                 WHERE family_id = ? AND refresh_token_hash IS NOT NULL AND revoked_at IS NULL
                 ORDER BY issued_at DESC, id DESC LIMIT 1",
            )
            .bind(family_id)
            .fetch_optional(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT id FROM oauth_tokens
                 WHERE family_id = ? AND refresh_token_hash IS NOT NULL AND revoked_at IS NULL
                 ORDER BY issued_at DESC, id DESC LIMIT 1",
            )
            .bind(family_id)
            .fetch_optional(p)
            .await?
        }
    };
    Ok(id)
}

/// Refresh Token Rotation：签发新 access+refresh，旧 token 标记使用。
/// 旧 token 重用 → 撤销整个 family + 安全通知（M11-PROTOCOL-08）。
pub async fn refresh_tokens(
    pool: &crate::db::DatabasePool,
    client: &OAuthClient,
    raw_refresh_token: &str,
    requested_scope: Option<&str>,
    now: i64,
) -> Result<TokenResponse, OidcError> {
    let refresh_hash = hash_token(raw_refresh_token);
    let Some(row) = lookup_refresh_token(pool, &refresh_hash).await? else {
        return Err(OidcError::InvalidGrant(
            "refresh token is invalid or has expired".into(),
        ));
    };
    if row.expires_at < now {
        return Err(OidcError::InvalidGrant(
            "refresh token is invalid or has expired".into(),
        ));
    }
    if row.client_id != client.id {
        return Err(OidcError::InvalidGrant(
            "refresh token was issued to a different client".into(),
        ));
    }
    let Some(family) = lookup_family(pool, &row.family_id).await? else {
        return Err(OidcError::InvalidGrant(
            "refresh token is invalid or has expired".into(),
        ));
    };
    if family.revoked_at.is_some() {
        return Err(OidcError::InvalidGrant(
            "refresh token is invalid or has expired".into(),
        ));
    }
    if !client.is_active() {
        return Err(OidcError::InvalidGrant(
            "refresh token is invalid or has expired".into(),
        ));
    }
    let user = fetch_user(pool, &row.user_id)
        .await?
        .ok_or_else(|| OidcError::InvalidGrant("refresh token is invalid or has expired".into()))?;
    if !user.is_active() {
        return Err(OidcError::InvalidGrant(
            "refresh token is invalid or has expired".into(),
        ));
    }

    // 重用检测：presented 必须是 family 内最新的有效 refresh token。
    let latest = latest_refresh_in_family(pool, &family.id).await?;
    if latest.as_deref() != Some(row.id.as_str()) {
        revoke_family(pool, &family.id, "refresh_token_reuse", "oauth_token", true).await?;
        return Err(OidcError::InvalidGrant(
            "refresh token is invalid or has expired".into(),
        ));
    }

    // scope 只能缩小。
    let family_scopes = split_scopes(&family.scope);
    let scopes: Vec<String> = match requested_scope {
        Some(raw) => {
            let requested = split_scopes(raw);
            if requested.is_empty() {
                family_scopes.clone()
            } else {
                if !scope_is_subset(&requested, &family_scopes) {
                    return Err(OidcError::InvalidGrant(
                        "requested scope exceeds the granted scope".into(),
                    ));
                }
                requested
            }
        }
        None => family_scopes.clone(),
    };

    let scope_str = super::protocol::join_scopes(&scopes);
    let issued_at = now;
    let refresh_expires = now + super::REFRESH_TOKEN_TTL_MS;
    let access_token = crate::auth::token::generate_token();
    let new_refresh_token = crate::auth::token::generate_token();
    let new_token_id = uuid::Uuid::now_v7().to_string();

    match pool {
        Either::Left(p) => {
            let mut tx = p.begin().await?;
            sqlx::query("UPDATE oauth_tokens SET last_used_at = ? WHERE id = ?")
                .bind(now)
                .bind(&row.id)
                .execute(&mut *tx)
                .await?;
            sqlx::query(
                "INSERT INTO oauth_tokens
                    (id, family_id, access_token_hash, refresh_token_hash, id_token_jti, client_id,
                     user_id, scope, issued_at, expires_at, revoked_at, revoke_reason, last_used_at)
                 VALUES (?, ?, ?, ?, NULL, ?, ?, ?, ?, ?, NULL, NULL, NULL)",
            )
            .bind(&new_token_id)
            .bind(&family.id)
            .bind(hash_token(&access_token))
            .bind(hash_token(&new_refresh_token))
            .bind(&client.id)
            .bind(&row.user_id)
            .bind(&scope_str)
            .bind(issued_at)
            .bind(refresh_expires)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
        }
        Either::Right(p) => {
            let mut tx = p.begin().await?;
            sqlx::query("UPDATE oauth_tokens SET last_used_at = ? WHERE id = ?")
                .bind(now)
                .bind(&row.id)
                .execute(&mut *tx)
                .await?;
            sqlx::query(
                "INSERT INTO oauth_tokens
                    (id, family_id, access_token_hash, refresh_token_hash, id_token_jti, client_id,
                     user_id, scope, issued_at, expires_at, revoked_at, revoke_reason, last_used_at)
                 VALUES (?, ?, ?, ?, NULL, ?, ?, ?, ?, ?, NULL, NULL, NULL)",
            )
            .bind(&new_token_id)
            .bind(&family.id)
            .bind(hash_token(&access_token))
            .bind(hash_token(&new_refresh_token))
            .bind(&client.id)
            .bind(&row.user_id)
            .bind(&scope_str)
            .bind(issued_at)
            .bind(refresh_expires)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
        }
    }

    Ok(TokenResponse {
        access_token,
        token_type: "Bearer".to_string(),
        expires_in: super::ACCESS_TOKEN_TTL_SECS,
        refresh_token: new_refresh_token,
        id_token: None,
    })
}
/// 撤销一个 family（含全部 token 行）；`notify_user` 时写安全通知。
pub async fn revoke_family(
    pool: &crate::db::DatabasePool,
    family_id: &str,
    reason: &str,
    request_id: &str,
    notify_user: bool,
) -> Result<(), OidcError> {
    let family = lookup_family(pool, family_id).await?;
    let user_id = family.as_ref().map(|f| f.user_id.clone());
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE oauth_token_families SET revoked_at = ?, revoke_reason = ? WHERE id = ?",
            )
            .bind(now_millis())
            .bind(reason)
            .bind(family_id)
            .execute(p)
            .await?;
            sqlx::query(
                "UPDATE oauth_tokens SET revoked_at = ?, revoke_reason = ?
                 WHERE family_id = ? AND revoked_at IS NULL",
            )
            .bind(now_millis())
            .bind(reason)
            .bind(family_id)
            .execute(p)
            .await?;
        }
        Either::Right(p) => {
            sqlx::query(
                "UPDATE oauth_token_families SET revoked_at = ?, revoke_reason = ? WHERE id = ?",
            )
            .bind(now_millis())
            .bind(reason)
            .bind(family_id)
            .execute(p)
            .await?;
            sqlx::query(
                "UPDATE oauth_tokens SET revoked_at = ?, revoke_reason = ?
                 WHERE family_id = ? AND revoked_at IS NULL",
            )
            .bind(now_millis())
            .bind(reason)
            .bind(family_id)
            .execute(p)
            .await?;
        }
    }
    crate::audit::AuditEntry::user_action(
        user_id.as_deref().unwrap_or("__system__"),
        "oauth.token_family.revoke",
    )
    .with_target("oauth_token_family", family_id)
    .with_reason(reason)
    .with_request_id(request_id)
    .record(pool)
    .await
    .map_err(|e| OidcError::Db(e.to_string()))?;
    if notify_user {
        if let Some(user_id) = user_id {
            super::consent::notify_oauth_security(
                pool,
                &user_id,
                "oauth_refresh_reuse",
                "oauth_token_family",
                family_id,
            )
            .await?;
        }
    }
    Ok(())
}

/// 撤销某用户在某 Client 的全部 family（consent 撤销 / 客户端停用时用）。
pub async fn revoke_families_for_user_client(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    client_internal_id: &str,
    reason: &str,
    now: i64,
) -> Result<u64, OidcError> {
    let affected = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE oauth_token_families SET revoked_at = ?, revoke_reason = ?
                 WHERE user_id = ? AND client_id = ? AND revoked_at IS NULL",
        )
        .bind(now)
        .bind(reason)
        .bind(user_id)
        .bind(client_internal_id)
        .execute(p)
        .await?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE oauth_token_families SET revoked_at = ?, revoke_reason = ?
                 WHERE user_id = ? AND client_id = ? AND revoked_at IS NULL",
        )
        .bind(now)
        .bind(reason)
        .bind(user_id)
        .bind(client_internal_id)
        .execute(p)
        .await?
        .rows_affected(),
    };
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE oauth_tokens SET revoked_at = ?, revoke_reason = ?
                 WHERE user_id = ? AND client_id = ? AND revoked_at IS NULL",
            )
            .bind(now)
            .bind(reason)
            .bind(user_id)
            .bind(client_internal_id)
            .execute(p)
            .await?;
        }
        Either::Right(p) => {
            sqlx::query(
                "UPDATE oauth_tokens SET revoked_at = ?, revoke_reason = ?
                 WHERE user_id = ? AND client_id = ? AND revoked_at IS NULL",
            )
            .bind(now)
            .bind(reason)
            .bind(user_id)
            .bind(client_internal_id)
            .execute(p)
            .await?;
        }
    }
    Ok(affected)
}

/// `/oauth/revoke`：撤销 access/refresh token（按 hint 或双表探测）。
/// 响应恒定 200，不泄漏 token 存在性（RFC 7009）。
pub async fn revoke_token(
    pool: &crate::db::DatabasePool,
    client: &OAuthClient,
    raw_token: &str,
    hint: Option<&str>,
    request_id: &str,
) -> Result<(), OidcError> {
    let token_hash = hash_token(raw_token);
    let is_refresh_hint = matches!(hint, Some("refresh_token"));
    let is_access_hint = matches!(hint, Some("access_token"));

    // 按 hint 优先；无 hint 时先 refresh 后 access。
    if !is_access_hint {
        let row = lookup_refresh_token(pool, &token_hash).await?;
        if let Some(row) = row {
            if row.client_id == client.id {
                revoke_family(pool, &row.family_id, "revoked_by_client", request_id, false).await?;
            }
            return Ok(());
        }
    }
    if !is_refresh_hint {
        let row = lookup_access_token(pool, &token_hash).await?;
        if let Some(row) = row {
            if row.client_id == client.id {
                // 撤销该 token 对；family 其余成员保留。
                match pool {
                    Either::Left(p) => {
                        sqlx::query("UPDATE oauth_tokens SET revoked_at = ?, revoke_reason = ? WHERE id = ?")
                            .bind(now_millis())
                            .bind("revoked_by_client")
                            .bind(&row.id)
                            .execute(p)
                            .await?;
                    }
                    Either::Right(p) => {
                        sqlx::query("UPDATE oauth_tokens SET revoked_at = ?, revoke_reason = ? WHERE id = ?")
                            .bind(now_millis())
                            .bind("revoked_by_client")
                            .bind(&row.id)
                            .execute(p)
                            .await?;
                    }
                }
            }
        }
    }
    Ok(())
}

/// userinfo 校验：返回 (TokenRow, FamilyRow, UserProjection, Client)。
#[allow(clippy::type_complexity)]
pub async fn validate_access_token(
    pool: &crate::db::DatabasePool,
    raw_access_token: &str,
    now: i64,
) -> Result<Option<(TokenRow, FamilyRow, UserProjection, OAuthClient)>, OidcError> {
    let access_hash = hash_token(raw_access_token);
    let Some(row) = lookup_access_token(pool, &access_hash).await? else {
        return Ok(None);
    };
    if row.revoked_at.is_some() {
        return Ok(None);
    }
    if row.issued_at + super::ACCESS_TOKEN_TTL_SECS * 1000 < now {
        return Ok(None);
    }
    let Some(family) = lookup_family(pool, &row.family_id).await? else {
        return Ok(None);
    };
    if family.revoked_at.is_some() {
        return Ok(None);
    }
    let Some(user) = fetch_user(pool, &row.user_id).await? else {
        return Ok(None);
    };
    if !user.is_active() {
        return Ok(None);
    }
    let Some(client) = super::clients::fetch_client_by_internal_id(pool, &row.client_id).await?
    else {
        return Ok(None);
    };
    if !client.is_active() {
        return Ok(None);
    }
    Ok(Some((row, family, user, client)))
}
