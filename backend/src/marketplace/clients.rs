//! M12-CLIENTS：Marketplace Client / Scope / Offer 管理与服务认证。
//!
//! 安全边界（docs/MARKETPLACE.md §2/§3，docs/MARKETPLACE-ACCOUNTING.md §5）：
//! - 只有管理员审批通过的 Confidential OAuth Client 可接入 Marketplace；
//! - client secret 与 webhook secret 只存 SHA-256 hash，明文只在创建/轮换时
//!   返回一次；
//! - redirect/webhook URL 强制 HTTPS + SSRF 防护（私网/回环/链路本地拒绝）；
//! - 逐应用 × 逐 scope 审批（`client_scopes`：pending/approved/disabled、
//!   限额 JSON、version、effective_at、审批/撤销审计）；
//! - 服务操作（offer.write / purchases.read / refund / webhook.manage）要求
//!   Confidential Client 的 client_secret_basic/post 认证或管理员（reason +
//!   recent-auth）；
//! - 普通 OIDC scope（openid/profile/email）永远不能调用扣款接口
//!   （M11-CONSENT-06 冻结白名单；本模块的 marketplace.* scope 不进入
//!   OAuth 白名单）；
//! - merchant balance 只站内可追踪，不提现/不兑换（M07-LEDGER-05 词表拦截
//!   兜底）；
//! - Client/Scope 紧急停用立即阻止新 Intent/confirm/refund，历史可查询。

use serde_json::{json, Value};
use sqlx::{Either, Row};

use crate::ai::gateway::is_private_ip;
use crate::audit::AuditEntry;
use crate::auth::token::generate_token;
use crate::authz::decision::AUTHZ_POLICY_VERSION;
use crate::db::DatabasePool;
use crate::marketplace::MarketplaceError;

/// Marketplace scope 名称（不进入 OIDC 白名单；docs/MARKETPLACE.md §2）。
pub const USER_SCOPES: [&str; 2] = ["marketplace.checkout.create", "marketplace.purchase"];
pub const SERVICE_SCOPES: [&str; 4] = [
    "marketplace.offer.write",
    "marketplace.purchases.read",
    "marketplace.refund",
    "marketplace.webhook.manage",
];
pub const ALL_SCOPES: [&str; 6] = [
    "marketplace.checkout.create",
    "marketplace.purchase",
    "marketplace.offer.write",
    "marketplace.purchases.read",
    "marketplace.refund",
    "marketplace.webhook.manage",
];

pub fn is_valid_scope(scope: &str) -> bool {
    ALL_SCOPES.contains(&scope)
}

/// 正常运营状态（未停用/未禁用/未紧急停用）。
pub fn is_operational_status(status: &str) -> bool {
    matches!(status, "active")
}

/// Marketplace Client 行。
#[derive(Debug, Clone)]
pub struct MarketplaceClient {
    pub id: String,
    pub client_id: String,
    pub oauth_client_id: String,
    pub owner_user_id: String,
    pub name: String,
    pub status: String,
    pub terms_url: String,
    pub privacy_url: String,
    pub webhook_url: Option<String>,
    pub webhook_secret_hash: Option<String>,
    pub webhook_secret_version: i64,
    pub redirect_uris_json: String,
    pub fee_bps: i64,
    pub version: i64,
    pub approval_history_json: String,
    pub created_by: String,
    pub created_at: i64,
    pub updated_by: String,
    pub updated_at: i64,
}

impl MarketplaceClient {
    pub fn is_active(&self) -> bool {
        self.status == "active"
    }

    /// 是否允许新销售（active；pending/disabled/emergency_disabled 均拒绝）。
    pub fn allows_new_sales(&self) -> bool {
        self.is_active()
    }

    pub fn redirect_uris(&self) -> Vec<String> {
        serde_json::from_str(&self.redirect_uris_json).unwrap_or_default()
    }
}

const CLIENT_COLUMNS: &str = "id, client_id, oauth_client_id, owner_user_id, name, status, \
     terms_url, privacy_url, webhook_url, webhook_secret_hash, webhook_secret_version, \
     redirect_uris_json, fee_bps, version, approval_history_json, created_by, created_at, \
     updated_by, updated_at";

fn client_from_sqlite(row: &sqlx::sqlite::SqliteRow) -> MarketplaceClient {
    MarketplaceClient {
        id: row.get("id"),
        client_id: row.get("client_id"),
        oauth_client_id: row.get("oauth_client_id"),
        owner_user_id: row.get("owner_user_id"),
        name: row.get("name"),
        status: row.get("status"),
        terms_url: row.get("terms_url"),
        privacy_url: row.get("privacy_url"),
        webhook_url: row.get("webhook_url"),
        webhook_secret_hash: row.get("webhook_secret_hash"),
        webhook_secret_version: row.get("webhook_secret_version"),
        redirect_uris_json: row.get("redirect_uris_json"),
        fee_bps: row.get("fee_bps"),
        version: row.get("version"),
        approval_history_json: row.get("approval_history_json"),
        created_by: row.get("created_by"),
        created_at: row.get("created_at"),
        updated_by: row.get("updated_by"),
        updated_at: row.get("updated_at"),
    }
}

fn client_from_mysql(row: &sqlx::mysql::MySqlRow) -> MarketplaceClient {
    MarketplaceClient {
        id: row.get("id"),
        client_id: row.get("client_id"),
        oauth_client_id: row.get("oauth_client_id"),
        owner_user_id: row.get("owner_user_id"),
        name: row.get("name"),
        status: row.get("status"),
        terms_url: row.get("terms_url"),
        privacy_url: row.get("privacy_url"),
        webhook_url: row.get("webhook_url"),
        webhook_secret_hash: row.get("webhook_secret_hash"),
        webhook_secret_version: row.get("webhook_secret_version"),
        redirect_uris_json: row.get("redirect_uris_json"),
        fee_bps: row.get("fee_bps"),
        version: row.get("version"),
        approval_history_json: row.get("approval_history_json"),
        created_by: row.get("created_by"),
        created_at: row.get("created_at"),
        updated_by: row.get("updated_by"),
        updated_at: row.get("updated_at"),
    }
}

