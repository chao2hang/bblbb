//! M07-SHOP 服务层：商品/订单/权益/装备/presentation 与 admin 操作。
//!
//! 设计要点：
//! - 服务端重算价格、库存、等级门槛、销售窗口与限购，不信任请求体。
//! - 购买同一事务：锁库存（条件更新 rows==1）→ 账本扣款（`apply_operation_in_*_tx`）
//!   → 写订单 → 发权益 → 审计 → Outbox；任何失败整体回滚。
//! - 幂等：(user_id, idempotency_key) 唯一 + request_hash 冲突检测。
//! - SQLite 用 `BEGIN IMMEDIATE` 整体写锁；MySQL/MariaDB 固定锁顺序
//!   （product 行锁 → 账本账户行锁）。
//! - Token 白名单：拒绝任意 CSS/HTML/JS/URL/SVG（M07-SHOP-SCHEMA-03）。
//! - 数字装扮默认不可退款；异常补偿走 `LedgerKind::Reversal` 反向流水。

use serde_json::{json, Value};
use sqlx::{Either, Row};

use crate::audit::AuditEntry;
use crate::authz::decision::AUTHZ_POLICY_VERSION;
use crate::db::DatabasePool;
use crate::economy::ledger::service as ledger;
use crate::economy::ledger::service::{LedgerCommand, LedgerError, LedgerKind};
use crate::error::AppError;
use crate::events::types::{SHOP_ENTITLEMENT_CHANGED, SHOP_ORDER_SUCCEEDED};
use crate::outbox::now_millis;

/// 商城错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShopError {
    Db(String),
    NotFound(String),
    Invalid(String),
    /// 余额不足（账本返回）。
    InsufficientBalance,
    /// 库存不足（并发不超卖）。
    OutOfStock,
    /// 等级门槛未达。
    BelowLevel {
        required: i64,
    },
    /// 不在销售窗口内。
    NotInSaleWindow,
    /// 超过限购数量。
    PurchaseLimitExceeded,
    /// 同幂等键不同请求摘要。
    IdempotencyConflict,
    /// 权益不属于本人或不可装备。
    EntitlementNotOwned,
    /// 装备槽冲突（slot 互斥 / 徽章超过 3 个）。
    SlotConflict,
    /// 不可退款（non_refundable）。
    NotRefundable,
    /// 权限/状态不允许。
    Forbidden(String),
}

impl From<sqlx::Error> for ShopError {
    fn from(e: sqlx::Error) -> Self {
        Self::Db(e.to_string())
    }
}

impl From<LedgerError> for ShopError {
    fn from(e: LedgerError) -> Self {
        match e {
            LedgerError::InsufficientBalance => Self::InsufficientBalance,
            LedgerError::IdempotencyConflict => Self::IdempotencyConflict,
            LedgerError::ConcurrentModification => Self::Db("concurrent modification".into()),
            other => Self::Db(other.to_string()),
        }
    }
}

impl std::fmt::Display for ShopError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Db(msg) => write!(f, "shop db error: {msg}"),
            Self::NotFound(msg) => write!(f, "shop not found: {msg}"),
            Self::Invalid(msg) => write!(f, "invalid shop request: {msg}"),
            Self::InsufficientBalance => write!(f, "insufficient balance"),
            Self::OutOfStock => write!(f, "out of stock"),
            Self::BelowLevel { required } => write!(f, "level {required} required"),
            Self::NotInSaleWindow => write!(f, "not in sale window"),
            Self::PurchaseLimitExceeded => write!(f, "purchase limit exceeded"),
            Self::IdempotencyConflict => write!(f, "idempotency key reused"),
            Self::EntitlementNotOwned => write!(f, "entitlement not owned or invalid"),
            Self::SlotConflict => write!(f, "equipment slot conflict"),
            Self::NotRefundable => write!(f, "order is not refundable"),
            Self::Forbidden(msg) => write!(f, "shop forbidden: {msg}"),
        }
    }
}

impl std::error::Error for ShopError {}

/// 商品行（shop_products）。
#[derive(Debug, Clone)]
pub struct ProductRow {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub slug: String,
    pub title: String,
    pub description_safe: Option<String>,
    pub icon_token: Option<String>,
    pub presentation_tokens_json: Option<String>,
    pub slot: String,
    pub currency_id: String,
    pub unit_price: i64,
    pub quantity_limit: i64,
    pub stock_remaining: Option<i64>,
    pub required_level: i64,
    pub validity_seconds: Option<i64>,
    pub sale_start_at: Option<i64>,
    pub sale_end_at: Option<i64>,
    pub refund_policy: String,
    pub version: i64,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
}

