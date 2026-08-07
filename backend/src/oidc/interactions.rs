//! Consent 交互（M11-CONSENT-02）：授权请求 → interaction → 同意/拒绝。
//!
//! - `oauth_interactions` 保存 pending/approved/denied 状态，绑定原始请求
//!   摘要（request_hash，含可恢复的 nonce/state/PKCE challenge）；
//! - 查询与 decision 都要求 Session（路由层）；decision 要求 CSRF（全局
//!   CSRF 中间件），并绑定原始请求摘要；
//! - 同意时逐 scope 写 consent、签发一次性授权码并 303 回已验证
//!   redirect_uri；拒绝时返回标准 `access_denied`。

use serde_json::{json, Value};
use sqlx::Either;

use super::consent::{consents_are_active, grant_consents};
use super::protocol::{bind_request, unbind_request, AuthorizeRequest};
use super::OidcError;

/// interaction 行（`oauth_interactions`）。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct InteractionRow {
    pub id: String,
    pub client_id: String,
    pub user_id: String,
    pub request_hash: String,
    pub redirect_uri: String,
    pub scope: String,
    pub status: String,
    pub decision_at: Option<i64>,
    pub expires_at: i64,
    pub created_at: i64,
}

/// 创建 interaction（authorize 端点，认证后），返回 interaction id。
pub async fn create_interaction(
    pool: &crate::db::DatabasePool,
    client_internal_id: &str,
    user_id: &str,
    req: &AuthorizeRequest,
    master_key: &[u8],
    now: i64,
) -> Result<String, OidcError> {
    let binding = bind_request(req, master_key)?;
    let id = uuid::Uuid::now_v7().to_string();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO oauth_interactions
                    (id, client_id, user_id, request_hash, redirect_uri, scope, status, expires_at, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, 'pending', ?, ?)",
            )
            .bind(&id)
            .bind(client_internal_id)
            .bind(user_id)
            .bind(&binding)
            .bind(&req.redirect_uri)
            .bind(&req.scope)
            .bind(now + super::INTERACTION_TTL_MS)
            .bind(now)
            .execute(p)
            .await?;
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO oauth_interactions
                    (id, client_id, user_id, request_hash, redirect_uri, scope, status, expires_at, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, 'pending', ?, ?)",
            )
            .bind(&id)
            .bind(client_internal_id)
            .bind(user_id)
            .bind(&binding)
            .bind(&req.redirect_uri)
            .bind(&req.scope)
            .bind(now + super::INTERACTION_TTL_MS)
            .bind(now)
            .execute(p)
            .await?;
        }
    }
    Ok(id)
}

/// 查询 interaction（仅 owner 可见；不存在/非本人 → None）。
pub async fn get_interaction(
    pool: &crate::db::DatabasePool,
    id: &str,
    user_id: &str,
) -> Result<Option<InteractionRow>, OidcError> {
    let row: Option<InteractionRow> = match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, InteractionRow>(
                "SELECT id, client_id, user_id, request_hash, redirect_uri, scope, status, decision_at, expires_at, created_at
                 FROM oauth_interactions WHERE id = ? AND user_id = ?",
            )
            .bind(id)
            .bind(user_id)
            .fetch_optional(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_as::<_, InteractionRow>(
                "SELECT id, client_id, user_id, request_hash, redirect_uri, scope, status, decision_at, expires_at, created_at
                 FROM oauth_interactions WHERE id = ? AND user_id = ?",
            )
            .bind(id)
            .bind(user_id)
            .fetch_optional(p)
            .await?
        }
    };
    Ok(row)
}

/// interaction 查询投影（consent 页展示用，仅已验证的 Client/scope 摘要）。
pub async fn interaction_view(
    pool: &crate::db::DatabasePool,
    interaction: &InteractionRow,
    now: i64,
) -> Result<Value, OidcError> {
    let client = super::clients::fetch_client_by_internal_id(pool, &interaction.client_id)
        .await?
        .ok_or_else(|| OidcError::NotFound("interaction not found".into()))?;
    let scopes = super::protocol::split_scopes(&interaction.scope);
    let previously_consented = consents_are_active(
        pool,
        &interaction.user_id,
        &interaction.client_id,
        &scopes,
        now,
    )
    .await?;
    let redirect_domain = url::Url::parse(&interaction.redirect_uri)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_default();
    Ok(json!({
        "id": interaction.id,
        "client": {
            "client_id": client.client_id,
            "name": client.name,
        },
        "redirect_domain": redirect_domain,
        "scope": scopes,
        "previously_consented": previously_consented,
        "request_hash": super::protocol::binding_digest(&interaction.request_hash),
        "status": interaction.status,
        "expires_at": interaction.expires_at,
        "created_at": interaction.created_at,
    }))
}

/// decision 结果。
#[derive(Debug, Clone)]
pub enum InteractionOutcome {
    /// 同意：返回授权码与 state。
    Approved {
        code: String,
        state: Option<String>,
        redirect_uri: String,
        scope: String,
    },
    /// 拒绝：标准 `access_denied` 重定向。
    Denied {
        state: Option<String>,
        redirect_uri: String,
    },
}