/// 按公开 client_id 取 Marketplace Client。
pub async fn fetch_client_by_client_id(
    pool: &DatabasePool,
    client_id: &str,
) -> Result<Option<MarketplaceClient>, MarketplaceError> {
    let sql = format!("SELECT {CLIENT_COLUMNS} FROM marketplace_clients WHERE client_id = ?");
    let row = match pool {
        Either::Left(p) => sqlx::query(&sql)
            .bind(client_id)
            .fetch_optional(p)
            .await?
            .map(|r| client_from_sqlite(&r)),
        Either::Right(p) => sqlx::query(&sql)
            .bind(client_id)
            .fetch_optional(p)
            .await?
            .map(|r| client_from_mysql(&r)),
    };
    Ok(row)
}

/// 按内部 id 取 Marketplace Client。
pub async fn fetch_client_by_internal_id(
    pool: &DatabasePool,
    id: &str,
) -> Result<Option<MarketplaceClient>, MarketplaceError> {
    let sql = format!("SELECT {CLIENT_COLUMNS} FROM marketplace_clients WHERE id = ?");
    let row = match pool {
        Either::Left(p) => sqlx::query(&sql)
            .bind(id)
            .fetch_optional(p)
            .await?
            .map(|r| client_from_sqlite(&r)),
        Either::Right(p) => sqlx::query(&sql)
            .bind(id)
            .fetch_optional(p)
            .await?
            .map(|r| client_from_mysql(&r)),
    };
    Ok(row)
}

/// 按内部 id 取 Marketplace Client（事务/连接内）。
pub async fn fetch_client_by_internal_id_conn(
    conn: &mut sqlx::SqliteConnection,
    id: &str,
) -> Result<Option<MarketplaceClient>, MarketplaceError> {
    let sql = format!("SELECT {CLIENT_COLUMNS} FROM marketplace_clients WHERE id = ?");
    let row = sqlx::query(&sql)
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .map(|r| client_from_sqlite(&r));
    Ok(row)
}

/// 按内部 id 取 Marketplace Client（MySQL 事务内）。
pub async fn fetch_client_by_internal_id_conn_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    id: &str,
) -> Result<Option<MarketplaceClient>, MarketplaceError> {
    let sql = format!("SELECT {CLIENT_COLUMNS} FROM marketplace_clients WHERE id = ?");
    let row = sqlx::query(&sql)
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .map(|r| client_from_mysql(&r));
    Ok(row)
}

/// scope 是否已批准（连接内）。
pub async fn scope_approved_conn(
    conn: &mut sqlx::SqliteConnection,
    client_id: &str,
    scope: &str,
) -> Result<bool, MarketplaceError> {
    let status: Option<String> =
        sqlx::query_scalar("SELECT status FROM client_scopes WHERE client_id = ? AND scope = ?")
            .bind(client_id)
            .bind(scope)
            .fetch_optional(&mut *conn)
            .await?;
    Ok(status.as_deref() == Some("approved"))
}

/// scope 是否已批准（MySQL 事务内）。
pub async fn scope_approved_conn_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    client_id: &str,
    scope: &str,
) -> Result<bool, MarketplaceError> {
    let status: Option<String> =
        sqlx::query_scalar("SELECT status FROM client_scopes WHERE client_id = ? AND scope = ?")
            .bind(client_id)
            .bind(scope)
            .fetch_optional(&mut **tx)
            .await?;
    Ok(status.as_deref() == Some("approved"))
}

/// 管理员列出 Client（cursor 分页；包含 scope 与商户余额摘要）。
pub async fn list_clients(
    pool: &DatabasePool,
    after: Option<&str>,
    limit: i64,
) -> Result<(Vec<Value>, Option<String>), MarketplaceError> {
    let limit = limit.clamp(1, 100);
    let (base, bind_cursor): (&str, bool) = if after.is_some() {
        (
            "SELECT {CLIENT_COLUMNS} FROM marketplace_clients WHERE id > ? ORDER BY created_at ASC LIMIT ?",
            true,
        )
    } else {
        (
            "SELECT {CLIENT_COLUMNS} FROM marketplace_clients ORDER BY created_at ASC LIMIT ?",
            false,
        )
    };
    let sql = base.replace("{CLIENT_COLUMNS}", CLIENT_COLUMNS);
    let rows: Vec<Value> = match pool {
        Either::Left(p) => {
            let result = if bind_cursor {
                sqlx::query(&sql)
                    .bind(after.unwrap_or(""))
                    .bind(limit + 1)
                    .fetch_all(p)
                    .await?
            } else {
                sqlx::query(&sql).bind(limit + 1).fetch_all(p).await?
            };
            result
                .iter()
                .map(|r| client_view_json(&client_from_sqlite(r)))
                .collect()
        }
        Either::Right(p) => {
            let result = if bind_cursor {
                sqlx::query(&sql)
                    .bind(after.unwrap_or(""))
                    .bind(limit + 1)
                    .fetch_all(p)
                    .await?
            } else {
                sqlx::query(&sql).bind(limit + 1).fetch_all(p).await?
            };
            result
                .iter()
                .map(|r| client_view_json(&client_from_mysql(r)))
                .collect()
        }
    };
    let has_more = rows.len() as i64 > limit;
    let mut items = rows;
    if has_more {
        items.pop();
    }
    let next_cursor = if has_more {
        items
            .last()
            .map(|v| v["id"].as_str().unwrap_or("").to_string())
            .filter(|s| !s.is_empty())
    } else {
        None
    };
    Ok((items, next_cursor))
}