fn product_row_from(row: &sqlx::sqlite::SqliteRow) -> ProductRow {
    ProductRow {
        id: row.get("id"),
        kind: row.get("kind"),
        status: row.get("status"),
        slug: row.get("slug"),
        title: row.get("title"),
        description_safe: row.get("description_safe"),
        icon_token: row.get("icon_token"),
        presentation_tokens_json: row.get("presentation_tokens_json"),
        slot: row.get("slot"),
        currency_id: row.get("currency_id"),
        unit_price: row.get("unit_price"),
        quantity_limit: row.get("quantity_limit"),
        stock_remaining: row.get("stock_remaining"),
        required_level: row.get("required_level"),
        validity_seconds: row.get("validity_seconds"),
        sale_start_at: row.get("sale_start_at"),
        sale_end_at: row.get("sale_end_at"),
        refund_policy: row.get("refund_policy"),
        version: row.get("version"),
        created_by: row.get("created_by"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn product_row_from_mysql(row: &sqlx::mysql::MySqlRow) -> ProductRow {
    ProductRow {
        id: row.get("id"),
        kind: row.get("kind"),
        status: row.get("status"),
        slug: row.get("slug"),
        title: row.get("title"),
        description_safe: row.get("description_safe"),
        icon_token: row.get("icon_token"),
        presentation_tokens_json: row.get("presentation_tokens_json"),
        slot: row.get("slot"),
        currency_id: row.get("currency_id"),
        unit_price: row.get("unit_price"),
        quantity_limit: row.get("quantity_limit"),
        stock_remaining: row.get("stock_remaining"),
        required_level: row.get("required_level"),
        validity_seconds: row.get("validity_seconds"),
        sale_start_at: row.get("sale_start_at"),
        sale_end_at: row.get("sale_end_at"),
        refund_policy: row.get("refund_policy"),
        version: row.get("version"),
        created_by: row.get("created_by"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

const PRODUCT_COLUMNS: &str = "id, kind, status, slug, title, description_safe, icon_token, \
     presentation_tokens_json, slot, currency_id, unit_price, quantity_limit, stock_remaining, \
     required_level, validity_seconds, sale_start_at, sale_end_at, refund_policy, version, \
     created_by, created_at, updated_at";

fn product_json(p: &ProductRow) -> Value {
    json!({
        "id": p.id,
        "kind": p.kind,
        "status": p.status,
        "slug": p.slug,
        "title": p.title,
        "description_safe": p.description_safe,
        "icon_token": p.icon_token,
        "presentation_tokens": p.presentation_tokens_json.as_deref().and_then(|s| serde_json::from_str::<Vec<String>>(s).ok()),
        "slot": p.slot,
        "currency_id": p.currency_id,
        "unit_price": p.unit_price,
        "quantity_limit": p.quantity_limit,
        "stock_remaining": p.stock_remaining,
        "required_level": p.required_level,
        "validity_seconds": p.validity_seconds,
        "sale_start_at": p.sale_start_at,
        "sale_end_at": p.sale_end_at,
        "refund_policy": p.refund_policy,
        "version": p.version,
        "created_at": p.created_at,
        "updated_at": p.updated_at,
    })
}

/// 商品是否处于可售窗口且满足等级门槛（供列表过滤与购买校验）。
fn purchasable(p: &ProductRow, user_level: i64, now: i64) -> Result<(), ShopError> {
    if p.status != "published" {
        return Err(ShopError::Forbidden("product not published".into()));
    }
    if user_level < p.required_level {
        return Err(ShopError::BelowLevel {
            required: p.required_level,
        });
    }
    if let Some(start) = p.sale_start_at {
        if now < start {
            return Err(ShopError::NotInSaleWindow);
        }
    }
    if let Some(end) = p.sale_end_at {
        if now > end {
            return Err(ShopError::NotInSaleWindow);
        }
    }
    Ok(())
}

/// 读商品（SQLite）。
async fn load_product(
    conn: &mut sqlx::SqliteConnection,
    id: &str,
) -> Result<ProductRow, ShopError> {
    let row = sqlx::query(&format!(
        "SELECT {PRODUCT_COLUMNS} FROM shop_products WHERE id = ?"
    ))
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| ShopError::NotFound(format!("product {id}")))?;
    Ok(product_row_from(&row))
}

/// 读商品（MySQL）。
async fn load_product_mysql(
    conn: &mut sqlx::MySqlConnection,
    id: &str,
) -> Result<ProductRow, ShopError> {
    let row = sqlx::query(&format!(
        "SELECT {PRODUCT_COLUMNS} FROM shop_products WHERE id = ?"
    ))
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| ShopError::NotFound(format!("product {id}")))?;
    Ok(product_row_from_mysql(&row))
}

/// 当前用户等级（users.level 缓存，可重建）。
async fn user_level(conn: &mut sqlx::SqliteConnection, user_id: &str) -> Result<i64, ShopError> {
    let level: i64 = sqlx::query_scalar("SELECT level FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| ShopError::NotFound(format!("user {user_id}")))?;
    Ok(level)
}

async fn user_level_mysql(
    conn: &mut sqlx::MySqlConnection,
    user_id: &str,
) -> Result<i64, ShopError> {
    let level: i64 = sqlx::query_scalar("SELECT level FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| ShopError::NotFound(format!("user {user_id}")))?;
    Ok(level)
}

/// 校验展示 Token 白名单（M07-SHOP-SCHEMA-03/06）。
/// 只允许注册前缀的小写安全 token；拒绝任意 CSS/HTML/JS/URL/SVG。
pub fn validate_tokens(
    icon_token: Option<&str>,
    presentation_tokens_json: Option<&str>,
) -> Result<(), ShopError> {
    if let Some(t) = icon_token {
        if !is_safe_token(t) {
            return Err(ShopError::Invalid(format!(
                "unsafe presentation token: {t}"
            )));
        }
    }
    if let Some(json_str) = presentation_tokens_json {
        let tokens: Vec<String> = serde_json::from_str(json_str).map_err(|_| {
            ShopError::Invalid("presentation_tokens_json must be a string array".into())
        })?;
        if tokens.len() > 20 {
            return Err(ShopError::Invalid("too many presentation tokens".into()));
        }
        for t in &tokens {
            if !is_safe_token(t) {
                return Err(ShopError::Invalid(format!(
                    "unsafe presentation token: {t}"
                )));
            }
        }
    }
    Ok(())
}

/// 注册的安全 Token 前缀（白名单枚举）。
const SAFE_TOKEN_PREFIXES: &[&str] = &[
    "nickname.decoration.",
    "nickname.color.",
    "avatar.frame.",
    "profile.effect.",
    "post.effect.",
    "badge.",
    "title.prefix.",
    "reaction.pack.",
];

fn is_safe_token(t: &str) -> bool {
    if t.is_empty() || t.len() > 64 {
        return false;
    }
    if !t
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '_')
    {
        return false;
    }
    if t.contains("..") || t.contains('/') || t.contains(':') || t.contains('\\') {
        return false;
    }
    SAFE_TOKEN_PREFIXES.iter().any(|p| t.starts_with(p))
}

/// 公开商品列表（只返回 published；admin 传 include_all 返回全部）。
pub async fn list_products(pool: &DatabasePool, include_all: bool) -> Result<Value, ShopError> {
    match pool {
        Either::Left(p) => {
            let rows = sqlx::query(&format!(
                "SELECT {PRODUCT_COLUMNS} FROM shop_products \
                 ORDER BY created_at DESC"
            ))
            .fetch_all(p)
            .await?;
            let items: Vec<Value> = rows
                .iter()
                .map(product_row_from)
                .filter(|pr| include_all || pr.status == "published")
                .map(|pr| product_json(&pr))
                .collect();
            Ok(json!({ "products": items }))
        }
        Either::Right(p) => {
            let rows = sqlx::query(&format!(
                "SELECT {PRODUCT_COLUMNS} FROM shop_products \
                 ORDER BY created_at DESC"
            ))
            .fetch_all(p)
            .await?;
            let items: Vec<Value> = rows
                .iter()
                .map(product_row_from_mysql)
                .filter(|pr| include_all || pr.status == "published")
                .map(|pr| product_json(&pr))
                .collect();
            Ok(json!({ "products": items }))
        }
    }
}

/// 单个商品。
pub async fn get_product(pool: &DatabasePool, id: &str) -> Result<Value, ShopError> {
    match pool {
        Either::Left(p) => {
            let mut conn = p.acquire().await?;
            let row = load_product(&mut conn, id).await?;
            Ok(product_json(&row))
        }
        Either::Right(p) => {
            let mut conn = p.acquire().await?;
            let row = load_product_mysql(&mut conn, id).await?;
            Ok(product_json(&row))
        }
    }
}

/// 订单行。
#[derive(Debug, Clone)]
#[allow(dead_code)] // 字段完整映射 shop_orders 列；JSON 输出按需裁剪
struct OrderRow {
    id: String,
    user_id: String,
    product_id: String,
    product_version: i64,
    quantity: i64,
    currency_id: String,
    unit_price: i64,
    total_amount: i64,
    point_operation_id: String,
    status: String,
    idempotency_key: String,
    created_at: i64,
}

fn order_json(o: &OrderRow) -> Value {
    json!({
        "id": o.id,
        "product_id": o.product_id,
        "product_version": o.product_version,
        "quantity": o.quantity,
        "currency_id": o.currency_id,
        "unit_price": o.unit_price,
        "total_amount": o.total_amount,
        "status": o.status,
        "created_at": o.created_at,
    })
}

/// 购买商品（核心事务；M07-SHOP-01..04）。
///
/// 服务端重算全部定价/库存/门槛；同事务完成锁库存+账本扣款+订单+权益+
/// 审计+Outbox；幂等键 (user_id, idempotency_key) 重放原订单。
#[allow(clippy::explicit_auto_deref)]
pub async fn buy_product(
    pool: &DatabasePool,
    user_id: &str,
    product_id: &str,
    quantity: i64,
    idempotency_key: &str,
) -> Result<Value, ShopError> {
    if quantity <= 0 || quantity > 100 {
        return Err(ShopError::Invalid("quantity out of range".into()));
    }
    if idempotency_key.is_empty() || idempotency_key.len() > 64 {
        return Err(ShopError::Invalid("invalid idempotency key".into()));
    }
    let now = now_millis();
    let request_hash = hash_request(user_id, product_id, quantity, idempotency_key);

    match pool {
        Either::Left(p) => {
            let mut conn = p.acquire().await?;
            sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
            let outcome: Result<Value, ShopError> = async {
                let product = load_product(&mut *conn, product_id).await?;
                let level = user_level(&mut *conn, user_id).await?;
                purchasable(&product, level, now)?;
                // 幂等预检：同 (user_id, idempotency_key) 已有订单 → 重放原订单
                // （在扣款/扣库存之前，避免重复计费）。
                let existing: Option<sqlx::sqlite::SqliteRow> = sqlx::query(
                    "SELECT id, request_hash FROM shop_orders WHERE user_id = ? AND idempotency_key = ?",
                )
                .bind(user_id)
                .bind(idempotency_key)
                .fetch_optional(&mut *conn)
                .await?;
                if let Some(existing) = existing {
                    let stored_hash: String = existing.get("request_hash");
                    if stored_hash != request_hash {
                        return Err(ShopError::IdempotencyConflict);
                    }
                    let row = sqlx::query(
                        "SELECT id, user_id, product_id, product_version, quantity, currency_id, unit_price, total_amount, point_operation_id, status, idempotency_key, request_hash, created_at
                         FROM shop_orders WHERE id = ?",
                    )
                    .bind(existing.get::<String, _>("id"))
                    .fetch_one(&mut *conn)
                    .await?;
                    let mut v = order_json(&row_to_order(&row));
                    v["order_id"] = v["id"].clone();
                    return Ok(v);
                }
                if let Some(stock) = product.stock_remaining {
                    if stock < quantity {
                        return Err(ShopError::OutOfStock);
                    }
                }
                // 限购：已购数量（含本次）不得超过 quantity_limit。
                if product.quantity_limit > 0 {
                    let bought: i64 = sqlx::query_scalar(
                        "SELECT COALESCE(SUM(quantity),0) FROM shop_orders \
                         WHERE user_id = ? AND product_id = ? AND status IN ('succeeded','partially_refunded')",
                    )
                    .bind(user_id)
                    .bind(product_id)
                    .fetch_one(&mut *conn)
                    .await?;
                    if bought + quantity > product.quantity_limit {
                        return Err(ShopError::PurchaseLimitExceeded);
                    }
                }
                // 锁库存（条件更新，rows==0 → 并发已售罄）。
                let affected = match product.stock_remaining {
                    Some(_) => sqlx::query(
                        "UPDATE shop_products SET stock_remaining = stock_remaining - ?, updated_at = ? \
                         WHERE id = ? AND stock_remaining >= ?",
                    )
                    .bind(quantity)
                    .bind(now)
                    .bind(product_id)
                    .bind(quantity)
                    .execute(&mut *conn)
                    .await?
                    .rows_affected(),
                    None => 1,
                };
                if affected != 1 {
                    return Err(ShopError::OutOfStock);
                }
                let total = product
                    .unit_price
                    .checked_mul(quantity)
                    .ok_or_else(|| ShopError::Invalid("amount overflow".into()))?;
                let cmd = LedgerCommand {
                    idempotency_scope: "shop".to_string(),
                    idempotency_key: uuid::Uuid::now_v7().to_string(),
                    kind: LedgerKind::ShopPurchase,
                    actor_id: Some(user_id.to_string()),
                    user_id: user_id.to_string(),
                    currency_id: product.currency_id.clone(),
                    delta_balance: -total,
                    delta_frozen: 0,
                    source_type: Some("product".to_string()),
                    source_id: Some(product.id.clone()),
                    memo: format!("shop purchase {} x{}", product.title, quantity),
                    reverses_operation_id: None,
                };
                let op = ledger::apply_operation_in_sqlite_tx(&mut *conn, cmd, now).await?;

                let order_id = uuid::Uuid::now_v7().to_string();
                let insert_result = sqlx::query(
                    "INSERT INTO shop_orders
                         (id, user_id, product_id, product_version, quantity, currency_id, unit_price, total_amount, point_operation_id, status, idempotency_key, request_hash, created_at, updated_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'succeeded', ?, ?, ?, ?)",
                )
                .bind(&order_id)
                .bind(user_id)
                .bind(&product.id)
                .bind(product.version)
                .bind(quantity)
                .bind(&product.currency_id)
                .bind(product.unit_price)
                .bind(total)
                .bind(&op.operation_id)
                .bind(idempotency_key)
                .bind(&request_hash)
                .bind(now)
                .bind(now)
                .execute(&mut *conn)
                .await;
                if let Err(insert_err) = insert_result {
                    // 同幂等键重放：查询原订单并校验摘要。
                    if is_duplicate_key_sqlite(&insert_err) {
                        let row: Option<sqlx::sqlite::SqliteRow> = sqlx::query(
                            "SELECT id, user_id, product_id, product_version, quantity, currency_id, unit_price, total_amount, point_operation_id, status, idempotency_key, request_hash, created_at
                             FROM shop_orders WHERE user_id = ? AND idempotency_key = ?",
                        )
                        .bind(user_id)
                        .bind(idempotency_key)
                        .fetch_optional(&mut *conn)
                        .await?;
                        if let Some(row) = row {
                            let stored_hash: String = row.get("request_hash");
                            if stored_hash != request_hash {
                                return Err(ShopError::IdempotencyConflict);
                            }
                            let mut v = order_json(&row_to_order(&row));
                            // 与首次成功响应同构：额外提供 order_id 别名。
                            v["order_id"] = v["id"].clone();
                            return Ok(v);
                        }
                        return Err(ShopError::IdempotencyConflict);
                    }
                    return Err(ShopError::from(insert_err));
                }

                // 发权益（reaction_pack 按 quantity 合并为一条 remaining_quantity）。
                let entitlement_id = uuid::Uuid::now_v7().to_string();
                let (valid_from, expires_at) = match product.validity_seconds {
                    Some(secs) => (now, Some(now + secs * 1000)),
                    None => (now, None),
                };
                sqlx::query(
                    "INSERT INTO user_entitlements
                         (id, user_id, product_id, order_id, status, quantity, remaining_quantity, valid_from, expires_at, created_at, updated_at)
                     VALUES (?, ?, ?, ?, 'owned', ?, ?, ?, ?, ?, ?)",
                )
                .bind(&entitlement_id)
                .bind(user_id)
                .bind(&product.id)
                .bind(&order_id)
                .bind(quantity)
                .bind(quantity)
                .bind(valid_from)
                .bind(expires_at)
                .bind(now)
                .bind(now)
                .execute(&mut *conn)
                .await?;

                AuditEntry::user_action(user_id, "shop.purchase")
                    .with_target("product", &product.id)
                    .with_target("order", &order_id)
                    .with_reason("shop purchase")
                    .with_policy_version(AUTHZ_POLICY_VERSION)
                    .record_into_sqlite(&mut *conn)
                    .await
                    .map_err(ShopError::from)?;
                enqueue_in_tx_flat_sqlite(
                    &mut *conn,
                    SHOP_ORDER_SUCCEEDED,
                    json!({"order_id": order_id, "user_id": user_id, "product_id": product.id, "quantity": quantity, "total_amount": total}),
                )
                .await?;
                enqueue_in_tx_flat_sqlite(
                    &mut *conn,
                    SHOP_ENTITLEMENT_CHANGED,
                    json!({"entitlement_id": entitlement_id, "user_id": user_id, "status": "owned"}),
                )
                .await?;

                Ok(json!({
                    "order_id": order_id,
                    "product_id": product.id,
                    "product_version": product.version,
                    "quantity": quantity,
                    "unit_price": product.unit_price,
                    "total_amount": total,
                    "status": "succeeded",
                    "entitlement_id": entitlement_id,
                    "balance_after": op.transactions[0].balance_after,
                    "created_at": now,
                }))
            }
            .await;
            match outcome {
                Ok(v) => {
                    sqlx::query("COMMIT").execute(&mut *conn).await?;
                    Ok(v)
                }
                Err(e) => {
                    let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                    Err(e)
                }
            }
        }
        Either::Right(p) => {
            let mut tx = p.begin().await?;
            let outcome: Result<Value, ShopError> = async {
                let product = load_product_mysql(&mut tx, product_id).await?;
                let level = user_level_mysql(&mut tx, user_id).await?;
                purchasable(&product, level, now)?;
                // 幂等预检：同 (user_id, idempotency_key) 已有订单 → 重放原订单
                // （在扣款/扣库存之前，避免重复计费）。
                let existing: Option<(String, String)> = sqlx::query_as(
                    "SELECT id, request_hash FROM shop_orders WHERE user_id = ? AND idempotency_key = ?",
                )
                .bind(user_id)
                .bind(idempotency_key)
                .fetch_optional(&mut *tx)
                .await?;
                if let Some((existing_id, stored_hash)) = existing {
                    if stored_hash != request_hash {
                        return Err(ShopError::IdempotencyConflict);
                    }
                    let row = sqlx::query(
                        "SELECT id, user_id, product_id, product_version, quantity, currency_id, unit_price, total_amount, point_operation_id, status, idempotency_key, request_hash, created_at
                         FROM shop_orders WHERE id = ?",
                    )
                    .bind(&existing_id)
                    .fetch_one(&mut *tx)
                    .await?;
                    let mut v = order_json(&row_to_order_mysql(&row));
                    v["order_id"] = v["id"].clone();
                    return Ok(v);
                }
                if let Some(stock) = product.stock_remaining {
                    if stock < quantity {
                        return Err(ShopError::OutOfStock);
                    }
                }
                if product.quantity_limit > 0 {
                    let bought: i64 = sqlx::query_scalar(
                        "SELECT COALESCE(SUM(quantity),0) FROM shop_orders \
                         WHERE user_id = ? AND product_id = ? AND status IN ('succeeded','partially_refunded')",
                    )
                    .bind(user_id)
                    .bind(product_id)
                    .fetch_one(&mut *tx)
                    .await?;
                    if bought + quantity > product.quantity_limit {
                        return Err(ShopError::PurchaseLimitExceeded);
                    }
                }
                let affected = match product.stock_remaining {
                    Some(_) => sqlx::query(
                        "UPDATE shop_products SET stock_remaining = stock_remaining - ?, updated_at = ? \
                         WHERE id = ? AND stock_remaining >= ?",
                    )
                    .bind(quantity)
                    .bind(now)
                    .bind(product_id)
                    .bind(quantity)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected(),
                    None => 1,
                };
                if affected != 1 {
                    return Err(ShopError::OutOfStock);
                }
                let total = product
                    .unit_price
                    .checked_mul(quantity)
                    .ok_or_else(|| ShopError::Invalid("amount overflow".into()))?;
                let cmd = LedgerCommand {
                    idempotency_scope: "shop".to_string(),
                    idempotency_key: uuid::Uuid::now_v7().to_string(),
                    kind: LedgerKind::ShopPurchase,
                    actor_id: Some(user_id.to_string()),
                    user_id: user_id.to_string(),
                    currency_id: product.currency_id.clone(),
                    delta_balance: -total,
                    delta_frozen: 0,
                    source_type: Some("product".to_string()),
                    source_id: Some(product.id.clone()),
                    memo: format!("shop purchase {} x{}", product.title, quantity),
                    reverses_operation_id: None,
                };
                let op = ledger::apply_operation_in_mysql_tx(&mut tx, cmd, now).await?;

                let order_id = uuid::Uuid::now_v7().to_string();
                let insert_result = sqlx::query(
                    "INSERT INTO shop_orders
                         (id, user_id, product_id, product_version, quantity, currency_id, unit_price, total_amount, point_operation_id, status, idempotency_key, request_hash, created_at, updated_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'succeeded', ?, ?, ?, ?)",
                )
                .bind(&order_id)
                .bind(user_id)
                .bind(&product.id)
                .bind(product.version)
                .bind(quantity)
                .bind(&product.currency_id)
                .bind(product.unit_price)
                .bind(total)
                .bind(&op.operation_id)
                .bind(idempotency_key)
                .bind(&request_hash)
                .bind(now)
                .bind(now)
                .execute(&mut *tx)
                .await;
                if let Err(insert_err) = insert_result {
                    if is_duplicate_key(&insert_err) {
                        let row = sqlx::query(
                            "SELECT id, user_id, product_id, product_version, quantity, currency_id, unit_price, total_amount, point_operation_id, status, idempotency_key, request_hash, created_at
                             FROM shop_orders WHERE user_id = ? AND idempotency_key = ?",
                        )
                        .bind(user_id)
                        .bind(idempotency_key)
                        .fetch_optional(&mut *tx)
                        .await?;
                        if let Some(row) = row {
                            let stored_hash: String = row.get("request_hash");
                            if stored_hash != request_hash {
                                return Err(ShopError::IdempotencyConflict);
                            }
                            let mut v = order_json(&row_to_order_mysql(&row));
                            v["order_id"] = v["id"].clone();
                            return Ok(v);
                        }
                        return Err(ShopError::IdempotencyConflict);
                    }
                    return Err(ShopError::from(insert_err));
                }

                let entitlement_id = uuid::Uuid::now_v7().to_string();
                let (valid_from, expires_at) = match product.validity_seconds {
                    Some(secs) => (now, Some(now + secs * 1000)),
                    None => (now, None),
                };
                sqlx::query(
                    "INSERT INTO user_entitlements
                         (id, user_id, product_id, order_id, status, quantity, remaining_quantity, valid_from, expires_at, created_at, updated_at)
                     VALUES (?, ?, ?, ?, 'owned', ?, ?, ?, ?, ?, ?)",
                )
                .bind(&entitlement_id)
                .bind(user_id)
                .bind(&product.id)
                .bind(&order_id)
                .bind(quantity)
                .bind(quantity)
                .bind(valid_from)
                .bind(expires_at)
                .bind(now)
                .bind(now)
                .execute(&mut *tx)
                .await?;

                AuditEntry::user_action(user_id, "shop.purchase")
                    .with_target("product", &product.id)
                    .with_target("order", &order_id)
                    .with_reason("shop purchase")
                    .with_policy_version(AUTHZ_POLICY_VERSION)
                    .record_into_mysql(&mut tx)
                    .await
                    .map_err(ShopError::from)?;
                enqueue_in_tx_flat_mysql(
                    &mut tx,
                    SHOP_ORDER_SUCCEEDED,
                    json!({"order_id": order_id, "user_id": user_id, "product_id": product.id, "quantity": quantity, "total_amount": total}),
                )
                .await?;
                enqueue_in_tx_flat_mysql(
                    &mut tx,
                    SHOP_ENTITLEMENT_CHANGED,
                    json!({"entitlement_id": entitlement_id, "user_id": user_id, "status": "owned"}),
                )
                .await?;

                Ok(json!({
                    "order_id": order_id,
                    "product_id": product.id,
                    "product_version": product.version,
                    "quantity": quantity,
                    "unit_price": product.unit_price,
                    "total_amount": total,
                    "status": "succeeded",
                    "entitlement_id": entitlement_id,
                    "balance_after": op.transactions[0].balance_after,
                    "created_at": now,
                }))
            }
            .await;
            match outcome {
                Ok(v) => {
                    tx.commit().await?;
                    Ok(v)
                }
                Err(e) => {
                    let _ = tx.rollback().await;
                    Err(e)
                }
            }
        }
    }
}

/// 幂等请求摘要（规范化：user_id|product_id|quantity|idempotency_key）。
fn hash_request(user_id: &str, product_id: &str, quantity: i64, key: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(format!("{user_id}|{product_id}|{quantity}|{key}"));
    hex::encode(hasher.finalize())
}

fn is_duplicate_key_sqlite(err: &sqlx::Error) -> bool {
    matches!(err, sqlx::Error::Database(db) if db.is_unique_violation())
}

fn is_duplicate_key(err: &sqlx::Error) -> bool {
    matches!(err, sqlx::Error::Database(db) if db.is_unique_violation())
}

fn row_to_order(row: &sqlx::sqlite::SqliteRow) -> OrderRow {
    OrderRow {
        id: row.get("id"),
        user_id: row.get("user_id"),
        product_id: row.get("product_id"),
        product_version: row.get("product_version"),
        quantity: row.get("quantity"),
        currency_id: row.get("currency_id"),
        unit_price: row.get("unit_price"),
        total_amount: row.get("total_amount"),
        point_operation_id: row.get("point_operation_id"),
        status: row.get("status"),
        idempotency_key: row.get("idempotency_key"),
        created_at: row.get("created_at"),
    }
}

fn row_to_order_mysql(row: &sqlx::mysql::MySqlRow) -> OrderRow {
    OrderRow {
        id: row.get("id"),
        user_id: row.get("user_id"),
        product_id: row.get("product_id"),
        product_version: row.get("product_version"),
        quantity: row.get("quantity"),
        currency_id: row.get("currency_id"),
        unit_price: row.get("unit_price"),
        total_amount: row.get("total_amount"),
        point_operation_id: row.get("point_operation_id"),
        status: row.get("status"),
        idempotency_key: row.get("idempotency_key"),
        created_at: row.get("created_at"),
    }
}

/// 查看订单（仅本人；admin 可传 user_id）。
pub async fn get_order(
    pool: &DatabasePool,
    user_id: &str,
    order_id: &str,
    is_admin: bool,
) -> Result<Value, ShopError> {
    match pool {
        Either::Left(p) => {
            let row = sqlx::query(
                "SELECT id, user_id, product_id, product_version, quantity, currency_id, unit_price, total_amount, point_operation_id, status, idempotency_key, created_at
                 FROM shop_orders WHERE id = ?",
            )
            .bind(order_id)
            .fetch_optional(p)
            .await?
            .ok_or_else(|| ShopError::NotFound(format!("order {order_id}")))?;
            if !is_admin {
                let owner: String = row.get("user_id");
                if owner != user_id {
                    return Err(ShopError::Forbidden("not your order".into()));
                }
            }
            Ok(order_json(&row_to_order(&row)))
        }
        Either::Right(p) => {
            let row = sqlx::query(
                "SELECT id, user_id, product_id, product_version, quantity, currency_id, unit_price, total_amount, point_operation_id, status, idempotency_key, created_at
                 FROM shop_orders WHERE id = ?",
            )
            .bind(order_id)
            .fetch_optional(p)
            .await?
            .ok_or_else(|| ShopError::NotFound(format!("order {order_id}")))?;
            if !is_admin {
                let owner: String = row.get("user_id");
                if owner != user_id {
                    return Err(ShopError::Forbidden("not your order".into()));
                }
            }
            Ok(order_json(&row_to_order_mysql(&row)))
        }
    }
}

/// 我的权益（自动处理过期：过期 → expired 状态投影，不删持有历史）。
pub async fn list_my_entitlements(pool: &DatabasePool, user_id: &str) -> Result<Value, ShopError> {
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            let rows = sqlx::query(
                "SELECT id, product_id, status, quantity, remaining_quantity, valid_from, expires_at, equipped_at, revoked_at, created_at
                 FROM user_entitlements WHERE user_id = ? ORDER BY created_at DESC",
            )
            .bind(user_id)
            .fetch_all(p)
            .await?;
            let items: Vec<Value> = rows
                .iter()
                .map(|row| {
                    let expires_at: Option<i64> = row.get("expires_at");
                    let mut status: String = row.get("status");
                    if status == "owned" && expires_at.is_some_and(|e| e < now) {
                        status = "expired".to_string();
                    }
                    json!({
                        "id": row.get::<String,_>("id"),
                        "product_id": row.get::<String,_>("product_id"),
                        "status": status,
                        "quantity": row.get::<i64,_>("quantity"),
                        "remaining_quantity": row.get::<i64,_>("remaining_quantity"),
                        "valid_from": row.get::<i64,_>("valid_from"),
                        "expires_at": expires_at,
                        "equipped_at": row.get::<Option<i64>,_>("equipped_at"),
                        "revoked_at": row.get::<Option<i64>,_>("revoked_at"),
                        "created_at": row.get::<i64,_>("created_at"),
                    })
                })
                .collect();
            Ok(json!({ "entitlements": items }))
        }
        Either::Right(p) => {
            let rows = sqlx::query(
                "SELECT id, product_id, status, quantity, remaining_quantity, valid_from, expires_at, equipped_at, revoked_at, created_at
                 FROM user_entitlements WHERE user_id = ? ORDER BY created_at DESC",
            )
            .bind(user_id)
            .fetch_all(p)
            .await?;
            let items: Vec<Value> = rows
                .iter()
                .map(|row| {
                    let expires_at: Option<i64> = row.get("expires_at");
                    let mut status: String = row.get("status");
                    if status == "owned" && expires_at.is_some_and(|e| e < now) {
                        status = "expired".to_string();
                    }
                    json!({
                        "id": row.get::<String,_>("id"),
                        "product_id": row.get::<String,_>("product_id"),
                        "status": status,
                        "quantity": row.get::<i64,_>("quantity"),
                        "remaining_quantity": row.get::<i64,_>("remaining_quantity"),
                        "valid_from": row.get::<i64,_>("valid_from"),
                        "expires_at": expires_at,
                        "equipped_at": row.get::<Option<i64>,_>("equipped_at"),
                        "revoked_at": row.get::<Option<i64>,_>("revoked_at"),
                        "created_at": row.get::<i64,_>("created_at"),
                    })
                })
                .collect();
            Ok(json!({ "entitlements": items }))
        }
    }
}

/// 装备权益（slot 互斥；badges ≤ 3）。
#[allow(clippy::explicit_auto_deref)]
pub async fn equip(
    pool: &DatabasePool,
    user_id: &str,
    entitlement_id: &str,
) -> Result<Value, ShopError> {
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            let mut conn = p.acquire().await?;
            sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
            let outcome: Result<Value, ShopError> = async {
                let row = sqlx::query(
                    "SELECT e.id, e.product_id, e.status, e.expires_at, e.quantity, e.remaining_quantity, p.slot, p.kind
                     FROM user_entitlements e JOIN shop_products p ON p.id = e.product_id
                     WHERE e.id = ? AND e.user_id = ?",
                )
                .bind(entitlement_id)
                .bind(user_id)
                .fetch_optional(&mut *conn)
                .await?
                .ok_or(ShopError::EntitlementNotOwned)?;
                let status: String = row.get("status");
                let expires_at: Option<i64> = row.get("expires_at");
                let slot: String = row.get("slot");
                let kind: String = row.get("kind");
                if status == "revoked" || status == "consumed" {
                    return Err(ShopError::EntitlementNotOwned);
                }
                if status == "expired" || expires_at.is_some_and(|e| e < now) {
                    return Err(ShopError::EntitlementNotOwned);
                }
                // 徽章 slot 最多 3 个 equipped。
                if slot == "profile_badge" {
                    let equipped: i64 = sqlx::query_scalar(
                        "SELECT COUNT(*) FROM user_entitlements WHERE user_id = ? AND status = 'equipped' AND id IN
                         (SELECT e.id FROM user_entitlements e JOIN shop_products p ON p.id = e.product_id WHERE p.slot = 'profile_badge')",
                    )
                    .bind(user_id)
                    .fetch_one(&mut *conn)
                    .await?;
                    if equipped >= 3 {
                        return Err(ShopError::SlotConflict);
                    }
                }
                // 同 slot 互斥：卸下其他 equipped。
                sqlx::query(
                    "UPDATE user_entitlements SET status = 'owned', equipped_at = NULL, updated_at = ?
                     WHERE user_id = ? AND status = 'equipped' AND id IN
                     (SELECT e.id FROM user_entitlements e JOIN shop_products p ON p.id = e.product_id WHERE p.slot = ?)",
                )
                .bind(now)
                .bind(user_id)
                .bind(&slot)
                .execute(&mut *conn)
                .await?;
                sqlx::query(
                    "UPDATE user_entitlements SET status = 'equipped', equipped_at = ?, updated_at = ? WHERE id = ? AND user_id = ?",
                )
                .bind(now)
                .bind(now)
                .bind(entitlement_id)
                .bind(user_id)
                .execute(&mut *conn)
                .await?;
                let _ = kind;
                Ok(json!({ "entitlement_id": entitlement_id, "slot": slot, "status": "equipped" }))
            }
            .await;
            match outcome {
                Ok(v) => {
                    sqlx::query("COMMIT").execute(&mut *conn).await?;
                    Ok(v)
                }
                Err(e) => {
                    let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                    Err(e)
                }
            }
        }
        Either::Right(p) => {
            let mut tx = p.begin().await?;
            let outcome: Result<Value, ShopError> = async {
                let row = sqlx::query(
                    "SELECT e.id, e.product_id, e.status, e.expires_at, p.slot, p.kind
                     FROM user_entitlements e JOIN shop_products p ON p.id = e.product_id
                     WHERE e.id = ? AND e.user_id = ?",
                )
                .bind(entitlement_id)
                .bind(user_id)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(ShopError::EntitlementNotOwned)?;
                let status: String = row.get("status");
                let expires_at: Option<i64> = row.get("expires_at");
                let slot: String = row.get("slot");
                if status == "revoked" || status == "consumed" {
                    return Err(ShopError::EntitlementNotOwned);
                }
                if status == "expired" || expires_at.is_some_and(|e| e < now) {
                    return Err(ShopError::EntitlementNotOwned);
                }
                if slot == "profile_badge" {
                    let equipped: i64 = sqlx::query_scalar(
                        "SELECT COUNT(*) FROM user_entitlements e JOIN shop_products p ON p.id = e.product_id
                         WHERE e.user_id = ? AND e.status = 'equipped' AND p.slot = 'profile_badge'",
                    )
                    .bind(user_id)
                    .fetch_one(&mut *tx)
                    .await?;
                    if equipped >= 3 {
                        return Err(ShopError::SlotConflict);
                    }
                }
                sqlx::query(
                    "UPDATE user_entitlements e JOIN shop_products p ON p.id = e.product_id
                     SET e.status = 'owned', e.equipped_at = NULL, e.updated_at = ?
                     WHERE e.user_id = ? AND e.status = 'equipped' AND p.slot = ?",
                )
                .bind(now)
                .bind(user_id)
                .bind(&slot)
                .execute(&mut *tx)
                .await?;
                sqlx::query(
                    "UPDATE user_entitlements SET status = 'equipped', equipped_at = ?, updated_at = ? WHERE id = ? AND user_id = ?",
                )
                .bind(now)
                .bind(now)
                .bind(entitlement_id)
                .bind(user_id)
                .execute(&mut *tx)
                .await?;
                Ok(json!({ "entitlement_id": entitlement_id, "slot": slot, "status": "equipped" }))
            }
            .await;
            match outcome {
                Ok(v) => {
                    tx.commit().await?;
                    Ok(v)
                }
                Err(e) => {
                    let _ = tx.rollback().await;
                    Err(e)
                }
            }
        }
    }
}

/// 卸下权益。
pub async fn unequip(
    pool: &DatabasePool,
    user_id: &str,
    entitlement_id: &str,
) -> Result<Value, ShopError> {
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            let affected = sqlx::query(
                "UPDATE user_entitlements SET status = 'owned', equipped_at = NULL, updated_at = ? WHERE id = ? AND user_id = ? AND status = 'equipped'",
            )
            .bind(now)
            .bind(entitlement_id)
            .bind(user_id)
            .execute(p)
            .await?
            .rows_affected();
            if affected != 1 {
                return Err(ShopError::EntitlementNotOwned);
            }
            Ok(json!({ "entitlement_id": entitlement_id, "status": "owned" }))
        }
        Either::Right(p) => {
            let affected = sqlx::query(
                "UPDATE user_entitlements SET status = 'owned', equipped_at = NULL, updated_at = ? WHERE id = ? AND user_id = ? AND status = 'equipped'",
            )
            .bind(now)
            .bind(entitlement_id)
            .bind(user_id)
            .execute(p)
            .await?
            .rows_affected();
            if affected != 1 {
                return Err(ShopError::EntitlementNotOwned);
            }
            Ok(json!({ "entitlement_id": entitlement_id, "status": "owned" }))
        }
    }
}

