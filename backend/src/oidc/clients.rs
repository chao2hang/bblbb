//! OAuth Client 数据模型与管理（M11-OIDC-SCHEMA-01、M11-CONSENT-05）。
//!
//! - Public/Confidential Client；secret 只存 SHA-256 hash；
//! - redirect / post-logout URI 以 JSON 存储并精确校验（协议规则见
//!   [`crate::oidc::protocol`]）；
//! - scope 白名单、状态 active/disabled、版本化行。

use serde_json::{json, Value};
use sqlx::Either;

use super::{hash_token, OidcError};

/// OAuth Client 行（`oauth_clients`）。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OAuthClient {
    pub id: String,
    pub name: String,
    pub client_type: String,
    pub client_id: String,
    pub client_secret_hash: Option<String>,
    pub redirect_uris_json: String,
    pub post_logout_uris_json: Option<String>,
    pub scopes_json: String,
    pub status: String,
    pub version: i64,
    pub created_by: String,
    pub created_at: i64,
    pub updated_by: String,
    pub updated_at: i64,
}

impl OAuthClient {
    pub fn is_confidential(&self) -> bool {
        self.client_type == "confidential"
    }

    pub fn is_active(&self) -> bool {
        self.status == "active"
    }

    pub fn redirect_uris(&self) -> Vec<String> {
        serde_json::from_str(&self.redirect_uris_json).unwrap_or_default()
    }

    pub fn post_logout_uris(&self) -> Vec<String> {
        self.post_logout_uris_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }

    pub fn scopes(&self) -> Vec<String> {
        serde_json::from_str(&self.scopes_json).unwrap_or_default()
    }

    /// 校验 client secret（常量时间比较）。
    pub fn verify_secret(&self, secret: &str) -> bool {
        match &self.client_secret_hash {
            Some(expected) => {
                let provided = hash_token(secret);
                if provided.len() != expected.len() {
                    return false;
                }
                let mut diff = 0u8;
                for (a, b) in provided.as_bytes().iter().zip(expected.as_bytes()) {
                    diff |= a ^ b;
                }
                diff == 0
            }
            None => false,
        }
    }
}

/// 按公开 client_id 取 Client。
pub async fn fetch_client_by_client_id(
    pool: &crate::db::DatabasePool,
    client_id: &str,
) -> Result<Option<OAuthClient>, OidcError> {
    match pool {
        Either::Left(p) => Ok(sqlx::query_as::<_, OAuthClient>(
            "SELECT id, name, client_type, client_id, client_secret_hash, redirect_uris_json,
                    post_logout_uris_json, scopes_json, status, version, created_by, created_at, updated_by, updated_at
             FROM oauth_clients WHERE client_id = ?",
        )
        .bind(client_id)
        .fetch_optional(p)
        .await?),
        Either::Right(p) => Ok(sqlx::query_as::<_, OAuthClient>(
            "SELECT id, name, client_type, client_id, client_secret_hash, redirect_uris_json,
                    post_logout_uris_json, scopes_json, status, version, created_by, created_at, updated_by, updated_at
             FROM oauth_clients WHERE client_id = ?",
        )
        .bind(client_id)
        .fetch_optional(p)
        .await?),
    }
}