/// Client 管理视图（不含 secret hash）。
pub fn client_view_json(c: &MarketplaceClient) -> Value {
    json!({
        "id": c.id,
        "client_id": c.client_id,
        "owner_user_id": c.owner_user_id,
        "name": c.name,
        "status": c.status,
        "terms_url": c.terms_url,
        "privacy_url": c.privacy_url,
        "webhook_url": c.webhook_url,
        "webhook_secret_version": c.webhook_secret_version,
        "redirect_uris": c.redirect_uris(),
        "fee_bps": c.fee_bps,
        "version": c.version,
        "approval_history": serde_json::from_str::<Value>(&c.approval_history_json).unwrap_or(Value::Array(vec![])),
        "created_at": c.created_at,
        "updated_at": c.updated_at,
    })
}

// ─────────────────────────── URL / SSRF 校验 ───────────────────────────

/// 校验 HTTPS + SSRF 安全 URL（webhook 目标）。
///
/// 拒绝：非 HTTPS、包含 userinfo、含 fragment、私网/回环/链路本地/
/// CGNAT/文档段 IP、空主机。域名类 host 由发送时的 DNS 重绑定复核兜底。
pub fn validate_webhook_url(raw: &str) -> Result<(), MarketplaceError> {
    validate_https_url_impl(raw, true)
}

/// 校验 redirect URI（HTTPS，精确匹配由 OIDC 模块负责；私网拒绝）。
pub fn validate_redirect_uri(raw: &str) -> Result<(), MarketplaceError> {
    validate_https_url_impl(raw, true)
}