/// 我的 presentation（只输出后端安全 Token；无权/过期 → 默认展示）。
pub async fn get_presentation(pool: &DatabasePool, user_id: &str) -> Result<Value, ShopError> {
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            let row = sqlx::query(
                "SELECT nickname_decoration_id, nickname_color_id, avatar_frame_id, avatar_attachment_id, profile_effect_id, title_prefix_id, profile_badge_ids_json, post_effect_id, version
                 FROM user_presentations WHERE user_id = ?",
            )
            .bind(user_id)
            .fetch_optional(p)
            .await?;
            let mut badges: Vec<String> = Vec::new();
            let mut version: i64 = 1;
            if let Some(row) = &row {
                version = row.get("version");
                let json_str: Option<String> = row.get("profile_badge_ids_json");
                if let Some(json_str) = json_str {
                    badges = serde_json::from_str(&json_str).unwrap_or_default();
                }
            }
            Ok(json!({
                "user_id": user_id,
                "version": version,
                "nickname_decoration_id": row.as_ref().and_then(|r| r.get::<Option<String>,_>("nickname_decoration_id")),
                "nickname_color_id": row.as_ref().and_then(|r| r.get::<Option<String>,_>("nickname_color_id")),
                "avatar_frame_id": row.as_ref().and_then(|r| r.get::<Option<String>,_>("avatar_frame_id")),
                "avatar_attachment_id": row.as_ref().and_then(|r| r.get::<Option<String>,_>("avatar_attachment_id")),
                "profile_effect_id": row.as_ref().and_then(|r| r.get::<Option<String>,_>("profile_effect_id")),
                "title_prefix_id": row.as_ref().and_then(|r| r.get::<Option<String>,_>("title_prefix_id")),
                "post_effect_id": row.as_ref().and_then(|r| r.get::<Option<String>,_>("post_effect_id")),
                "profile_badge_ids": badges,
                "now": now,
            }))
        }
        Either::Right(p) => {
            let row = sqlx::query(
                "SELECT nickname_decoration_id, nickname_color_id, avatar_frame_id, avatar_attachment_id, profile_effect_id, title_prefix_id, profile_badge_ids_json, post_effect_id, version
                 FROM user_presentations WHERE user_id = ?",
            )
            .bind(user_id)
            .fetch_optional(p)
            .await?;
            let mut badges: Vec<String> = Vec::new();
            let mut version: i64 = 1;
            if let Some(row) = &row {
                version = row.get("version");
                let json_str: Option<String> = row.get("profile_badge_ids_json");
                if let Some(json_str) = json_str {
                    badges = serde_json::from_str(&json_str).unwrap_or_default();
                }
            }
            Ok(json!({
                "user_id": user_id,
                "version": version,
                "nickname_decoration_id": row.as_ref().and_then(|r| r.get::<Option<String>,_>("nickname_decoration_id")),
                "nickname_color_id": row.as_ref().and_then(|r| r.get::<Option<String>,_>("nickname_color_id")),
                "avatar_frame_id": row.as_ref().and_then(|r| r.get::<Option<String>,_>("avatar_frame_id")),
                "avatar_attachment_id": row.as_ref().and_then(|r| r.get::<Option<String>,_>("avatar_attachment_id")),
                "profile_effect_id": row.as_ref().and_then(|r| r.get::<Option<String>,_>("profile_effect_id")),
                "title_prefix_id": row.as_ref().and_then(|r| r.get::<Option<String>,_>("title_prefix_id")),
                "post_effect_id": row.as_ref().and_then(|r| r.get::<Option<String>,_>("post_effect_id")),
                "profile_badge_ids": badges,
                "now": now,
            }))
        }
    }
}