/// 按内部 id 取 Client。
pub async fn fetch_client_by_internal_id(
    pool: &crate::db::DatabasePool,
    id: &str,
) -> Result<Option<OAuthClient>, OidcError> {
    match pool {
        Either::Left(p) => Ok(sqlx::query_as::<_, OAuthClient>(
            "SELECT id, name, client_type, client_id, client_secret_hash, redirect_uris_json,
                    post_logout_uris_json, scopes_json, status, version, created_by, created_at, updated_by, updated_at
             FROM oauth_clients WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(p)
        .await?),
        Either::Right(p) => Ok(sqlx::query_as::<_, OAuthClient>(
            "SELECT id, name, client_type, client_id, client_secret_hash, redirect_uris_json,
                    post_logout_uris_json, scopes_json, status, version, created_by, created_at, updated_by, updated_at
             FROM oauth_clients WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(p)
        .await?),
    }
}

/// 分页列出 Client（`after` = 上一页最后一个内部 id，`limit` 1..=100）。
pub async fn list_clients(
    pool: &crate::db::DatabasePool,
    after: Option<&str>,
    limit: i64,
) -> Result<(Vec<OAuthClient>, Option<String>), OidcError> {
    let take = limit.clamp(1, 100) + 1;
    const COLS: &str = "id, name, client_type, client_id, client_secret_hash, redirect_uris_json,
                        post_logout_uris_json, scopes_json, status, version, created_by, created_at, updated_by, updated_at";
    let rows: Vec<OAuthClient> = match pool {
        Either::Left(p) => {
            let sql = match after {
                Some(_) => format!(
                    "SELECT {COLS} FROM oauth_clients WHERE id < ? ORDER BY created_at DESC, id DESC LIMIT ?"
                ),
                None => format!("SELECT {COLS} FROM oauth_clients ORDER BY created_at DESC, id DESC LIMIT ?"),
            };
            let mut q = sqlx::query_as::<_, OAuthClient>(&sql);
            if let Some(after) = after {
                q = q.bind(after);
            }
            q.bind(take).fetch_all(p).await?
        }
        Either::Right(p) => {
            let sql = match after {
                Some(_) => format!(
                    "SELECT {COLS} FROM oauth_clients WHERE id < ? ORDER BY created_at DESC, id DESC LIMIT ?"
                ),
                None => format!("SELECT {COLS} FROM oauth_clients ORDER BY created_at DESC, id DESC LIMIT ?"),
            };
            let mut q = sqlx::query_as::<_, OAuthClient>(&sql);
            if let Some(after) = after {
                q = q.bind(after);
            }
            q.bind(take).fetch_all(p).await?
        }
    };
    let next_cursor = if rows.len() as i64 > limit {
        Some(rows[rows.len() - 1].id.clone())
    } else {
        None
    };
    let visible = rows.into_iter().take(limit as usize).collect();
    Ok((visible, next_cursor))
}

/// 创建 Client 输入（管理员）。
#[derive(Debug, Clone)]
pub struct ClientCreateInput {
    pub name: String,
    pub client_type: String,
    pub redirect_uris: Vec<String>,
    pub post_logout_uris: Vec<String>,
    pub scopes: Vec<String>,
}

/// 更新 Client 输入（管理员；`None` = 不修改）。
#[derive(Debug, Clone, Default)]
pub struct ClientUpdateInput {
    pub name: Option<String>,
    pub client_type: Option<String>,
    pub redirect_uris: Option<Vec<String>>,
    pub post_logout_uris: Option<Vec<String>>,
    pub scopes: Option<Vec<String>>,
    pub status: Option<String>,
    pub reset_secret: Option<bool>,
}

/// 校验 Client 定义（URI 精确校验 + scope 白名单 + client type 枚举）。
pub fn validate_client_definition(input: &ClientCreateInput) -> Result<(), String> {
    let name = input.name.trim();
    if name.is_empty() || name.chars().count() > 120 {
        return Err("name must be 1-120 characters".to_string());
    }
    if !matches!(input.client_type.as_str(), "public" | "confidential") {
        return Err("client_type must be 'public' or 'confidential'".to_string());
    }
    if input.redirect_uris.is_empty() {
        return Err("at least one redirect URI is required".to_string());
    }
    for uri in &input.redirect_uris {
        super::protocol::validate_redirect_uri(uri).map_err(|e| e.to_string())?;
    }
    for uri in &input.post_logout_uris {
        super::protocol::validate_post_logout_uri(uri).map_err(|e| e.to_string())?;
    }
    if input.scopes.is_empty() {
        return Err("at least one scope is required".to_string());
    }
    for scope in &input.scopes {
        if !super::SCOPE_SET.split(' ').any(|allowed| allowed == scope) {
            return Err(format!("unsupported scope '{scope}'"));
        }
    }
    Ok(())
}

/// 创建 Client：生成 client_id + confidential secret（仅返回一次）。
/// 返回 (Client, 明文 secret 或 None)。
#[allow(clippy::too_many_arguments)]
pub async fn create_client(
    pool: &crate::db::DatabasePool,
    input: &ClientCreateInput,
    created_by: &str,
    now: i64,
) -> Result<(OAuthClient, Option<String>), OidcError> {
    validate_client_definition(input).map_err(OidcError::InvalidRequest)?;

    let id = uuid::Uuid::now_v7().to_string();
    let public_client_id = uuid::Uuid::now_v7().to_string();
    let secret = if input.client_type == "confidential" {
        Some(crate::auth::token::generate_token())
    } else {
        None
    };
    let secret_hash = secret.as_deref().map(hash_token);
    let redirect_json = serde_json::to_string(&input.redirect_uris)
        .map_err(|e| OidcError::ServerError(e.to_string()))?;
    let post_logout_json = if input.post_logout_uris.is_empty() {
        None
    } else {
        Some(
            serde_json::to_string(&input.post_logout_uris)
                .map_err(|e| OidcError::ServerError(e.to_string()))?,
        )
    };
    let scopes_json =
        serde_json::to_string(&input.scopes).map_err(|e| OidcError::ServerError(e.to_string()))?;

    let affected = match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO oauth_clients
                    (id, name, client_type, client_id, client_secret_hash, redirect_uris_json,
                     post_logout_uris_json, scopes_json, status, version, created_by, created_at, updated_by, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'active', 1, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(input.name.trim())
            .bind(&input.client_type)
            .bind(&public_client_id)
            .bind(&secret_hash)
            .bind(&redirect_json)
            .bind(&post_logout_json)
            .bind(&scopes_json)
            .bind(created_by)
            .bind(now)
            .bind(created_by)
            .bind(now)
            .execute(p)
            .await?
            .rows_affected()
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO oauth_clients
                    (id, name, client_type, client_id, client_secret_hash, redirect_uris_json,
                     post_logout_uris_json, scopes_json, status, version, created_by, created_at, updated_by, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'active', 1, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(input.name.trim())
            .bind(&input.client_type)
            .bind(&public_client_id)
            .bind(&secret_hash)
            .bind(&redirect_json)
            .bind(&post_logout_json)
            .bind(&scopes_json)
            .bind(created_by)
            .bind(now)
            .bind(created_by)
            .bind(now)
            .execute(p)
            .await?
            .rows_affected()
        }
    };
    if affected != 1 {
        return Err(OidcError::ServerError(
            "client insert affected 0 rows".into(),
        ));
    }

    let client = OAuthClient {
        id,
        name: input.name.trim().to_string(),
        client_type: input.client_type.clone(),
        client_id: public_client_id,
        client_secret_hash: secret_hash.clone(),
        redirect_uris_json: redirect_json,
        post_logout_uris_json: post_logout_json,
        scopes_json,
        status: "active".to_string(),
        version: 1,
        created_by: created_by.to_string(),
        created_at: now,
        updated_by: created_by.to_string(),
        updated_at: now,
    };
    Ok((client, secret))
}

/// 更新 Client（乐观锁 version+1；写 updated_by/updated_at）。返回受影响行数。
#[allow(clippy::too_many_arguments)]
pub async fn update_client(
    pool: &crate::db::DatabasePool,
    client: &OAuthClient,
    input: &ClientUpdateInput,
    actor: &str,
    now: i64,
) -> Result<(), OidcError> {
    let name = input.name.as_deref().unwrap_or(&client.name);
    if name.trim().is_empty() || name.trim().chars().count() > 120 {
        return Err(OidcError::InvalidRequest(
            "name must be 1-120 characters".into(),
        ));
    }
    let client_type = input.client_type.as_deref().unwrap_or(&client.client_type);
    if !matches!(client_type, "public" | "confidential") {
        return Err(OidcError::InvalidRequest(
            "client_type must be 'public' or 'confidential'".into(),
        ));
    }
    let redirect_uris = match &input.redirect_uris {
        Some(uris) => {
            if uris.is_empty() {
                return Err(OidcError::InvalidRequest(
                    "at least one redirect URI is required".into(),
                ));
            }
            for uri in uris {
                super::protocol::validate_redirect_uri(uri)
                    .map_err(|e| OidcError::InvalidRequest(e.to_string()))?;
            }
            uris.clone()
        }
        None => client.redirect_uris(),
    };
    let post_logout_uris = match &input.post_logout_uris {
        Some(uris) => {
            for uri in uris {
                super::protocol::validate_post_logout_uri(uri)
                    .map_err(|e| OidcError::InvalidRequest(e.to_string()))?;
            }
            uris.clone()
        }
        None => client.post_logout_uris(),
    };
    let scopes = match &input.scopes {
        Some(scopes) => {
            if scopes.is_empty() {
                return Err(OidcError::InvalidRequest(
                    "at least one scope is required".into(),
                ));
            }
            for scope in scopes {
                if !super::SCOPE_SET.split(' ').any(|a| a == scope) {
                    return Err(OidcError::InvalidRequest(format!(
                        "unsupported scope '{scope}'"
                    )));
                }
            }
            scopes.clone()
        }
        None => client.scopes(),
    };
    let status = input.status.as_deref().unwrap_or(&client.status);
    if !matches!(status, "active" | "disabled") {
        return Err(OidcError::InvalidRequest(
            "status must be 'active' or 'disabled'".into(),
        ));
    }

    let redirect_json =
        serde_json::to_string(&redirect_uris).map_err(|e| OidcError::ServerError(e.to_string()))?;
    let post_logout_json = if post_logout_uris.is_empty() {
        None
    } else {
        Some(
            serde_json::to_string(&post_logout_uris)
                .map_err(|e| OidcError::ServerError(e.to_string()))?,
        )
    };
    let scopes_json =
        serde_json::to_string(&scopes).map_err(|e| OidcError::ServerError(e.to_string()))?;

    let new_version = client.version + 1;
    let affected = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE oauth_clients
                 SET name = ?, client_type = ?, redirect_uris_json = ?, post_logout_uris_json = ?,
                     scopes_json = ?, status = ?, version = ?, updated_by = ?, updated_at = ?
                 WHERE id = ? AND version = ?",
        )
        .bind(name.trim())
        .bind(client_type)
        .bind(&redirect_json)
        .bind(&post_logout_json)
        .bind(&scopes_json)
        .bind(status)
        .bind(new_version)
        .bind(actor)
        .bind(now)
        .bind(&client.id)
        .bind(client.version)
        .execute(p)
        .await?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE oauth_clients
                 SET name = ?, client_type = ?, redirect_uris_json = ?, post_logout_uris_json = ?,
                     scopes_json = ?, status = ?, version = ?, updated_by = ?, updated_at = ?
                 WHERE id = ? AND version = ?",
        )
        .bind(name.trim())
        .bind(client_type)
        .bind(&redirect_json)
        .bind(&post_logout_json)
        .bind(&scopes_json)
        .bind(status)
        .bind(new_version)
        .bind(actor)
        .bind(now)
        .bind(&client.id)
        .bind(client.version)
        .execute(p)
        .await?
        .rows_affected(),
    };
    if affected != 1 {
        return Err(OidcError::InvalidRequest(
            "client version conflict; reload and retry".into(),
        ));
    }
    Ok(())
}