fn validate_https_url_impl(raw: &str, reject_private: bool) -> Result<(), MarketplaceError> {
    let parsed =
        url::Url::parse(raw).map_err(|_| MarketplaceError::InvalidUrl("invalid URL".into()))?;
    if parsed.scheme() != "https" {
        return Err(MarketplaceError::InvalidUrl(
            "URL must use https".to_string(),
        ));
    }
    if parsed.username() != "" || parsed.password().is_some() {
        return Err(MarketplaceError::InvalidUrl(
            "URL must not contain userinfo".to_string(),
        ));
    }
    if parsed.fragment().is_some() {
        return Err(MarketplaceError::InvalidUrl(
            "URL must not contain a fragment".to_string(),
        ));
    }
    if parsed.host().is_none() {
        return Err(MarketplaceError::InvalidUrl(
            "URL must have a host".to_string(),
        ));
    }
    if reject_private {
        // 用结构化 host 判定（IPv6 字面量带括号，host_str 无法直接 parse）。
        match parsed.host() {
            Some(url::Host::Ipv4(ip)) => {
                if is_private_ip(&std::net::IpAddr::V4(ip)) {
                    return Err(MarketplaceError::UrlBlocked(
                        "private / loopback / link-local addresses are blocked".to_string(),
                    ));
                }
            }
            Some(url::Host::Ipv6(ip)) if is_private_ip(&std::net::IpAddr::V6(ip)) => {
                return Err(MarketplaceError::UrlBlocked(
                    "private / loopback / link-local addresses are blocked".to_string(),
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

/// 校验 URL 数组（redirect_uris）。
pub fn validate_redirect_uris(uris: &[String]) -> Result<(), MarketplaceError> {
    if uris.is_empty() {
        return Err(MarketplaceError::Invalid(
            "at least one redirect URI required".to_string(),
        ));
    }
    for uri in uris {
        validate_redirect_uri(uri)?;
    }
    Ok(())
}

/// 规范化 URL 数组 → JSON。
pub fn uris_json(uris: &[String]) -> String {
    serde_json::to_string(uris).unwrap_or_else(|_| "[]".to_string())
}

// ─────────────────────────── 注册 / 更新 / 停用 ───────────────────────────

/// 管理员注册或更新 Marketplace Client。
///
/// `key` 是 OAuth client_id 或内部 id；不存在时按注册创建（关联该
/// Confidential Client），存在时按 If-Match 更新。`scopes` 数组可选，
/// 提供时按 status 逐 scope 审批/禁用（M12-CLIENTS-03）。
/// status 变更为 active 时创建商户账户并追加审批历史（M12-CLIENTS-06）。
#[allow(clippy::too_many_arguments)]
pub async fn upsert_client(
    pool: &DatabasePool,
    key: &str,
    body: &Value,
    expected_version: i64,
    actor_id: &str,
    actor_name: &str,
    now: i64,
) -> Result<MarketplaceClient, MarketplaceError> {
    let existing = match fetch_client_by_client_id(pool, key).await? {
        Some(c) => Some(c),
        None => fetch_client_by_internal_id(pool, key).await?,
    };

    let name = str_field(body, "name")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            existing
                .as_ref()
                .map(|c| c.name.clone())
                .unwrap_or_default()
        });
    let owner_user_id = str_field(body, "owner_user_id")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            existing
                .as_ref()
                .map(|c| c.owner_user_id.clone())
                .unwrap_or_default()
        });
    let terms_url = str_field(body, "terms_url")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            existing
                .as_ref()
                .map(|c| c.terms_url.clone())
                .unwrap_or_default()
        });
    let privacy_url = str_field(body, "privacy_url")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            existing
                .as_ref()
                .map(|c| c.privacy_url.clone())
                .unwrap_or_default()
        });
    let webhook_url = optional_str_field(body, "webhook_url")
        .or_else(|| existing.as_ref().and_then(|c| c.webhook_url.clone()));
    let redirect_uris = array_field(body, "redirect_uris");
    let fee_bps = body
        .get("fee_bps")
        .and_then(Value::as_i64)
        .unwrap_or(existing.as_ref().map(|c| c.fee_bps).unwrap_or(0));
    let target_status = str_field(body, "status").unwrap_or_else(|| {
        existing
            .as_ref()
            .map(|c| c.status.clone())
            .unwrap_or("pending".into())
    });

    if !(0..=10_000).contains(&fee_bps) {
        return Err(MarketplaceError::Invalid(
            "fee_bps must be 0..=10000".into(),
        ));
    }
    if !matches!(
        target_status.as_str(),
        "pending" | "active" | "disabled" | "emergency_disabled"
    ) {
        return Err(MarketplaceError::Invalid("invalid client status".into()));
    }

    match existing {
        Some(client) => {
            if expected_version != client.version {
                return Err(MarketplaceError::VersionConflict {
                    expected: expected_version,
                    current: client.version,
                });
            }
            // webhook URL 变更校验。
            if let Some(url) = &webhook_url {
                validate_webhook_url(url)?;
            }
            let uris = if redirect_uris.is_empty() {
                client.redirect_uris()
            } else {
                redirect_uris.clone()
            };
            validate_redirect_uris(&uris)?;
            let uris_json = uris_json(&uris);

            let new_version = client.version + 1;
            let new_status = if client.status == "emergency_disabled" && target_status == "active" {
                return Err(MarketplaceError::Forbidden(
                    "emergency_disabled requires explicit re-approval; use status=disabled first then active".into(),
                ));
            } else {
                target_status.clone()
            };

            let mut history: Vec<Value> =
                serde_json::from_str(&client.approval_history_json).unwrap_or_default();
            history.push(json!({
                "action": if new_status == "active" { "approve" } else if new_status == "disabled" { "disable" } else { "update" },
                "from": client.status,
                "to": new_status,
                "by": actor_name,
                "at": now,
            }));

            let rows = match pool {
                Either::Left(p) => sqlx::query(
                    "UPDATE marketplace_clients
                     SET name = ?, owner_user_id = ?, terms_url = ?, privacy_url = ?, webhook_url = ?,
                         redirect_uris_json = ?, fee_bps = ?, status = ?, version = ?, approval_history_json = ?,
                         updated_by = ?, updated_at = ?
                     WHERE id = ? AND version = ?",
                )
                .bind(&name)
                .bind(&owner_user_id)
                .bind(&terms_url)
                .bind(&privacy_url)
                .bind(&webhook_url)
                .bind(&uris_json)
                .bind(fee_bps)
                .bind(&new_status)
                .bind(new_version)
                .bind(serde_json::to_string(&history).unwrap_or_else(|_| "[]".into()))
                .bind(actor_id)
                .bind(now)
                .bind(&client.id)
                .bind(expected_version)
                .execute(p)
                .await?
                .rows_affected(),
                Either::Right(p) => sqlx::query(
                    "UPDATE marketplace_clients
                     SET name = ?, owner_user_id = ?, terms_url = ?, privacy_url = ?, webhook_url = ?,
                         redirect_uris_json = ?, fee_bps = ?, status = ?, version = ?, approval_history_json = ?,
                         updated_by = ?, updated_at = ?
                     WHERE id = ? AND version = ?",
                )
                .bind(&name)
                .bind(&owner_user_id)
                .bind(&terms_url)
                .bind(&privacy_url)
                .bind(&webhook_url)
                .bind(&uris_json)
                .bind(fee_bps)
                .bind(&new_status)
                .bind(new_version)
                .bind(serde_json::to_string(&history).unwrap_or_else(|_| "[]".into()))
                .bind(actor_id)
                .bind(now)
                .bind(&client.id)
                .bind(expected_version)
                .execute(p)
                .await?
                .rows_affected(),
            };
            if rows != 1 {
                return Err(MarketplaceError::VersionConflict {
                    expected: expected_version,
                    current: client.version,
                });
            }

            if new_status == "active" && client.status != "active" {
                ensure_merchant_account(pool, &client.id, &owner_user_id, now).await?;
            }
            // 逐 scope 审批/禁用。
            if let Some(Value::Array(scopes)) = body.get("scopes") {
                for scope_entry in scopes {
                    apply_scope_entry(pool, &client.id, scope_entry, actor_id, actor_name, now)
                        .await?;
                }
            }
            let updated = fetch_client_by_internal_id(pool, &client.id)
                .await?
                .ok_or_else(|| MarketplaceError::NotFound("client".into()))?;
            Ok(updated)
        }
        None => {
            // 注册：必须关联存在的 Confidential OAuth Client。
            let oauth = crate::oidc::clients::fetch_client_by_client_id(pool, key)
                .await
                .map_err(|e| MarketplaceError::Db(e.to_string()))?
                .ok_or_else(|| {
                    MarketplaceError::InvalidClient(
                        "OAuth client not found; create a confidential OAuth client first".into(),
                    )
                })?;
            if !oauth.is_confidential() {
                return Err(MarketplaceError::InvalidClient(
                    "only confidential OAuth clients can join the marketplace".into(),
                ));
            }
            if oauth.status != "active" {
                return Err(MarketplaceError::InvalidClient(
                    "OAuth client is not active".into(),
                ));
            }
            if name.is_empty() {
                return Err(MarketplaceError::Invalid("name required".into()));
            }
            if owner_user_id.is_empty() {
                return Err(MarketplaceError::Invalid("owner_user_id required".into()));
            }
            if terms_url.is_empty() {
                return Err(MarketplaceError::Invalid("terms_url required".into()));
            }
            if privacy_url.is_empty() {
                return Err(MarketplaceError::Invalid("privacy_url required".into()));
            }
            validate_webhook_url_if_present(&webhook_url)?;
            validate_https_url_impl(&terms_url, true)?;
            validate_https_url_impl(&privacy_url, true)?;
            if redirect_uris.is_empty() {
                return Err(MarketplaceError::Invalid(
                    "at least one redirect URI required".into(),
                ));
            }
            validate_redirect_uris(&redirect_uris)?;
            if !matches!(target_status.as_str(), "pending" | "active" | "disabled") {
                return Err(MarketplaceError::Invalid(
                    "new client can only be pending/active/disabled".into(),
                ));
            }
            let id = uuid::Uuid::now_v7().to_string();
            let history = json!([{
                "action": "register",
                "from": null,
                "to": target_status,
                "by": actor_name,
                "at": now,
            }]);
            let rows = match pool {
                Either::Left(p) => sqlx::query(
                    "INSERT INTO marketplace_clients
                     (id, client_id, oauth_client_id, owner_user_id, name, status, terms_url, privacy_url,
                      webhook_url, webhook_secret_hash, webhook_secret_version, redirect_uris_json, fee_bps,
                      version, approval_history_json, created_by, created_at, updated_by, updated_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, 0, ?, ?, 1, ?, ?, ?, ?, ?)",
                )
                .bind(&id)
                .bind(&oauth.client_id)
                .bind(&oauth.id)
                .bind(&owner_user_id)
                .bind(&name)
                .bind(&target_status)
                .bind(&terms_url)
                .bind(&privacy_url)
                .bind(&webhook_url)
                .bind(uris_json(&redirect_uris))
                .bind(fee_bps)
                .bind(serde_json::to_string(&history).unwrap_or_else(|_| "[]".into()))
                .bind(actor_id)
                .bind(now)
                .bind(actor_id)
                .bind(now)
                .execute(p)
                .await?
                .rows_affected(),
                Either::Right(p) => sqlx::query(
                    "INSERT INTO marketplace_clients
                     (id, client_id, oauth_client_id, owner_user_id, name, status, terms_url, privacy_url,
                      webhook_url, webhook_secret_hash, webhook_secret_version, redirect_uris_json, fee_bps,
                      version, approval_history_json, created_by, created_at, updated_by, updated_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, 0, ?, ?, 1, ?, ?, ?, ?, ?)",
                )
                .bind(&id)
                .bind(&oauth.client_id)
                .bind(&oauth.id)
                .bind(&owner_user_id)
                .bind(&name)
                .bind(&target_status)
                .bind(&terms_url)
                .bind(&privacy_url)
                .bind(&webhook_url)
                .bind(uris_json(&redirect_uris))
                .bind(fee_bps)
                .bind(serde_json::to_string(&history).unwrap_or_else(|_| "[]".into()))
                .bind(actor_id)
                .bind(now)
                .bind(actor_id)
                .bind(now)
                .execute(p)
                .await?
                .rows_affected(),
            };
            if rows != 1 {
                return Err(MarketplaceError::Db(
                    "insert marketplace client failed".into(),
                ));
            }
            if target_status == "active" {
                ensure_merchant_account(pool, &id, &owner_user_id, now).await?;
            }
            if let Some(Value::Array(scopes)) = body.get("scopes") {
                for scope_entry in scopes {
                    apply_scope_entry(pool, &id, scope_entry, actor_id, actor_name, now).await?;
                }
            }
            let created = fetch_client_by_internal_id(pool, &id)
                .await?
                .ok_or_else(|| MarketplaceError::NotFound("client".into()))?;
            Ok(created)
        }
    }
}