// ─── Admin ───────────────────────────────────────────────────────────────

/// admin 商品列表。
pub async fn list_admin_products(pool: &DatabasePool) -> Result<Value, ShopError> {
    list_products(pool, true).await
}

/// 创建商品（admin；reason+recent-auth+审计由路由层完成）。
pub async fn create_product(
    pool: &DatabasePool,
    input: &Value,
    created_by: &str,
) -> Result<Value, ShopError> {
    let kind = input
        .get("kind")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ShopError::Invalid("kind required".into()))?;
    let slug = input
        .get("slug")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ShopError::Invalid("slug required".into()))?;
    let title = input
        .get("title")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ShopError::Invalid("title required".into()))?;
    let slot = input
        .get("slot")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ShopError::Invalid("slot required".into()))?;
    let currency_id = input
        .get("currency_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ShopError::Invalid("currency_id required".into()))?;
    let unit_price = input
        .get("unit_price")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ShopError::Invalid("unit_price required".into()))?;
    if unit_price < 0 {
        return Err(ShopError::Invalid("unit_price must be >= 0".into()));
    }
    let icon_token = input.get("icon_token").and_then(|v| v.as_str());
    let presentation_tokens = input
        .get("presentation_tokens")
        .map(|v| v.to_string())
        .or_else(|| {
            input
                .get("presentation_tokens_json")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        });
    validate_tokens(icon_token, presentation_tokens.as_deref())?;
    if !is_safe_token(slot) {
        return Err(ShopError::Invalid("unsafe slot".into()));
    }

    let now = now_millis();
    let id = uuid::Uuid::now_v7().to_string();
    let quantity_limit = input
        .get("quantity_limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(1)
        .max(1);
    let stock_remaining = input.get("stock_remaining").and_then(|v| v.as_i64());
    let required_level = input
        .get("required_level")
        .and_then(|v| v.as_i64())
        .unwrap_or(1)
        .max(1);
    let validity_seconds = input.get("validity_seconds").and_then(|v| v.as_i64());
    let sale_start_at = input.get("sale_start_at").and_then(|v| v.as_i64());
    let sale_end_at = input.get("sale_end_at").and_then(|v| v.as_i64());
    let refund_policy = input
        .get("refund_policy")
        .and_then(|v| v.as_str())
        .unwrap_or("non_refundable");
    let status = input
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("draft");
    if ![
        "draft",
        "pending_review",
        "published",
        "disabled",
        "retired",
    ]
    .contains(&status)
    {
        return Err(ShopError::Invalid("invalid status".into()));
    }
    if !["non_refundable", "compensation_only", "full_refund"].contains(&refund_policy) {
        return Err(ShopError::Invalid("invalid refund_policy".into()));
    }

    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO shop_products
                     (id, kind, status, slug, title, description_safe, icon_token, presentation_tokens_json, slot, currency_id, unit_price, quantity_limit, stock_remaining, required_level, validity_seconds, sale_start_at, sale_end_at, refund_policy, version, created_by, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?)",
            )
            .bind(&id)
            .bind(kind)
            .bind(status)
            .bind(slug)
            .bind(title)
            .bind(input.get("description_safe").and_then(|v| v.as_str()))
            .bind(icon_token)
            .bind(presentation_tokens.as_deref())
            .bind(slot)
            .bind(currency_id)
            .bind(unit_price)
            .bind(quantity_limit)
            .bind(stock_remaining)
            .bind(required_level)
            .bind(validity_seconds)
            .bind(sale_start_at)
            .bind(sale_end_at)
            .bind(refund_policy)
            .bind(created_by)
            .bind(now)
            .bind(now)
            .execute(p)
            .await?;
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO shop_products
                     (id, kind, status, slug, title, description_safe, icon_token, presentation_tokens_json, slot, currency_id, unit_price, quantity_limit, stock_remaining, required_level, validity_seconds, sale_start_at, sale_end_at, refund_policy, version, created_by, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?)",
            )
            .bind(&id)
            .bind(kind)
            .bind(status)
            .bind(slug)
            .bind(title)
            .bind(input.get("description_safe").and_then(|v| v.as_str()))
            .bind(icon_token)
            .bind(presentation_tokens.as_deref())
            .bind(slot)
            .bind(currency_id)
            .bind(unit_price)
            .bind(quantity_limit)
            .bind(stock_remaining)
            .bind(required_level)
            .bind(validity_seconds)
            .bind(sale_start_at)
            .bind(sale_end_at)
            .bind(refund_policy)
            .bind(created_by)
            .bind(now)
            .bind(now)
            .execute(p)
            .await?;
        }
    }
    Ok(json!({ "id": id, "status": status }))
}