/// 更新 secret hash（secret 重置；仅用于调用方已生成新 secret 的场景）。
pub async fn update_client_secret(
    pool: &crate::db::DatabasePool,
    client_id_internal: &str,
    secret_hash: &str,
    actor: &str,
    now: i64,
) -> Result<(), OidcError> {
    let affected = match pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE oauth_clients SET client_secret_hash = ?, updated_by = ?, updated_at = ?, version = version + 1
                 WHERE id = ?",
            )
            .bind(secret_hash)
            .bind(actor)
            .bind(now)
            .bind(client_id_internal)
            .execute(p)
            .await?
            .rows_affected()
        }
        Either::Right(p) => {
            sqlx::query(
                "UPDATE oauth_clients SET client_secret_hash = ?, updated_by = ?, updated_at = ?, version = version + 1
                 WHERE id = ?",
            )
            .bind(secret_hash)
            .bind(actor)
            .bind(now)
            .bind(client_id_internal)
            .execute(p)
            .await?
            .rows_affected()
        }
    };
    if affected != 1 {
        return Err(OidcError::NotFound("client not found".into()));
    }
    Ok(())
}

/// 管理员视图投影（不含 secret hash 原文）。
pub fn client_admin_view(client: &OAuthClient) -> Value {
    json!({
        "id": client.id,
        "name": client.name,
        "client_id": client.client_id,
        "client_type": client.client_type,
        "redirect_uris": client.redirect_uris(),
        "post_logout_uris": client.post_logout_uris(),
        "scopes": client.scopes(),
        "status": client.status,
        "version": client.version,
        "secret_configured": client.client_secret_hash.is_some(),
        "created_by": client.created_by,
        "created_at": client.created_at,
        "updated_by": client.updated_by,
        "updated_at": client.updated_at,
    })
}