fn validate_webhook_url_if_present(url: &Option<String>) -> Result<(), MarketplaceError> {
    if let Some(u) = url {
        validate_webhook_url(u)?;
    }
    Ok(())
}

/// 创建/更新单个 scope 审批条目。
async fn apply_scope_entry(
    pool: &DatabasePool,
    client_id: &str,
    entry: &Value,
    actor_id: &str,
    actor_name: &str,
    now: i64,
) -> Result<(), MarketplaceError> {
    let scope = entry
        .get("scope")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if !is_valid_scope(&scope) {
        return Err(MarketplaceError::Invalid(format!("unknown scope: {scope}")));
    }
    let status = entry
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("pending")
        .to_string();
    if !matches!(status.as_str(), "pending" | "approved" | "disabled") {
        return Err(MarketplaceError::Invalid("invalid scope status".into()));
    }
    let limits = entry.get("limits").cloned().unwrap_or_else(|| json!({}));
    upsert_scope(
        pool, client_id, &scope, &status, &limits, actor_id, actor_name, now,
    )
    .await
    .map(|_| ())
}

/// 逐 scope 审批/禁用（版本化 + effective_at + 审批审计）。
#[allow(clippy::too_many_arguments)]
pub async fn upsert_scope(
    pool: &DatabasePool,
    client_id: &str,
    scope: &str,
    status: &str,
    limits: &Value,
    actor_id: &str,
    actor_name: &str,
    now: i64,
) -> Result<Value, MarketplaceError> {
    if !is_valid_scope(scope) {
        return Err(MarketplaceError::Invalid(format!("unknown scope: {scope}")));
    }
    let existing: Option<(String, i64)> = match pool {
        Either::Left(p) => {
            sqlx::query_as(
                "SELECT id, version FROM client_scopes WHERE client_id = ? AND scope = ?",
            )
            .bind(client_id)
            .bind(scope)
            .fetch_optional(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_as(
                "SELECT id, version FROM client_scopes WHERE client_id = ? AND scope = ?",
            )
            .bind(client_id)
            .bind(scope)
            .fetch_optional(p)
            .await?
        }
    };
    let limits_json = serde_json::to_string(limits).unwrap_or_else(|_| "{}".into());
    match existing {
        Some((id, version)) => {
            let new_version = version + 1;
            let rows = match pool {
                Either::Left(p) => sqlx::query(
                    "UPDATE client_scopes
                     SET status = ?, limits_json = ?, version = ?, effective_at = ?,
                         approved_by = ?, approved_at = ?, revoke_reason = NULL, updated_at = ?
                     WHERE id = ? AND version = ?",
                )
                .bind(status)
                .bind(&limits_json)
                .bind(new_version)
                .bind(now)
                .bind(actor_id)
                .bind(if status == "approved" {
                    Some(now)
                } else {
                    None
                })
                .bind(now)
                .bind(&id)
                .bind(version)
                .execute(p)
                .await?
                .rows_affected(),
                Either::Right(p) => sqlx::query(
                    "UPDATE client_scopes
                     SET status = ?, limits_json = ?, version = ?, effective_at = ?,
                         approved_by = ?, approved_at = ?, revoke_reason = NULL, updated_at = ?
                     WHERE id = ? AND version = ?",
                )
                .bind(status)
                .bind(&limits_json)
                .bind(new_version)
                .bind(now)
                .bind(actor_id)
                .bind(if status == "approved" {
                    Some(now)
                } else {
                    None
                })
                .bind(now)
                .bind(&id)
                .bind(version)
                .execute(p)
                .await?
                .rows_affected(),
            };
            if rows != 1 {
                return Err(MarketplaceError::VersionConflict {
                    expected: version,
                    current: version,
                });
            }
            let _ = AuditEntry::user_action(actor_id, "marketplace.scope.update")
                .with_target("client", client_id)
                .with_metadata(json!({ "scope": scope, "status": status, "actor": actor_name }))
                .with_policy_version(AUTHZ_POLICY_VERSION)
                .record(pool)
                .await;
            Ok(json!({ "scope": scope, "status": status, "version": new_version }))
        }
        None => {
            let id = uuid::Uuid::now_v7().to_string();
            let rows = match pool {
                Either::Left(p) => sqlx::query(
                    "INSERT INTO client_scopes
                     (id, client_id, scope, status, limits_json, version, effective_at, approved_by, approved_at, revoke_reason, created_at, updated_at)
                     VALUES (?, ?, ?, ?, ?, 1, ?, ?, ?, NULL, ?, ?)",
                )
                .bind(&id)
                .bind(client_id)
                .bind(scope)
                .bind(status)
                .bind(&limits_json)
                .bind(if status == "approved" { now } else { 0 })
                .bind(if status == "approved" { Some(actor_id) } else { None })
                .bind(if status == "approved" { Some(now) } else { None::<i64> })
                .bind(now)
                .bind(now)
                .execute(p)
                .await?
                .rows_affected(),
                Either::Right(p) => sqlx::query(
                    "INSERT INTO client_scopes
                     (id, client_id, scope, status, limits_json, version, effective_at, approved_by, approved_at, revoke_reason, created_at, updated_at)
                     VALUES (?, ?, ?, ?, ?, 1, ?, ?, ?, NULL, ?, ?)",
                )
                .bind(&id)
                .bind(client_id)
                .bind(scope)
                .bind(status)
                .bind(&limits_json)
                .bind(if status == "approved" { now } else { 0 })
                .bind(if status == "approved" { Some(actor_id) } else { None })
                .bind(if status == "approved" { Some(now) } else { None::<i64> })
                .bind(now)
                .bind(now)
                .execute(p)
                .await?
                .rows_affected(),
            };
            if rows != 1 {
                return Err(MarketplaceError::Db("insert scope failed".into()));
            }
            let _ = AuditEntry::user_action(actor_id, "marketplace.scope.approve")
                .with_target("client", client_id)
                .with_metadata(json!({ "scope": scope, "status": status, "actor": actor_name }))
                .with_policy_version(AUTHZ_POLICY_VERSION)
                .record(pool)
                .await;
            Ok(json!({ "scope": scope, "status": status, "version": 1 }))
        }
    }
}

/// 列出 Client 的 scope 审批状态。
pub async fn list_scopes(
    pool: &DatabasePool,
    client_id: &str,
) -> Result<Vec<Value>, MarketplaceError> {
    let rows: Vec<Value> = match pool {
        Either::Left(p) => {
            let r = sqlx::query(
                "SELECT scope, status, limits_json, version, effective_at, approved_at, revoke_reason
                 FROM client_scopes WHERE client_id = ? ORDER BY scope",
            )
            .bind(client_id)
            .fetch_all(p)
            .await?;
            r.iter()
                .map(|row| {
                    json!({
                        "scope": row.get::<String,_>("scope"),
                        "status": row.get::<String,_>("status"),
                        "limits": serde_json::from_str::<Value>(&row.get::<String,_>("limits_json")).unwrap_or_else(|_| json!({})),
                        "version": row.get::<i64,_>("version"),
                        "effective_at": row.get::<i64,_>("effective_at"),
                        "approved_at": row.get::<Option<i64>,_>("approved_at"),
                    })
                })
                .collect()
        }
        Either::Right(p) => {
            let r = sqlx::query(
                "SELECT scope, status, limits_json, version, effective_at, approved_at, revoke_reason
                 FROM client_scopes WHERE client_id = ? ORDER BY scope",
            )
            .bind(client_id)
            .fetch_all(p)
            .await?;
            r.iter()
                .map(|row| {
                    json!({
                        "scope": row.get::<String,_>("scope"),
                        "status": row.get::<String,_>("status"),
                        "limits": serde_json::from_str::<Value>(&row.get::<String,_>("limits_json")).unwrap_or_else(|_| json!({})),
                        "version": row.get::<i64,_>("version"),
                        "effective_at": row.get::<i64,_>("effective_at"),
                        "approved_at": row.get::<Option<i64>,_>("approved_at"),
                    })
                })
                .collect()
        }
    };
    Ok(rows)
}

/// scope 是否已批准。
pub async fn scope_approved(
    pool: &DatabasePool,
    client_id: &str,
    scope: &str,
) -> Result<bool, MarketplaceError> {
    let status: Option<String> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT status FROM client_scopes WHERE client_id = ? AND scope = ?")
                .bind(client_id)
                .bind(scope)
                .fetch_optional(p)
                .await?
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT status FROM client_scopes WHERE client_id = ? AND scope = ?")
                .bind(client_id)
                .bind(scope)
                .fetch_optional(p)
                .await?
        }
    };
    Ok(status.as_deref() == Some("approved"))
}