/// 更新商品（admin；新值直接覆盖，version 递增由路由层校验 If-Match）。
pub async fn update_product(
    pool: &DatabasePool,
    id: &str,
    input: &Value,
) -> Result<Value, ShopError> {
    let now = now_millis();
    let icon_token = input.get("icon_token").and_then(|v| v.as_str());
    let presentation_tokens = input
        .get("presentation_tokens")
        .map(|v| v.to_string())
        .or_else(|| {
            input
                .get("presentation_tokens_json")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        });
    validate_tokens(icon_token, presentation_tokens.as_deref())?;
    let unit_price = input.get("unit_price").and_then(|v| v.as_i64());
    if unit_price.is_some_and(|v| v < 0) {
        return Err(ShopError::Invalid("unit_price must be >= 0".into()));
    }
    match pool {
        Either::Left(p) => {
            let affected = sqlx::query(
                "UPDATE shop_products SET
                     title = COALESCE(?, title),
                     description_safe = COALESCE(?, description_safe),
                     icon_token = COALESCE(?, icon_token),
                     presentation_tokens_json = COALESCE(?, presentation_tokens_json),
                     unit_price = COALESCE(?, unit_price),
                     stock_remaining = COALESCE(?, stock_remaining),
                     required_level = COALESCE(?, required_level),
                     quantity_limit = COALESCE(?, quantity_limit),
                     validity_seconds = COALESCE(?, validity_seconds),
                     sale_start_at = COALESCE(?, sale_start_at),
                     sale_end_at = COALESCE(?, sale_end_at),
                     version = version + 1, updated_at = ?
                 WHERE id = ?",
            )
            .bind(input.get("title").and_then(|v| v.as_str()))
            .bind(input.get("description_safe").and_then(|v| v.as_str()))
            .bind(icon_token)
            .bind(presentation_tokens.as_deref())
            .bind(unit_price)
            .bind(input.get("stock_remaining").and_then(|v| v.as_i64()))
            .bind(input.get("required_level").and_then(|v| v.as_i64()))
            .bind(input.get("quantity_limit").and_then(|v| v.as_i64()))
            .bind(input.get("validity_seconds").and_then(|v| v.as_i64()))
            .bind(input.get("sale_start_at").and_then(|v| v.as_i64()))
            .bind(input.get("sale_end_at").and_then(|v| v.as_i64()))
            .bind(now)
            .bind(id)
            .execute(p)
            .await?
            .rows_affected();
            if affected != 1 {
                return Err(ShopError::NotFound(format!("product {id}")));
            }
        }
        Either::Right(p) => {
            let affected = sqlx::query(
                "UPDATE shop_products SET
                     title = COALESCE(?, title),
                     description_safe = COALESCE(?, description_safe),
                     icon_token = COALESCE(?, icon_token),
                     presentation_tokens_json = COALESCE(?, presentation_tokens_json),
                     unit_price = COALESCE(?, unit_price),
                     stock_remaining = COALESCE(?, stock_remaining),
                     required_level = COALESCE(?, required_level),
                     quantity_limit = COALESCE(?, quantity_limit),
                     validity_seconds = COALESCE(?, validity_seconds),
                     sale_start_at = COALESCE(?, sale_start_at),
                     sale_end_at = COALESCE(?, sale_end_at),
                     version = version + 1, updated_at = ?
                 WHERE id = ?",
            )
            .bind(input.get("title").and_then(|v| v.as_str()))
            .bind(input.get("description_safe").and_then(|v| v.as_str()))
            .bind(icon_token)
            .bind(presentation_tokens.as_deref())
            .bind(unit_price)
            .bind(input.get("stock_remaining").and_then(|v| v.as_i64()))
            .bind(input.get("required_level").and_then(|v| v.as_i64()))
            .bind(input.get("quantity_limit").and_then(|v| v.as_i64()))
            .bind(input.get("validity_seconds").and_then(|v| v.as_i64()))
            .bind(input.get("sale_start_at").and_then(|v| v.as_i64()))
            .bind(input.get("sale_end_at").and_then(|v| v.as_i64()))
            .bind(now)
            .bind(id)
            .execute(p)
            .await?
            .rows_affected();
            if affected != 1 {
                return Err(ShopError::NotFound(format!("product {id}")));
            }
        }
    }
    Ok(json!({ "id": id, "updated_at": now }))
}