/// 消费 interaction decision（allow/deny）。
///
/// - allow：逐 scope 写 consent → 签发授权码（绑定请求摘要/nonce/state/
///   PKCE）→ 标记 approved；
/// - deny：标记 denied → 返回 `access_denied` 重定向（含原 state）。
#[allow(clippy::too_many_arguments)]
pub async fn decide_interaction(
    pool: &crate::db::DatabasePool,
    interaction: &InteractionRow,
    allow: bool,
    master_key: &[u8],
    request_id: &str,
    now: i64,
) -> Result<InteractionOutcome, OidcError> {
    if interaction.status != "pending" {
        return Err(OidcError::InvalidRequest(
            "interaction has already been decided".into(),
        ));
    }
    if interaction.expires_at < now {
        return Err(OidcError::InvalidRequest(
            "interaction has expired; restart the authorization request".into(),
        ));
    }
    let req = unbind_request(&interaction.request_hash, master_key)?;
    let scopes = super::protocol::split_scopes(&interaction.scope);

    if !allow {
        match pool {
            Either::Left(p) => {
                sqlx::query("UPDATE oauth_interactions SET status = 'denied', decision_at = ? WHERE id = ? AND status = 'pending'")
                    .bind(now)
                    .bind(&interaction.id)
                    .execute(p)
                    .await?;
            }
            Either::Right(p) => {
                sqlx::query("UPDATE oauth_interactions SET status = 'denied', decision_at = ? WHERE id = ? AND status = 'pending'")
                    .bind(now)
                    .bind(&interaction.id)
                    .execute(p)
                    .await?;
            }
        }
        crate::audit::AuditEntry::user_action(&interaction.user_id, "oauth.interaction.deny")
            .with_target("oauth_interaction", &interaction.id)
            .with_request_id(request_id)
            .record(pool)
            .await
            .map_err(|e| OidcError::Db(e.to_string()))?;
        return Ok(InteractionOutcome::Denied {
            state: req.state,
            redirect_uri: interaction.redirect_uri.clone(),
        });
    }

    // 同意：consent + 授权码。
    let newly_granted = grant_consents(
        pool,
        &interaction.user_id,
        &interaction.client_id,
        &scopes,
        now,
    )
    .await?;
    if !newly_granted.is_empty() {
        // 首次授权或新增 scope：安全审计 + 通知（docs/AUTH-OIDC.md §15）。
        crate::audit::AuditEntry::user_action(&interaction.user_id, "oauth.consent.grant")
            .with_target("oauth_interaction", &interaction.id)
            .with_request_id(request_id)
            .with_metadata(json!({ "scopes": newly_granted }))
            .record(pool)
            .await
            .map_err(|e| OidcError::Db(e.to_string()))?;
        super::consent::notify_oauth_security(
            pool,
            &interaction.user_id,
            "oauth_consent_granted",
            "oauth_interaction",
            &interaction.id,
        )
        .await?;
    }

    let code = super::tokens::create_authorization_code(
        pool,
        &interaction.client_id,
        &interaction.user_id,
        &interaction.redirect_uri,
        &interaction.scope,
        req.nonce.as_deref(),
        req.state.as_deref(),
        &super::protocol::binding_digest(&interaction.request_hash),
        Some(req.code_challenge.as_str()),
        now,
    )
    .await?;

    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE oauth_interactions SET status = 'approved', decision_at = ? WHERE id = ? AND status = 'pending'")
                .bind(now)
                .bind(&interaction.id)
                .execute(p)
                .await?;
        }
        Either::Right(p) => {
            sqlx::query("UPDATE oauth_interactions SET status = 'approved', decision_at = ? WHERE id = ? AND status = 'pending'")
                .bind(now)
                .bind(&interaction.id)
                .execute(p)
                .await?;
        }
    }
    Ok(InteractionOutcome::Approved {
        code,
        state: req.state,
        redirect_uri: interaction.redirect_uri.clone(),
        scope: interaction.scope.clone(),
    })
}

/// 校验 interaction 状态/所有权（路由层复用）。
pub async fn load_interaction_for_owner(
    pool: &crate::db::DatabasePool,
    id: &str,
    user_id: &str,
) -> Result<InteractionRow, OidcError> {
    get_interaction(pool, id, user_id)
        .await?
        .ok_or_else(|| OidcError::NotFound("interaction not found".into()))
}

/// 生成 decision 重定向 URL。
pub fn decision_redirect_url(redirect_uri: &str, query: &[(String, String)]) -> String {
    let separator = if redirect_uri.contains('?') { "&" } else { "?" };
    let mut pairs: Vec<String> = query
        .iter()
        .map(|(k, v)| {
            format!(
                "{k}={}",
                url::form_urlencoded::byte_serialize(v.as_bytes()).collect::<String>()
            )
        })
        .collect();
    pairs.sort();
    format!("{redirect_uri}{separator}{}", pairs.join("&"))
}