/// 紧急停用：立即阻止新 Intent/confirm/refund；历史可查询。
pub async fn emergency_disable(
    pool: &DatabasePool,
    key: &str,
    reason: &str,
    actor_id: &str,
    actor_name: &str,
    expected_version: i64,
    now: i64,
) -> Result<MarketplaceClient, MarketplaceError> {
    let client = match fetch_client_by_client_id(pool, key).await? {
        Some(c) => c,
        None => fetch_client_by_internal_id(pool, key)
            .await?
            .ok_or_else(|| MarketplaceError::NotFound("marketplace client not found".into()))?,
    };
    if expected_version != client.version {
        return Err(MarketplaceError::VersionConflict {
            expected: expected_version,
            current: client.version,
        });
    }
    let new_version = client.version + 1;
    let mut history: Vec<Value> =
        serde_json::from_str(&client.approval_history_json).unwrap_or_default();
    history.push(json!({
        "action": "emergency_disable",
        "from": client.status,
        "to": "emergency_disabled",
        "by": actor_name,
        "reason": reason,
        "at": now,
    }));
    let rows = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE marketplace_clients
             SET status = 'emergency_disabled', version = ?, approval_history_json = ?, updated_by = ?, updated_at = ?
             WHERE id = ? AND version = ?",
        )
        .bind(new_version)
        .bind(serde_json::to_string(&history).unwrap_or_else(|_| "[]".into()))
        .bind(actor_id)
        .bind(now)
        .bind(&client.id)
        .bind(expected_version)
        .execute(p)
        .await?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE marketplace_clients
             SET status = 'emergency_disabled', version = ?, approval_history_json = ?, updated_by = ?, updated_at = ?
             WHERE id = ? AND version = ?",
        )
        .bind(new_version)
        .bind(serde_json::to_string(&history).unwrap_or_else(|_| "[]".into()))
        .bind(actor_id)
        .bind(now)
        .bind(&client.id)
        .bind(expected_version)
        .execute(p)
        .await?
        .rows_affected(),
    };
    if rows != 1 {
        return Err(MarketplaceError::VersionConflict {
            expected: expected_version,
            current: client.version,
        });
    }
    let _ = AuditEntry::user_action(actor_id, "marketplace.emergency_disable")
        .with_target("client", &client.client_id)
        .with_reason(reason)
        .with_metadata(json!({ "actor": actor_name }))
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await;
    let updated = fetch_client_by_internal_id(pool, &client.id)
        .await?
        .ok_or_else(|| MarketplaceError::NotFound("client".into()))?;
    Ok(updated)
}