/// 发布商品。
pub async fn publish_product(pool: &DatabasePool, id: &str) -> Result<Value, ShopError> {
    set_product_status(pool, id, "published").await
}

/// 禁用商品。
pub async fn disable_product(pool: &DatabasePool, id: &str) -> Result<Value, ShopError> {
    set_product_status(pool, id, "disabled").await
}

async fn set_product_status(
    pool: &DatabasePool,
    id: &str,
    status: &str,
) -> Result<Value, ShopError> {
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            let affected = sqlx::query(
                "UPDATE shop_products SET status = ?, version = version + 1, updated_at = ? WHERE id = ?",
            )
            .bind(status)
            .bind(now)
            .bind(id)
            .execute(p)
            .await?
            .rows_affected();
            if affected != 1 {
                return Err(ShopError::NotFound(format!("product {id}")));
            }
        }
        Either::Right(p) => {
            let affected = sqlx::query(
                "UPDATE shop_products SET status = ?, version = version + 1, updated_at = ? WHERE id = ?",
            )
            .bind(status)
            .bind(now)
            .bind(id)
            .execute(p)
            .await?
            .rows_affected();
            if affected != 1 {
                return Err(ShopError::NotFound(format!("product {id}")));
            }
        }
    }
    Ok(json!({ "id": id, "status": status }))
}

