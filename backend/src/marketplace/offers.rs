//! M12-CLIENTS-05 / M12-SCHEMA-03：Offer 服务端登记与版本化。
//!
//! 报价由市场通过鉴权接口创建，但由 BBLBB 保存并分配 `offer_id` 和
//! `version`。结账只接受 `offer_id + expected_version`，服务端读取可信金额；
//! 不接受请求方在结账时覆盖 `amount`、`currency_id` 或收款方。报价变价、
//! 禁用或库存策略变化后创建新版本；旧版本不能创建新 Intent。
//!
//! 金额/货币/库存/平台费全部落库（`offers` + `offer_versions` 快照）。

use serde_json::{json, Value};
use sqlx::{Either, Row};

use crate::db::DatabasePool;
use crate::marketplace::clients::{MarketplaceClient, ServicePrincipal};
use crate::marketplace::MarketplaceError;

/// Offer 行（`offers` 当前版本投影）。
#[derive(Debug, Clone)]
pub struct OfferRow {
    pub id: String,
    pub client_id: String,
    pub external_offer_id: String,
    pub title: String,
    pub description_safe: Option<String>,
    pub currency_id: String,
    pub amount: i64,
    pub quantity_min: i64,
    pub quantity_max: i64,
    pub stock_policy: String,
    pub stock_remaining: Option<i64>,
    pub status: String,
    pub fee_bps: i64,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

impl OfferRow {
    pub fn is_active(&self) -> bool {
        self.status == "active"
    }
}

const OFFER_COLUMNS: &str = "id, client_id, external_offer_id, title, description_safe, \
     currency_id, amount, quantity_min, quantity_max, stock_policy, stock_remaining, status, \
     fee_bps, version, created_at, updated_at";

/// 供 checkout 事务内按连接加载 Offer。
pub const OFFER_COLUMNS_PUB: &str = "id, client_id, external_offer_id, title, description_safe, \
     currency_id, amount, quantity_min, quantity_max, stock_policy, stock_remaining, status, \
     fee_bps, version, created_at, updated_at";

fn offer_from_sqlite(row: &sqlx::sqlite::SqliteRow) -> OfferRow {
    offer_from_row_sqlite(row)
}

pub fn offer_from_row_sqlite(row: &sqlx::sqlite::SqliteRow) -> OfferRow {
    OfferRow {
        id: row.get("id"),
        client_id: row.get("client_id"),
        external_offer_id: row.get("external_offer_id"),
        title: row.get("title"),
        description_safe: row.get("description_safe"),
        currency_id: row.get("currency_id"),
        amount: row.get("amount"),
        quantity_min: row.get("quantity_min"),
        quantity_max: row.get("quantity_max"),
        stock_policy: row.get("stock_policy"),
        stock_remaining: row.get("stock_remaining"),
        status: row.get("status"),
        fee_bps: row.get("fee_bps"),
        version: row.get("version"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn offer_from_mysql(row: &sqlx::mysql::MySqlRow) -> OfferRow {
    offer_from_row_mysql(row)
}

pub fn offer_from_row_mysql(row: &sqlx::mysql::MySqlRow) -> OfferRow {
    OfferRow {
        id: row.get("id"),
        client_id: row.get("client_id"),
        external_offer_id: row.get("external_offer_id"),
        title: row.get("title"),
        description_safe: row.get("description_safe"),
        currency_id: row.get("currency_id"),
        amount: row.get("amount"),
        quantity_min: row.get("quantity_min"),
        quantity_max: row.get("quantity_max"),
        stock_policy: row.get("stock_policy"),
        stock_remaining: row.get("stock_remaining"),
        status: row.get("status"),
        fee_bps: row.get("fee_bps"),
        version: row.get("version"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

pub fn offer_json(o: &OfferRow) -> Value {
    json!({
        "id": o.id,
        "client_id": o.client_id,
        "external_offer_id": o.external_offer_id,
        "title": o.title,
        "description_safe": o.description_safe,
        "currency_id": o.currency_id,
        "unit_amount": o.amount,
        "quantity_min": o.quantity_min,
        "quantity_max": o.quantity_max,
        "stock_policy": o.stock_policy,
        "stock_remaining": o.stock_remaining,
        "status": o.status,
        "fee_bps": o.fee_bps,
        "version": o.version,
        "created_at": o.created_at,
        "updated_at": o.updated_at,
    })
}

/// 按 id 取 Offer。
pub async fn get_offer(
    pool: &DatabasePool,
    id: &str,
) -> Result<Option<OfferRow>, MarketplaceError> {
    let sql = format!("SELECT {OFFER_COLUMNS} FROM offers WHERE id = ?");
    let row = match pool {
        Either::Left(p) => sqlx::query(&sql)
            .bind(id)
            .fetch_optional(p)
            .await?
            .map(|r| offer_from_sqlite(&r)),
        Either::Right(p) => sqlx::query(&sql)
            .bind(id)
            .fetch_optional(p)
            .await?
            .map(|r| offer_from_mysql(&r)),
    };
    Ok(row)
}

/// Client 自己的 Offer 列表（服务端投影）。
pub async fn list_offers_for_client(
    pool: &DatabasePool,
    client_id: &str,
    include_all: bool,
) -> Result<Vec<Value>, MarketplaceError> {
    let sql =
        format!("SELECT {OFFER_COLUMNS} FROM offers WHERE client_id = ? ORDER BY created_at DESC");
    let rows: Vec<Value> = match pool {
        Either::Left(p) => sqlx::query(&sql)
            .bind(client_id)
            .fetch_all(p)
            .await?
            .iter()
            .map(|r| offer_json(&offer_from_sqlite(r)))
            .collect(),
        Either::Right(p) => sqlx::query(&sql)
            .bind(client_id)
            .fetch_all(p)
            .await?
            .iter()
            .map(|r| offer_json(&offer_from_mysql(r)))
            .collect(),
    };
    Ok(rows
        .into_iter()
        .filter(|v| include_all || v["status"] == "active")
        .collect())
}

/// 校验 Offer 输入字段。
pub fn validate_offer_input(body: &Value) -> Result<OfferInput, MarketplaceError> {
    let external_offer_id = body
        .get("external_offer_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let title = body
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let description_safe = body
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_string);
    let currency_id = body
        .get("currency_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let amount = body
        .get("unit_amount")
        .and_then(Value::as_i64)
        .unwrap_or(-1);
    let quantity_min = body
        .get("quantity_min")
        .and_then(Value::as_i64)
        .unwrap_or(1);
    let quantity_max = body
        .get("quantity_max")
        .and_then(Value::as_i64)
        .unwrap_or(1);
    let stock_policy = body
        .get("stock_policy")
        .and_then(Value::as_str)
        .unwrap_or("unlimited")
        .to_string();
    let stock_remaining = body.get("stock_remaining").and_then(Value::as_i64);

    if external_offer_id.is_empty() || external_offer_id.len() > 128 {
        return Err(MarketplaceError::Invalid(
            "external_offer_id required (<=128 chars)".into(),
        ));
    }
    if title.is_empty() || title.len() > 120 {
        return Err(MarketplaceError::Invalid(
            "title required (<=120 chars)".into(),
        ));
    }
    if amount < 0 {
        return Err(MarketplaceError::Invalid("unit_amount must be >= 0".into()));
    }
    if !(quantity_min >= 1 && quantity_max >= 1 && quantity_max >= quantity_min) {
        return Err(MarketplaceError::Invalid(
            "quantity_min/quantity_max must satisfy 1 <= min <= max".into(),
        ));
    }
    if !matches!(stock_policy.as_str(), "unlimited" | "finite") {
        return Err(MarketplaceError::Invalid(
            "stock_policy must be unlimited or finite".into(),
        ));
    }
    if stock_policy == "finite" && stock_remaining.is_none() {
        return Err(MarketplaceError::Invalid(
            "stock_remaining required for finite stock".into(),
        ));
    }
    if let Some(stock) = stock_remaining {
        if stock < 0 {
            return Err(MarketplaceError::Invalid(
                "stock_remaining must be >= 0".into(),
            ));
        }
    }
    Ok(OfferInput {
        external_offer_id,
        title,
        description_safe,
        currency_id,
        amount,
        quantity_min,
        quantity_max,
        stock_policy,
        stock_remaining,
    })
}

#[derive(Debug, Clone)]
pub struct OfferInput {
    pub external_offer_id: String,
    pub title: String,
    pub description_safe: Option<String>,
    pub currency_id: String,
    pub amount: i64,
    pub quantity_min: i64,
    pub quantity_max: i64,
    pub stock_policy: String,
    pub stock_remaining: Option<i64>,
}

/// 校验货币存在（`currencies` 表）。
async fn ensure_currency(pool: &DatabasePool, currency_id: &str) -> Result<(), MarketplaceError> {
    let exists: bool = match pool {
        Either::Left(p) => {
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM currencies WHERE id = ?")
                .bind(currency_id)
                .fetch_one(p)
                .await?
                > 0
        }
        Either::Right(p) => {
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM currencies WHERE id = ?")
                .bind(currency_id)
                .fetch_one(p)
                .await?
                > 0
        }
    };
    if !exists {
        return Err(MarketplaceError::Invalid(format!(
            "unknown currency: {currency_id}"
        )));
    }
    Ok(())
}

/// POST /marketplace/offers：服务端登记报价（Client 服务认证）。
///
/// 返回 offer_id + version=1；同时写入 `offer_versions` 不可变快照。
/// 收款 Client 永远是认证的 Client（不接受请求体覆盖）。
pub async fn create_offer(
    pool: &DatabasePool,
    principal: &ServicePrincipal,
    body: &Value,
    now: i64,
) -> Result<OfferRow, MarketplaceError> {
    let input = validate_offer_input(body)?;
    ensure_currency(pool, &input.currency_id).await?;
    let client_id = principal.client.id.clone();
    let id = uuid::Uuid::now_v7().to_string();
    let rows = match pool {
        Either::Left(p) => sqlx::query(
            "INSERT INTO offers
             (id, client_id, external_offer_id, title, description_safe, currency_id, amount,
              quantity_min, quantity_max, stock_policy, stock_remaining, status, fee_bps, version,
              created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'draft', ?, 1, ?, ?)",
        )
        .bind(&id)
        .bind(&client_id)
        .bind(&input.external_offer_id)
        .bind(&input.title)
        .bind(&input.description_safe)
        .bind(&input.currency_id)
        .bind(input.amount)
        .bind(input.quantity_min)
        .bind(input.quantity_max)
        .bind(&input.stock_policy)
        .bind(input.stock_remaining)
        .bind(principal.client.fee_bps)
        .bind(now)
        .bind(now)
        .execute(p)
        .await?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "INSERT INTO offers
             (id, client_id, external_offer_id, title, description_safe, currency_id, amount,
              quantity_min, quantity_max, stock_policy, stock_remaining, status, fee_bps, version,
              created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'draft', ?, 1, ?, ?)",
        )
        .bind(&id)
        .bind(&client_id)
        .bind(&input.external_offer_id)
        .bind(&input.title)
        .bind(&input.description_safe)
        .bind(&input.currency_id)
        .bind(input.amount)
        .bind(input.quantity_min)
        .bind(input.quantity_max)
        .bind(&input.stock_policy)
        .bind(input.stock_remaining)
        .bind(principal.client.fee_bps)
        .bind(now)
        .bind(now)
        .execute(p)
        .await?
        .rows_affected(),
    };
    if rows != 1 {
        return Err(MarketplaceError::Db("insert offer failed".into()));
    }
    snapshot_version(pool, &id, 1, &input, &principal.client, "draft", now).await?;
    get_offer(pool, &id)
        .await?
        .ok_or_else(|| MarketplaceError::NotFound("offer".into()))
}

/// 写入 offer_versions 快照。
async fn snapshot_version(
    pool: &DatabasePool,
    offer_id: &str,
    version: i64,
    input: &OfferInput,
    client: &MarketplaceClient,
    status: &str,
    now: i64,
) -> Result<(), MarketplaceError> {
    let id = uuid::Uuid::now_v7().to_string();
    let rows = match pool {
        Either::Left(p) => sqlx::query(
            "INSERT INTO offer_versions
             (id, offer_id, version, title, description_safe, currency_id, amount, quantity_min,
              quantity_max, stock_policy, stock_remaining, status, fee_bps, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(offer_id)
        .bind(version)
        .bind(&input.title)
        .bind(&input.description_safe)
        .bind(&input.currency_id)
        .bind(input.amount)
        .bind(input.quantity_min)
        .bind(input.quantity_max)
        .bind(&input.stock_policy)
        .bind(input.stock_remaining)
        .bind(status)
        .bind(client.fee_bps)
        .bind(now)
        .execute(p)
        .await?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "INSERT INTO offer_versions
             (id, offer_id, version, title, description_safe, currency_id, amount, quantity_min,
              quantity_max, stock_policy, stock_remaining, status, fee_bps, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(offer_id)
        .bind(version)
        .bind(&input.title)
        .bind(&input.description_safe)
        .bind(&input.currency_id)
        .bind(input.amount)
        .bind(input.quantity_min)
        .bind(input.quantity_max)
        .bind(&input.stock_policy)
        .bind(input.stock_remaining)
        .bind(status)
        .bind(client.fee_bps)
        .bind(now)
        .execute(p)
        .await?
        .rows_affected(),
    };
    if rows != 1 {
        return Err(MarketplaceError::Db("insert offer version failed".into()));
    }
    Ok(())
}

/// PATCH /marketplace/offers/{id}：更新报价（If-Match）。
///
/// 任何字段变化都创建新版本快照；`expected_version` 与当前版本不一致 → 409
/// `version_conflict`。`status` 可选：draft/active/paused/disabled。
pub async fn update_offer(
    pool: &DatabasePool,
    principal: &ServicePrincipal,
    offer_id: &str,
    expected_version: i64,
    body: &Value,
    now: i64,
) -> Result<OfferRow, MarketplaceError> {
    let current = get_offer(pool, offer_id)
        .await?
        .ok_or_else(|| MarketplaceError::NotFound("offer not found".into()))?;
    if current.client_id != principal.client.id {
        return Err(MarketplaceError::Forbidden(
            "offer belongs to another client".into(),
        ));
    }
    if expected_version != current.version {
        return Err(MarketplaceError::VersionConflict {
            expected: expected_version,
            current: current.version,
        });
    }
    let input = validate_offer_input(body)?;
    ensure_currency(pool, &input.currency_id).await?;
    let status = body
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or(&current.status)
        .to_string();
    if !matches!(status.as_str(), "draft" | "active" | "paused" | "disabled") {
        return Err(MarketplaceError::Invalid("invalid offer status".into()));
    }
    let new_version = current.version + 1;
    let rows = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE offers
             SET external_offer_id = ?, title = ?, description_safe = ?, currency_id = ?, amount = ?,
                 quantity_min = ?, quantity_max = ?, stock_policy = ?, stock_remaining = ?, status = ?,
                 version = ?, updated_at = ?
             WHERE id = ? AND version = ?",
        )
        .bind(&input.external_offer_id)
        .bind(&input.title)
        .bind(&input.description_safe)
        .bind(&input.currency_id)
        .bind(input.amount)
        .bind(input.quantity_min)
        .bind(input.quantity_max)
        .bind(&input.stock_policy)
        .bind(input.stock_remaining)
        .bind(&status)
        .bind(new_version)
        .bind(now)
        .bind(offer_id)
        .bind(expected_version)
        .execute(p)
        .await?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE offers
             SET external_offer_id = ?, title = ?, description_safe = ?, currency_id = ?, amount = ?,
                 quantity_min = ?, quantity_max = ?, stock_policy = ?, stock_remaining = ?, status = ?,
                 version = ?, updated_at = ?
             WHERE id = ? AND version = ?",
        )
        .bind(&input.external_offer_id)
        .bind(&input.title)
        .bind(&input.description_safe)
        .bind(&input.currency_id)
        .bind(input.amount)
        .bind(input.quantity_min)
        .bind(input.quantity_max)
        .bind(&input.stock_policy)
        .bind(input.stock_remaining)
        .bind(&status)
        .bind(new_version)
        .bind(now)
        .bind(offer_id)
        .bind(expected_version)
        .execute(p)
        .await?
        .rows_affected(),
    };
    if rows != 1 {
        return Err(MarketplaceError::VersionConflict {
            expected: expected_version,
            current: current.version,
        });
    }
    snapshot_version(
        pool,
        offer_id,
        new_version,
        &input,
        &principal.client,
        &status,
        now,
    )
    .await?;
    get_offer(pool, offer_id)
        .await?
        .ok_or_else(|| MarketplaceError::NotFound("offer".into()))
}

/// 状态切换（Client 或管理员）：仅 owner client 可操作。
pub async fn set_offer_status(
    pool: &DatabasePool,
    principal: &ServicePrincipal,
    offer_id: &str,
    status: &str,
    expected_version: i64,
    now: i64,
) -> Result<OfferRow, MarketplaceError> {
    let current = get_offer(pool, offer_id)
        .await?
        .ok_or_else(|| MarketplaceError::NotFound("offer not found".into()))?;
    if current.client_id != principal.client.id {
        return Err(MarketplaceError::Forbidden(
            "offer belongs to another client".into(),
        ));
    }
    if !matches!(status, "active" | "paused" | "disabled") {
        return Err(MarketplaceError::Invalid("invalid offer status".into()));
    }
    let body = json!({
        "external_offer_id": current.external_offer_id,
        "title": current.title,
        "description": current.description_safe,
        "currency_id": current.currency_id,
        "unit_amount": current.amount,
        "quantity_min": current.quantity_min,
        "quantity_max": current.quantity_max,
        "stock_policy": current.stock_policy,
        "stock_remaining": current.stock_remaining,
        "status": status,
    });
    update_offer(pool, principal, offer_id, expected_version, &body, now).await
}