/// 轮换 Webhook Secret：旧密文作废、版本 +1；新明文只返回一次。
///
/// `master_key` 为 `marketplace_webhook_encryption_key`（AES-256-GCM 加密
/// 存储，签名时可解密；空 = fail closed）。
pub async fn rotate_webhook_secret(
    pool: &DatabasePool,
    key: &str,
    actor_id: &str,
    reason: &str,
    master_key: &str,
    now: i64,
) -> Result<(MarketplaceClient, String), MarketplaceError> {
    let client = match fetch_client_by_client_id(pool, key).await? {
        Some(c) => c,
        None => fetch_client_by_internal_id(pool, key)
            .await?
            .ok_or_else(|| MarketplaceError::NotFound("marketplace client not found".into()))?,
    };
    let secret = generate_token();
    let stored = crate::marketplace::webhooks::encrypt_webhook_secret(master_key, &secret)?;
    let new_version = client.webhook_secret_version + 1;
    let rows = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE marketplace_clients
             SET webhook_secret_hash = ?, webhook_secret_version = ?, updated_by = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(&stored)
        .bind(new_version)
        .bind(actor_id)
        .bind(now)
        .bind(&client.id)
        .execute(p)
        .await?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE marketplace_clients
             SET webhook_secret_hash = ?, webhook_secret_version = ?, updated_by = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(&stored)
        .bind(new_version)
        .bind(actor_id)
        .bind(now)
        .bind(&client.id)
        .execute(p)
        .await?
        .rows_affected(),
    };
    if rows != 1 {
        return Err(MarketplaceError::Db("rotate webhook secret failed".into()));
    }
    let _ = AuditEntry::user_action(actor_id, "marketplace.webhook_secret.rotate")
        .with_target("client", &client.client_id)
        .with_reason(reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await;
    let updated = fetch_client_by_internal_id(pool, &client.id)
        .await?
        .ok_or_else(|| MarketplaceError::NotFound("client".into()))?;
    Ok((updated, secret))
}