/// admin 订单列表。
pub async fn list_admin_orders(pool: &DatabasePool) -> Result<Value, ShopError> {
    match pool {
        Either::Left(p) => {
            let rows = sqlx::query(
                "SELECT id, user_id, product_id, product_version, quantity, currency_id, unit_price, total_amount, point_operation_id, status, idempotency_key, created_at
                 FROM shop_orders ORDER BY created_at DESC",
            )
            .fetch_all(p)
            .await?;
            let items: Vec<Value> = rows
                .iter()
                .map(|row| {
                    let mut v = order_json(&row_to_order(row));
                    if let Some(obj) = v.as_object_mut() {
                        obj.insert("user_id".into(), json!(row.get::<String, _>("user_id")));
                    }
                    v
                })
                .collect();
            Ok(json!({ "orders": items }))
        }
        Either::Right(p) => {
            let rows = sqlx::query(
                "SELECT id, user_id, product_id, product_version, quantity, currency_id, unit_price, total_amount, point_operation_id, status, idempotency_key, created_at
                 FROM shop_orders ORDER BY created_at DESC",
            )
            .fetch_all(p)
            .await?;
            let items: Vec<Value> = rows
                .iter()
                .map(|row| {
                    let mut v = order_json(&row_to_order_mysql(row));
                    if let Some(obj) = v.as_object_mut() {
                        obj.insert("user_id".into(), json!(row.get::<String, _>("user_id")));
                    }
                    v
                })
                .collect();
            Ok(json!({ "orders": items }))
        }
    }
}