/// 确保商户账户存在（Client 审批/注册为 active 时创建；coin 货币）。
pub async fn ensure_merchant_account(
    pool: &DatabasePool,
    client_internal_id: &str,
    owner_user_id: &str,
    now: i64,
) -> Result<(), MarketplaceError> {
    // 商户账本用户行（满足 point_accounts FK；密码 '!' 无法登录）。
    crate::marketplace::ensure_ledger_user(
        pool,
        &crate::marketplace::merchant_ledger_user(client_internal_id),
        now,
    )
    .await?;
    let id = uuid::Uuid::now_v7().to_string();
    let rows = match pool {
        Either::Left(p) => sqlx::query(
            "INSERT OR IGNORE INTO marketplace_merchant_accounts
             (id, client_id, owner_user_id, currency_id, available_balance, pending_balance, frozen_balance, status, version, created_at, updated_at)
             VALUES (?, ?, ?, ?, 0, 0, 0, 'active', 1, ?, ?)",
        )
        .bind(&id)
        .bind(client_internal_id)
        .bind(owner_user_id)
        .bind(crate::economy::ledger::service::CURRENCY_COIN)
        .bind(now)
        .bind(now)
        .execute(p)
        .await?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "INSERT IGNORE INTO marketplace_merchant_accounts
             (id, client_id, owner_user_id, currency_id, available_balance, pending_balance, frozen_balance, status, version, created_at, updated_at)
             VALUES (?, ?, ?, ?, 0, 0, 0, 'active', 1, ?, ?)",
        )
        .bind(&id)
        .bind(client_internal_id)
        .bind(owner_user_id)
        .bind(crate::economy::ledger::service::CURRENCY_COIN)
        .bind(now)
        .bind(now)
        .execute(p)
        .await?
        .rows_affected(),
    };
    if rows != 1 {
        // 已存在：更新 owner（Owner 转让时核心服务更新）。
        match pool {
            Either::Left(p) => {
                sqlx::query(
                    "UPDATE marketplace_merchant_accounts SET owner_user_id = ?, updated_at = ?
                     WHERE client_id = ? AND currency_id = ?",
                )
                .bind(owner_user_id)
                .bind(now)
                .bind(client_internal_id)
                .bind(crate::economy::ledger::service::CURRENCY_COIN)
                .execute(p)
                .await?;
            }
            Either::Right(p) => {
                sqlx::query(
                    "UPDATE marketplace_merchant_accounts SET owner_user_id = ?, updated_at = ?
                     WHERE client_id = ? AND currency_id = ?",
                )
                .bind(owner_user_id)
                .bind(now)
                .bind(client_internal_id)
                .bind(crate::economy::ledger::service::CURRENCY_COIN)
                .execute(p)
                .await?;
            }
        }
    }
    Ok(())
}

// ─────────────────────────── 服务认证（Client Secret） ───────────────────────────

/// 服务认证结果。
#[derive(Debug, Clone)]
pub struct ServicePrincipal {
    pub client: MarketplaceClient,
}

/// 校验 Confidential Client 的 client_secret（HTTP Basic：`client_id:secret`）。
///
/// 仅允许 Confidential Client；secret 与 `oauth_clients.client_secret_hash`
/// 常量时间比对；Client 必须 active 才能执行服务操作。
pub async fn service_authenticate(
    pool: &DatabasePool,
    client_id: &str,
    secret: &str,
    required_scope: &str,
) -> Result<ServicePrincipal, MarketplaceError> {
    let oauth = crate::oidc::clients::fetch_client_by_client_id(pool, client_id)
        .await
        .map_err(|e| MarketplaceError::Db(e.to_string()))?
        .ok_or_else(|| MarketplaceError::InvalidClient("unknown client".into()))?;
    if !oauth.is_confidential() {
        return Err(MarketplaceError::InvalidClient(
            "public clients cannot use marketplace service operations".into(),
        ));
    }
    let Some(hash) = oauth.client_secret_hash.as_deref() else {
        return Err(MarketplaceError::InvalidClient(
            "client has no secret configured".into(),
        ));
    };
    if !crate::auth::token::verify_token(secret, hash) {
        return Err(MarketplaceError::InvalidClient(
            "invalid client secret".into(),
        ));
    }
    let client = fetch_client_by_client_id(pool, client_id)
        .await?
        .ok_or_else(|| {
            MarketplaceError::InvalidClient("marketplace registration not found".into())
        })?;
    if !client.is_active() {
        return Err(MarketplaceError::MarketplaceDisabled(
            "marketplace client is not active".into(),
        ));
    }
    if !scope_approved(pool, &client.id, required_scope).await? {
        return Err(MarketplaceError::MarketplaceDisabled(format!(
            "scope {required_scope} is not approved for this client"
        )));
    }
    Ok(ServicePrincipal { client })
}

/// 从 `Authorization: Basic` 头解析 client_id:secret。
pub fn parse_basic_auth(header: Option<&str>) -> Option<(String, String)> {
    let header = header?.strip_prefix("Basic ")?;
    let decoded =
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, header.trim()).ok()?;
    let text = String::from_utf8(decoded).ok()?;
    let (id, secret) = text.split_once(':')?;
    Some((id.to_string(), secret.to_string()))
}

// ─────────────────────────── JSON 字段助手 ───────────────────────────

fn str_field(body: &Value, key: &str) -> Option<String> {
    body.get(key).and_then(Value::as_str).map(str::to_string)
}

fn optional_str_field(body: &Value, key: &str) -> Option<String> {
    match body.get(key) {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Null) | None => None,
        _ => None,
    }
}

fn array_field(body: &Value, key: &str) -> Vec<String> {
    body.get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}