/// 订单退款（admin；数字装扮默认不可退款；可退订单用 Reversal 补偿流水）。
#[allow(clippy::explicit_auto_deref)]
pub async fn refund_order(
    pool: &DatabasePool,
    order_id: &str,
    actor_id: &str,
    reason: &str,
) -> Result<Value, ShopError> {
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            let mut conn = p.acquire().await?;
            sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
            let outcome: Result<Value, ShopError> = async {
                let row = sqlx::query(
                    "SELECT o.id, o.user_id, o.product_id, o.product_version, o.quantity, o.currency_id, o.unit_price, o.total_amount, o.point_operation_id, o.status, p.refund_policy
                     FROM shop_orders o JOIN shop_products p ON p.id = o.product_id
                     WHERE o.id = ?",
                )
                .bind(order_id)
                .fetch_optional(&mut *conn)
                .await?
                .ok_or_else(|| ShopError::NotFound(format!("order {order_id}")))?;
                let status: String = row.get("status");
                let refund_policy: String = row.get("refund_policy");
                if status != "succeeded" {
                    return Err(ShopError::Invalid("order not refundable in current state".into()));
                }
                if refund_policy == "non_refundable" {
                    return Err(ShopError::NotRefundable);
                }
                let user_id: String = row.get("user_id");
                let currency_id: String = row.get("currency_id");
                let total: i64 = row.get("total_amount");
                let op_id: String = row.get("point_operation_id");
                let cmd = LedgerCommand {
                    idempotency_scope: "shop".to_string(),
                    idempotency_key: uuid::Uuid::now_v7().to_string(),
                    kind: LedgerKind::Reversal,
                    actor_id: Some(actor_id.to_string()),
                    user_id: user_id.clone(),
                    currency_id: currency_id.clone(),
                    delta_balance: total,
                    delta_frozen: 0,
                    source_type: Some("order".to_string()),
                    source_id: Some(order_id.to_string()),
                    memo: format!("refund {reason}"),
                    reverses_operation_id: Some(op_id),
                };
                let op = ledger::apply_operation_in_sqlite_tx(&mut *conn, cmd, now).await?;
                sqlx::query(
                    "UPDATE shop_orders SET status = 'refunded', updated_at = ? WHERE id = ?",
                )
                .bind(now)
                .bind(order_id)
                .execute(&mut *conn)
                .await?;
                let aff = sqlx::query(
                    "UPDATE user_entitlements SET status = 'revoked', revoked_at = ?, updated_at = ?
                     WHERE order_id = ? AND status IN ('owned','equipped')",
                )
                .bind(now)
                .bind(now)
                .bind(order_id)
                .execute(&mut *conn)
                .await?
                .rows_affected();
                debug_assert_eq!(aff, 1, "退款必须撤销对应权益");
                AuditEntry::user_action(actor_id, "shop.refund")
                    .with_target("order", order_id)
                    .with_reason(reason)
                    .with_policy_version(AUTHZ_POLICY_VERSION)
                    .record_into_sqlite(&mut *conn)
                    .await
                    .map_err(ShopError::from)?;
                Ok(json!({
                    "order_id": order_id,
                    "status": "refunded",
                    "refunded_amount": total,
                    "compensation_operation_id": op.operation_id,
                }))
            }
            .await;
            match outcome {
                Ok(v) => {
                    sqlx::query("COMMIT").execute(&mut *conn).await?;
                    Ok(v)
                }
                Err(e) => {
                    let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                    Err(e)
                }
            }
        }
        Either::Right(p) => {
            let mut tx = p.begin().await?;
            let outcome: Result<Value, ShopError> = async {
                let row = sqlx::query(
                    "SELECT o.id, o.user_id, o.currency_id, o.total_amount, o.point_operation_id, o.status, p.refund_policy
                     FROM shop_orders o JOIN shop_products p ON p.id = o.product_id
                     WHERE o.id = ?",
                )
                .bind(order_id)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or_else(|| ShopError::NotFound(format!("order {order_id}")))?;
                let status: String = row.get("status");
                let refund_policy: String = row.get("refund_policy");
                if status != "succeeded" {
                    return Err(ShopError::Invalid("order not refundable in current state".into()));
                }
                if refund_policy == "non_refundable" {
                    return Err(ShopError::NotRefundable);
                }
                let user_id: String = row.get("user_id");
                let currency_id: String = row.get("currency_id");
                let total: i64 = row.get("total_amount");
                let op_id: String = row.get("point_operation_id");
                let cmd = LedgerCommand {
                    idempotency_scope: "shop".to_string(),
                    idempotency_key: uuid::Uuid::now_v7().to_string(),
                    kind: LedgerKind::Reversal,
                    actor_id: Some(actor_id.to_string()),
                    user_id,
                    currency_id,
                    delta_balance: total,
                    delta_frozen: 0,
                    source_type: Some("order".to_string()),
                    source_id: Some(order_id.to_string()),
                    memo: format!("refund {reason}"),
                    reverses_operation_id: Some(op_id),
                };
                let op = ledger::apply_operation_in_mysql_tx(&mut tx, cmd, now).await?;
                sqlx::query(
                    "UPDATE shop_orders SET status = 'refunded', updated_at = ? WHERE id = ?",
                )
                .bind(now)
                .bind(order_id)
                .execute(&mut *tx)
                .await?;
                sqlx::query(
                    "UPDATE user_entitlements SET status = 'revoked', revoked_at = ?, updated_at = ?
                     WHERE order_id = ? AND status IN ('owned','equipped')",
                )
                .bind(now)
                .bind(now)
                .bind(order_id)
                .execute(&mut *tx)
                .await?;
                AuditEntry::user_action(actor_id, "shop.refund")
                    .with_target("order", order_id)
                    .with_reason(reason)
                    .with_policy_version(AUTHZ_POLICY_VERSION)
                    .record_into_mysql(&mut tx)
                    .await
                    .map_err(ShopError::from)?;
                Ok(json!({
                    "order_id": order_id,
                    "status": "refunded",
                    "refunded_amount": total,
                    "compensation_operation_id": op.operation_id,
                }))
            }
            .await;
            match outcome {
                Ok(v) => {
                    tx.commit().await?;
                    Ok(v)
                }
                Err(e) => {
                    let _ = tx.rollback().await;
                    Err(e)
                }
            }
        }
    }
}

/// 在 SQLite IMMEDIATE 事务内写 Outbox（直接写表，不额外 begin/commit）。
async fn enqueue_in_tx_flat_sqlite(
    conn: &mut sqlx::SqliteConnection,
    event_type: &str,
    payload: Value,
) -> Result<String, ShopError> {
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    let payload_str = serde_json::to_string(&payload).unwrap_or_default();
    sqlx::query(
        "INSERT INTO outbox_events (id, event_type, payload, payload_version, status, attempts, max_attempts, next_attempt_at, created_at)
         VALUES (?, ?, ?, 1, 'pending', 0, 5, ?, ?)",
    )
    .bind(&id)
    .bind(event_type)
    .bind(&payload_str)
    .bind(now)
    .bind(now)
    .execute(&mut *conn)
    .await?;
    Ok(id)
}

/// 在 MySQL 事务内写 Outbox。
async fn enqueue_in_tx_flat_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    event_type: &str,
    payload: Value,
) -> Result<String, ShopError> {
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    let payload_str = serde_json::to_string(&payload).unwrap_or_default();
    sqlx::query(
        "INSERT INTO outbox_events (id, event_type, payload, payload_version, status, attempts, max_attempts, next_attempt_at, created_at)
         VALUES (?, ?, ?, 1, 'pending', 0, 5, ?, ?)",
    )
    .bind(&id)
    .bind(event_type)
    .bind(&payload_str)
    .bind(now)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(id)
}

/// 将 ShopError 映射为 AppError（路由层用）。
pub fn shop_error_to_app(e: ShopError, request_id: &str) -> AppError {
    match e {
        ShopError::NotFound(m) => AppError::not_found(m, request_id),
        ShopError::Invalid(m) => AppError::bad_request(m, request_id, None),
        ShopError::InsufficientBalance => {
            AppError::bad_request("insufficient balance", request_id, None)
        }
        ShopError::OutOfStock => AppError::conflict("out of stock", request_id),
        ShopError::BelowLevel { required } => {
            AppError::bad_request(format!("level {required} required"), request_id, None)
        }
        ShopError::NotInSaleWindow => {
            AppError::bad_request("product not in sale window", request_id, None)
        }
        ShopError::PurchaseLimitExceeded => {
            AppError::bad_request("purchase limit exceeded", request_id, None)
        }
        ShopError::IdempotencyConflict => {
            AppError::conflict("idempotency key conflict", request_id)
        }
        ShopError::EntitlementNotOwned => AppError::not_found("entitlement not found", request_id),
        ShopError::SlotConflict => AppError::conflict("equipment slot conflict", request_id),
        ShopError::NotRefundable => {
            AppError::bad_request("order is not refundable", request_id, None)
        }
        ShopError::Forbidden(m) => AppError::forbidden(m, request_id),
        ShopError::Db(m) => AppError::internal(m, request_id),
    }
}
