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
    /// 展示版本冲突。
    VersionConflict,
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
            Self::VersionConflict => write!(f, "presentation version conflict"),
            Self::EntitlementNotOwned => write!(f, "entitlement not owned or invalid"),
            Self::SlotConflict => write!(f, "equipment slot conflict"),
            Self::NotRefundable => write!(f, "order is not refundable"),
            Self::Forbidden(msg) => write!(f, "shop forbidden: {msg}"),
        }
    }
}

impl std::error::Error for ShopError {}

impl ShopError {
    /// 稳定错误码（docs/ERROR-CODES.md；M16-HARNESS-04 路由层按此输出 Problem code）。
    pub fn code(&self) -> &'static str {
        match self {
            Self::Db(_) => "internal_error",
            Self::NotFound(_) => "not_found",
            Self::Invalid(_) => "invalid_request",
            Self::InsufficientBalance => "insufficient_funds",
            Self::OutOfStock => "shop_stock_exhausted",
            Self::BelowLevel { .. } => "invalid_request",
            Self::NotInSaleWindow => "product_unavailable",
            Self::PurchaseLimitExceeded => "shop_purchase_limit_exceeded",
            Self::IdempotencyConflict => "idempotency_conflict",
            Self::VersionConflict => "version_conflict",
            Self::EntitlementNotOwned => "entitlement_not_usable",
            Self::SlotConflict => "presentation_slot_conflict",
            Self::NotRefundable => "refund_not_allowed",
            Self::Forbidden(_) => "forbidden",
        }
    }
}

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
    pub asset_attachment_id: Option<String>,
    pub slot: String,
    pub currency_id: String,
    /// 结算货币 code/name（LEFT JOIN currencies；悬空引用容忍为 None）。
    pub currency_code: Option<String>,
    pub currency_name: Option<String>,
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
        asset_attachment_id: row.get("asset_attachment_id"),
        slot: row.get("slot"),
        currency_id: row.get("currency_id"),
        currency_code: row.get("currency_code"),
        currency_name: row.get("currency_name"),
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
        asset_attachment_id: row.get("asset_attachment_id"),
        slot: row.get("slot"),
        currency_id: row.get("currency_id"),
        currency_code: row.get("currency_code"),
        currency_name: row.get("currency_name"),
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

/// 商品列 + 结算货币投影（LEFT JOIN currencies；货币被删除时不阻断商品展示）。
const PRODUCT_SELECT: &str = "SELECT p.id, p.kind, p.status, p.slug, p.title, p.description_safe, p.icon_token, \
     p.presentation_tokens_json, p.asset_attachment_id, p.slot, p.currency_id, \
     c.code AS currency_code, c.name AS currency_name, p.unit_price, p.quantity_limit, p.stock_remaining, \
     p.required_level, p.validity_seconds, p.sale_start_at, p.sale_end_at, p.refund_policy, p.version, \
     p.created_by, p.created_at, p.updated_at \
     FROM shop_products p LEFT JOIN currencies c ON c.id = p.currency_id";

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
        "asset_attachment_id": p.asset_attachment_id,
        "slot": p.slot,
        "currency_id": p.currency_id,
        "currency_code": p.currency_code,
        "currency_name": p.currency_name,
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
    let row = sqlx::query(&format!("{PRODUCT_SELECT} WHERE p.id = ?"))
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
    let row = sqlx::query(&format!("{PRODUCT_SELECT} WHERE p.id = ?"))
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| ShopError::NotFound(format!("product {id}")))?;
    Ok(product_row_from_mysql(&row))
}

/// 当前用户信任等级（users.trust_level 缓存，可重建）。
async fn user_level(conn: &mut sqlx::SqliteConnection, user_id: &str) -> Result<i64, ShopError> {
    let level: i64 = sqlx::query_scalar("SELECT trust_level FROM users WHERE id = ?")
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
    let level: i64 = sqlx::query_scalar("SELECT trust_level FROM users WHERE id = ?")
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
            if !is_registered_presentation_token(t) {
                return Err(ShopError::Invalid(format!(
                    "unsafe or unregistered presentation token: {t}"
                )));
            }
        }
    }
    Ok(())
}

/// 注册的安全 Token 前缀（白名单枚举）。
const SAFE_TOKEN_PREFIXES: &[&str] = &[
    "nickname.color.",
    "avatar.frame.",
    "profile.effect.",
    "post.effect.",
    "badge.",
    "reaction.pack.",
    "utility.",
    "title.prefix.",
];

const VALID_SLOTS: &[&str] = &[
    "nickname_color",
    "avatar_frame",
    "profile_badges",
    "profile_badge", // 兼容早期种子数据；新商品使用 profile_badges。
    "profile_effect",
    "post_effect",
    "title_prefix",
];

const NICKNAME_COLOR_VALUES: &[&str] = &[
    "blue",
    "purple",
    "green",
    "gold",
    "red",
    "teal",
    "pink",
    "rainbow",
    "breathing",
    "gradient_sunset",
    "gradient_ocean",
    "gradient_aurora",
];

/// 不装备到展示槽位的商品类型（M07-SHOP-UI-09）：消耗品/道具/前缀类，
/// `slot` 允许缺省（存空串）。装备类商品仍要求合法槽位。
const SLOT_OPTIONAL_KINDS: &[&str] = &["reaction_pack", "utility"];

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

fn is_registered_presentation_token(t: &str) -> bool {
    if !is_safe_token(t) {
        return false;
    }
    if let Some(value) = t.strip_prefix("nickname.color.") {
        // 注册枚举色，或样式库定义引用（`c` + id；存在性由
        // cosmetics::validate_token_def_references 异步裁决，此处只做形状放行）。
        return NICKNAME_COLOR_VALUES.contains(&value) || is_def_reference_value(value);
    }
    true
}

/// 样式库定义引用的取值形状：`c` 开头 + 小写字母/数字/下划线（与
/// cosmetics::valid_def_id 一致；最短 `c` + 1 字符由存在性校验兜底）。
fn is_def_reference_value(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 2
        && bytes.len() <= 40
        && bytes[0] == b'c'
        && bytes[1..]
            .iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_')
}

/// 注册昵称色枚举判断（cosmetics 样式库复用：定义之外的取值需查样式库）。
pub fn is_registered_nickname_color(value: &str) -> bool {
    NICKNAME_COLOR_VALUES.contains(&value)
}

fn is_valid_slot(slot: &str) -> bool {
    VALID_SLOTS.contains(&slot)
}

/// 槽位校验（M07-SHOP-UI-09）：装备类商品要求合法槽位；
/// 消耗品/道具类（SLOT_OPTIONAL_KINDS）允许空槽位。
pub(super) fn validate_slot_for_kind(kind: &str, slot: &str) -> Result<(), ShopError> {
    if slot.is_empty() {
        if SLOT_OPTIONAL_KINDS.contains(&kind) {
            return Ok(());
        }
        return Err(ShopError::Invalid(
            "slot required for this product kind".into(),
        ));
    }
    if !is_valid_slot(slot) {
        return Err(ShopError::Invalid("invalid presentation slot".into()));
    }
    Ok(())
}

pub(super) fn validate_asset_kind_slot(kind: &str, slot: &str) -> Result<(), ShopError> {
    if !matches!((kind, slot), ("cosmetic_avatar", "avatar_frame")) {
        return Err(ShopError::Invalid(
            "PNG assets are only supported for avatar frame products".into(),
        ));
    }
    Ok(())
}

pub(super) async fn validate_asset_attachment(
    pool: &DatabasePool,
    asset_attachment_id: &str,
) -> Result<(), ShopError> {
    if asset_attachment_id.is_empty()
        || asset_attachment_id.len() > 64
        || !asset_attachment_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(ShopError::Invalid("invalid asset_attachment_id".into()));
    }
    let ready_png: Option<i64> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT 1 FROM attachments
             WHERE id = ? AND status = 'ready' AND media_type = 'image/png' AND is_public = 1",
            )
            .bind(asset_attachment_id)
            .fetch_optional(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT 1 FROM attachments
             WHERE id = ? AND status = 'ready' AND media_type = 'image/png' AND is_public = 1",
            )
            .bind(asset_attachment_id)
            .fetch_optional(p)
            .await?
        }
    };
    if ready_png != Some(1) {
        return Err(ShopError::Invalid(
            "asset_attachment_id must reference a ready public PNG attachment".into(),
        ));
    }
    Ok(())
}

/// 公开商品列表（只返回 published；admin 传 include_all 返回全部）。
pub async fn list_products(pool: &DatabasePool, include_all: bool) -> Result<Value, ShopError> {
    match pool {
        Either::Left(p) => {
            let rows = sqlx::query(&format!("{PRODUCT_SELECT} ORDER BY p.created_at DESC"))
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
            let rows = sqlx::query(&format!("{PRODUCT_SELECT} ORDER BY p.created_at DESC"))
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

/// 购买响应兼容层：旧调用方读取顶层 order_id，新调用方读取 order.id。
/// 两者指向同一条已提交订单事实，避免成功响应被误判成“处理中”。
fn order_create_result(mut value: Value) -> Value {
    let order_id = value
        .get("id")
        .cloned()
        .or_else(|| value.get("order_id").cloned());
    let Some(order_id) = order_id else {
        return value;
    };

    if value.get("id").is_none() {
        value["id"] = order_id.clone();
    }
    let mut order = value.clone();
    if let Some(object) = order.as_object_mut() {
        object.remove("order");
        object.remove("order_id");
    }
    value["order"] = order;
    value["order_id"] = order_id;
    value
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
    if quantity <= 0 {
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
                // 站点商城配置（0073 shop_site_config）：总开关 + 单笔数量上限
                // 由购买路径真实消费（P0 整改；此前硬编码 100）。
                let cfg: Option<(i64, i64)> = sqlx::query_as(
                    "SELECT enabled, max_quantity_per_order FROM shop_site_config WHERE id = 'singleton'",
                )
                .fetch_optional(&mut *conn)
                .await?;
                let (shop_enabled, max_qty) = cfg.unwrap_or((1, 100));
                if shop_enabled == 0 {
                    return Err(ShopError::Forbidden("shop is disabled".into()));
                }
                if quantity > max_qty {
                    return Err(ShopError::Invalid(
                        "quantity exceeds site order limit".into(),
                    ));
                }
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
                    let mut v = order_create_result(order_json(&row_to_order(&row)));
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
                            let mut v = order_create_result(order_json(&row_to_order(&row)));
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

                Ok(order_create_result(json!({
                    "order_id": order_id,
                    "product_id": product.id,
                    "product_version": product.version,
                    "currency_id": product.currency_id,
                    "quantity": quantity,
                    "unit_price": product.unit_price,
                    "total_amount": total,
                    "status": "succeeded",
                    "entitlement_id": entitlement_id,
                     "entitlement_status": "granted",
                    "balance_after": op.transactions[0].balance_after,
                    "created_at": now,
                })))
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
                // 站点商城配置（0073 shop_site_config）：总开关 + 单笔数量上限
                // 由购买路径真实消费（与 SQLite 分支同语义）。
                let cfg: Option<(i64, i64)> = sqlx::query_as(
                    "SELECT enabled, max_quantity_per_order FROM shop_site_config WHERE id = 'singleton'",
                )
                .fetch_optional(&mut *tx)
                .await?;
                let (shop_enabled, max_qty) = cfg.unwrap_or((1, 100));
                if shop_enabled == 0 {
                    return Err(ShopError::Forbidden("shop is disabled".into()));
                }
                if quantity > max_qty {
                    return Err(ShopError::Invalid(
                        "quantity exceeds site order limit".into(),
                    ));
                }
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
                    let mut v = order_create_result(order_json(&row_to_order_mysql(&row)));
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
                        "SELECT CAST(COALESCE(SUM(quantity),0) AS SIGNED) FROM shop_orders \
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
                            let mut v = order_create_result(order_json(&row_to_order_mysql(&row)));
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

                Ok(order_create_result(json!({
                    "order_id": order_id,
                    "product_id": product.id,
                    "product_version": product.version,
                    "currency_id": product.currency_id,
                    "quantity": quantity,
                    "unit_price": product.unit_price,
                    "total_amount": total,
                    "status": "succeeded",
                    "entitlement_id": entitlement_id,
                     "entitlement_status": "granted",
                    "balance_after": op.transactions[0].balance_after,
                    "created_at": now,
                })))
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
                "SELECT o.id, o.user_id, o.product_id, o.product_version, o.quantity, o.currency_id, o.unit_price, o.total_amount, o.point_operation_id, o.status, o.idempotency_key, o.created_at, o.updated_at,
                         p.title AS product_title, c.code AS currency_code, c.name AS currency_name,
                         e.id AS entitlement_id, e.status AS entitlement_status
                 FROM shop_orders o LEFT JOIN shop_products p ON p.id = o.product_id
                 LEFT JOIN currencies c ON c.id = o.currency_id
                 LEFT JOIN user_entitlements e ON e.order_id = o.id
                 WHERE o.id = ?",
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
            let mut value = order_json(&row_to_order(&row));
            if let Some(obj) = value.as_object_mut() {
                obj.insert(
                    "product_title".into(),
                    json!(row.get::<Option<String>, _>("product_title")),
                );
                obj.insert(
                    "currency_code".into(),
                    json!(row.get::<Option<String>, _>("currency_code")),
                );
                obj.insert(
                    "currency_name".into(),
                    json!(row.get::<Option<String>, _>("currency_name")),
                );
                obj.insert("updated_at".into(), json!(row.get::<i64, _>("updated_at")));
                obj.insert(
                    "entitlement_id".into(),
                    json!(row.get::<Option<String>, _>("entitlement_id")),
                );
                let entitlement_status = match row
                    .get::<Option<String>, _>("entitlement_status")
                    .as_deref()
                {
                    Some("revoked") => "revoked",
                    Some(_) => "granted",
                    None => "pending",
                };
                obj.insert("entitlement_status".into(), json!(entitlement_status));
            }
            Ok(value)
        }
        Either::Right(p) => {
            let row = sqlx::query(
                "SELECT o.id, o.user_id, o.product_id, o.product_version, o.quantity, o.currency_id, o.unit_price, o.total_amount, o.point_operation_id, o.status, o.idempotency_key, o.created_at, o.updated_at,
                         p.title AS product_title, c.code AS currency_code, c.name AS currency_name,
                         e.id AS entitlement_id, e.status AS entitlement_status
                 FROM shop_orders o LEFT JOIN shop_products p ON p.id = o.product_id
                 LEFT JOIN currencies c ON c.id = o.currency_id
                 LEFT JOIN user_entitlements e ON e.order_id = o.id
                 WHERE o.id = ?",
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
            let mut value = order_json(&row_to_order_mysql(&row));
            if let Some(obj) = value.as_object_mut() {
                obj.insert(
                    "product_title".into(),
                    json!(row.get::<Option<String>, _>("product_title")),
                );
                obj.insert(
                    "currency_code".into(),
                    json!(row.get::<Option<String>, _>("currency_code")),
                );
                obj.insert(
                    "currency_name".into(),
                    json!(row.get::<Option<String>, _>("currency_name")),
                );
                obj.insert("updated_at".into(), json!(row.get::<i64, _>("updated_at")));
                obj.insert(
                    "entitlement_id".into(),
                    json!(row.get::<Option<String>, _>("entitlement_id")),
                );
                let entitlement_status = match row
                    .get::<Option<String>, _>("entitlement_status")
                    .as_deref()
                {
                    Some("revoked") => "revoked",
                    Some(_) => "granted",
                    None => "pending",
                };
                obj.insert("entitlement_status".into(), json!(entitlement_status));
            }
            Ok(value)
        }
    }
}

/// 我的权益（自动处理过期：过期 → expired 状态投影，不删持有历史）。
pub async fn list_my_entitlements(pool: &DatabasePool, user_id: &str) -> Result<Value, ShopError> {
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            let rows = sqlx::query(
                "SELECT e.id, e.product_id, e.status, e.quantity, e.remaining_quantity, e.valid_from, e.expires_at, e.equipped_at, e.revoked_at, e.created_at,
                         p.title AS product_title, p.kind, p.slot, p.icon_token, p.presentation_tokens_json, p.asset_attachment_id
                 FROM user_entitlements e JOIN shop_products p ON p.id = e.product_id
                  WHERE e.user_id = ? ORDER BY e.created_at DESC",
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
                        "product_title": row.get::<String,_>("product_title"),
                        "kind": row.get::<String,_>("kind"),
                        "slot": row.get::<String,_>("slot"),
                        "icon_token": row.get::<Option<String>,_>("icon_token"),
                        "presentation_tokens": row.get::<Option<String>,_>("presentation_tokens_json").as_deref().and_then(|s| serde_json::from_str::<Vec<String>>(s).ok()),
                        "asset_attachment_id": row.get::<Option<String>,_>("asset_attachment_id"),
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
                "SELECT e.id, e.product_id, e.status, e.quantity, e.remaining_quantity, e.valid_from, e.expires_at, e.equipped_at, e.revoked_at, e.created_at,
                         p.title AS product_title, p.kind, p.slot, p.icon_token, p.presentation_tokens_json, p.asset_attachment_id
                 FROM user_entitlements e JOIN shop_products p ON p.id = e.product_id
                  WHERE e.user_id = ? ORDER BY e.created_at DESC",
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
                        "product_title": row.get::<String,_>("product_title"),
                        "kind": row.get::<String,_>("kind"),
                        "slot": row.get::<String,_>("slot"),
                        "icon_token": row.get::<Option<String>,_>("icon_token"),
                        "presentation_tokens": row.get::<Option<String>,_>("presentation_tokens_json").as_deref().and_then(|s| serde_json::from_str::<Vec<String>>(s).ok()),
                        "asset_attachment_id": row.get::<Option<String>,_>("asset_attachment_id"),
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

#[allow(dead_code)]
async fn rebuild_presentation_sqlite(
    conn: &mut sqlx::SqliteConnection,
    user_id: &str,
    now: i64,
) -> Result<(), ShopError> {
    let rows = sqlx::query(
        "SELECT e.id, p.slot FROM user_entitlements e
         JOIN shop_products p ON p.id = e.product_id
         WHERE e.user_id = ? AND e.status = 'equipped'
           AND (e.expires_at IS NULL OR e.expires_at > ?)",
    )
    .bind(user_id)
    .bind(now)
    .fetch_all(&mut *conn)
    .await?;
    let mut values: [Option<String>; 5] = Default::default();
    let mut badges = Vec::new();
    for row in rows {
        let id: String = row.get("id");
        let slot: String = row.get("slot");
        match slot.as_str() {
            "nickname_color" => values[0] = Some(id),
            "avatar_frame" => values[1] = Some(id),
            "profile_effect" => values[2] = Some(id),
            "post_effect" => values[3] = Some(id),
            "title_prefix" => values[4] = Some(id),
            "profile_badge" | "profile_badges" if badges.len() < 3 => badges.push(id),
            _ => {}
        }
    }
    let badges_json = (!badges.is_empty())
        .then(|| serde_json::to_string(&badges).unwrap_or_else(|_| "[]".into()));
    sqlx::query(
        "INSERT OR IGNORE INTO user_presentations (user_id, version, updated_at, created_at)
         VALUES (?, 1, ?, ?)",
    )
    .bind(user_id)
    .bind(now)
    .bind(now)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "UPDATE user_presentations SET nickname_decoration_id = NULL, nickname_color_id = ?,
         avatar_frame_id = ?, avatar_attachment_id = NULL, profile_effect_id = ?,
         title_prefix_id = ?, profile_badge_ids_json = ?, post_effect_id = ?,
         version = version + 1, updated_at = ? WHERE user_id = ?",
    )
    .bind(&values[0])
    .bind(&values[1])
    .bind(&values[2])
    .bind(&values[4])
    .bind(badges_json)
    .bind(&values[3])
    .bind(now)
    .bind(user_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

#[allow(dead_code)]
async fn rebuild_presentation_mysql(
    conn: &mut sqlx::MySqlConnection,
    user_id: &str,
    now: i64,
) -> Result<(), ShopError> {
    let rows = sqlx::query(
        "SELECT e.id, p.slot FROM user_entitlements e
         JOIN shop_products p ON p.id = e.product_id
         WHERE e.user_id = ? AND e.status = 'equipped'
           AND (e.expires_at IS NULL OR e.expires_at > ?)",
    )
    .bind(user_id)
    .bind(now)
    .fetch_all(&mut *conn)
    .await?;
    let mut values: [Option<String>; 5] = Default::default();
    let mut badges = Vec::new();
    for row in rows {
        let id: String = row.get("id");
        let slot: String = row.get("slot");
        match slot.as_str() {
            "nickname_color" => values[0] = Some(id),
            "avatar_frame" => values[1] = Some(id),
            "profile_effect" => values[2] = Some(id),
            "post_effect" => values[3] = Some(id),
            "title_prefix" => values[4] = Some(id),
            "profile_badge" | "profile_badges" if badges.len() < 3 => badges.push(id),
            _ => {}
        }
    }
    let badges_json = (!badges.is_empty())
        .then(|| serde_json::to_string(&badges).unwrap_or_else(|_| "[]".into()));
    sqlx::query(
        "INSERT IGNORE INTO user_presentations (user_id, version, updated_at, created_at)
         VALUES (?, 1, ?, ?)",
    )
    .bind(user_id)
    .bind(now)
    .bind(now)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "UPDATE user_presentations SET nickname_decoration_id = NULL, nickname_color_id = ?,
         avatar_frame_id = ?, avatar_attachment_id = NULL, profile_effect_id = ?,
         title_prefix_id = ?, profile_badge_ids_json = ?, post_effect_id = ?,
         version = version + 1, updated_at = ? WHERE user_id = ?",
    )
    .bind(&values[0])
    .bind(&values[1])
    .bind(&values[2])
    .bind(&values[4])
    .bind(badges_json)
    .bind(&values[3])
    .bind(now)
    .bind(user_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

#[allow(clippy::explicit_auto_deref)]
async fn rebuild_presentation(
    pool: &DatabasePool,
    user_id: &str,
    now: i64,
) -> Result<(), ShopError> {
    match pool {
        Either::Left(p) => {
            let mut conn = p.acquire().await?;
            rebuild_presentation_sqlite(&mut *conn, user_id, now).await
        }
        Either::Right(p) => {
            let mut conn = p.acquire().await?;
            rebuild_presentation_mysql(&mut *conn, user_id, now).await
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
    equip_with_version(pool, user_id, entitlement_id, None).await
}

pub async fn equip_with_version(
    pool: &DatabasePool,
    user_id: &str,
    entitlement_id: &str,
    expected_version: Option<i64>,
) -> Result<Value, ShopError> {
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            let mut conn = p.acquire().await?;
            sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
            let outcome: Result<Value, ShopError> = async {
                if let Some(expected) = expected_version {
                    let actual: i64 = sqlx::query_scalar("SELECT COALESCE(version, 1) FROM user_presentations WHERE user_id = ?")
                        .bind(user_id).fetch_optional(&mut *conn).await?.unwrap_or(1);
                    if actual != expected { return Err(ShopError::VersionConflict); }
                }
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
                if slot.is_empty() || matches!(kind.as_str(), "reaction_pack" | "utility") {
                    return Err(ShopError::Invalid("this product is a consumable and cannot be equipped".into()));
                }
                if status == "revoked" || status == "consumed" {
                    return Err(ShopError::EntitlementNotOwned);
                }
                if status == "expired" || expires_at.is_some_and(|e| e < now) {
                    return Err(ShopError::EntitlementNotOwned);
                }
                // 徽章 slot 最多 3 个 equipped。
                if slot == "profile_badge" || slot == "profile_badges" {
                    let equipped: i64 = sqlx::query_scalar(
                        "SELECT COUNT(*) FROM user_entitlements WHERE user_id = ? AND status = 'equipped'
                         AND (expires_at IS NULL OR expires_at > ?)
                         AND id IN
                         (SELECT e.id FROM user_entitlements e JOIN shop_products p ON p.id = e.product_id WHERE p.slot IN ('profile_badge', 'profile_badges'))",
                    )
                    .bind(user_id)
                    .bind(now)
                    .fetch_one(&mut *conn)
                    .await?;
                    if equipped >= 3 {
                        return Err(ShopError::SlotConflict);
                    }
                }
                // 普通槽位互斥；徽章槽位允许同时装备最多 3 枚。
                if slot != "profile_badge" && slot != "profile_badges" {
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
                }
                sqlx::query(
                    "UPDATE user_entitlements SET status = 'equipped', equipped_at = ?, updated_at = ? WHERE id = ? AND user_id = ?",
                )
                .bind(now)
                .bind(now)
                .bind(entitlement_id)
                .bind(user_id)
                .execute(&mut *conn)
                .await?;
                rebuild_presentation_sqlite(&mut conn, user_id, now).await?;
                let _ = kind;
                Ok(json!({ "entitlement_id": entitlement_id,
                     "slot": slot, "status": "equipped" }))
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
                if let Some(expected) = expected_version {
                    let actual: i64 = sqlx::query_scalar("SELECT COALESCE(version, 1) FROM user_presentations WHERE user_id = ?")
                        .bind(user_id).fetch_optional(&mut *tx).await?.unwrap_or(1);
                    if actual != expected { return Err(ShopError::VersionConflict); }
                }
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
                let kind: String = row.get("kind");
                if slot.is_empty() || matches!(kind.as_str(), "reaction_pack" | "utility") {
                    return Err(ShopError::Invalid("this product is a consumable and cannot be equipped".into()));
                }
                if status == "revoked" || status == "consumed" {
                    return Err(ShopError::EntitlementNotOwned);
                }
                if status == "expired" || expires_at.is_some_and(|e| e < now) {
                    return Err(ShopError::EntitlementNotOwned);
                }
                if slot == "profile_badge" || slot == "profile_badges" {
                    let equipped: i64 = sqlx::query_scalar(
                        "SELECT COUNT(*) FROM user_entitlements e JOIN shop_products p ON p.id = e.product_id
                         WHERE e.user_id = ? AND e.status = 'equipped'
                           AND (e.expires_at IS NULL OR e.expires_at > ?)
                           AND p.slot IN ('profile_badge', 'profile_badges')",
                    )
                    .bind(user_id)
                    .bind(now)
                    .fetch_one(&mut *tx)
                    .await?;
                    if equipped >= 3 {
                        return Err(ShopError::SlotConflict);
                    }
                }
                if slot != "profile_badge" && slot != "profile_badges" {
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
                }
                sqlx::query(
                    "UPDATE user_entitlements SET status = 'equipped', equipped_at = ?, updated_at = ? WHERE id = ? AND user_id = ?",
                )
                .bind(now)
                .bind(now)
                .bind(entitlement_id)
                .bind(user_id)
                .execute(&mut *tx)
                .await?;
                rebuild_presentation_mysql(&mut tx, user_id, now).await?;
                Ok(json!({ "entitlement_id": entitlement_id,
                     "slot": slot, "status": "equipped" }))
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
    unequip_with_version(pool, user_id, entitlement_id, None).await
}

pub async fn unequip_with_version(
    pool: &DatabasePool,
    user_id: &str,
    entitlement_id: &str,
    expected_version: Option<i64>,
) -> Result<Value, ShopError> {
    let now = now_millis();
    if let Some(expected) = expected_version {
        let actual = get_presentation(pool, user_id)
            .await?
            .get("version")
            .and_then(Value::as_i64)
            .unwrap_or(1);
        if actual != expected {
            return Err(ShopError::VersionConflict);
        }
    }
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
            rebuild_presentation(pool, user_id, now).await?;
            Ok(json!({ "entitlement_id": entitlement_id,
                     "status": "owned" }))
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
            rebuild_presentation(pool, user_id, now).await?;
            Ok(json!({ "entitlement_id": entitlement_id,
                     "status": "owned" }))
        }
    }
}

/// 我的 presentation（只输出后端安全 Token；无权/过期 → 默认展示）。
pub async fn get_presentation(pool: &DatabasePool, user_id: &str) -> Result<Value, ShopError> {
    let now = now_millis();
    let compiled_tokens = get_public_presentation_tokens(pool, user_id).await?;
    match pool {
        Either::Left(p) => {
            let row = sqlx::query(
                "SELECT nickname_color_id, avatar_frame_id, profile_effect_id, profile_badge_ids_json, post_effect_id, title_prefix_id, version
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
                "nickname_color_id": row.as_ref().and_then(|r| r.get::<Option<String>,_>("nickname_color_id")),
                "avatar_frame_id": row.as_ref().and_then(|r| r.get::<Option<String>,_>("avatar_frame_id")),
                "profile_effect_id": row.as_ref().and_then(|r| r.get::<Option<String>,_>("profile_effect_id")),
                "post_effect_id": row.as_ref().and_then(|r| r.get::<Option<String>,_>("post_effect_id")),
                "title_prefix_id": row.as_ref().and_then(|r| r.get::<Option<String>,_>("title_prefix_id")),
                "profile_badge_ids": badges,
                "presentation_tokens": compiled_tokens.clone(),
                "now": now,
            }))
        }
        Either::Right(p) => {
            let row = sqlx::query(
                "SELECT nickname_color_id, avatar_frame_id, profile_effect_id, profile_badge_ids_json, post_effect_id, title_prefix_id, version
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
                "nickname_color_id": row.as_ref().and_then(|r| r.get::<Option<String>,_>("nickname_color_id")),
                "avatar_frame_id": row.as_ref().and_then(|r| r.get::<Option<String>,_>("avatar_frame_id")),
                "profile_effect_id": row.as_ref().and_then(|r| r.get::<Option<String>,_>("profile_effect_id")),
                "post_effect_id": row.as_ref().and_then(|r| r.get::<Option<String>,_>("post_effect_id")),
                "title_prefix_id": row.as_ref().and_then(|r| r.get::<Option<String>,_>("title_prefix_id")),
                "profile_badge_ids": badges,
                "presentation_tokens": compiled_tokens.clone(),
                "now": now,
            }))
        }
    }
}

/// `user_presentations` 装配行（6 个有效槽位列，与 get_public_presentation_tokens 对应）。
type PresentationSlotRow = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// 公开装扮投影（M07-SHOP-SCHEMA-06）：衣柜装配 → 白名单 Token 投影。
///
/// 语义约定：
/// - 只读 `user_presentations` 当前装配（过期/卸下由 equip/unequip 流程
///   裁决，本投影不做权益状态机裁决）；
/// - 商品仅取 `status='published'`；`presentation_tokens` 取首个匹配槽位
///   前缀的注册 Token 并剥离前缀（`avatar.frame.gold_ring` → `gold_ring`）；
/// - 未装配 / 商品缺失 / Token 不合法 → 该槽位跳过；全部为空返回 None；
/// - 封禁/注销中的整体置空由调用方（users::get_public_user）负责。
pub async fn get_public_presentation_tokens(
    pool: &DatabasePool,
    user_id: &str,
) -> Result<Option<crate::users::dto::PublicPresentationTokens>, ShopError> {
    let row: Option<PresentationSlotRow> = match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT nickname_color_id, avatar_frame_id, profile_effect_id, profile_badge_ids_json, post_effect_id, title_prefix_id \
             FROM user_presentations WHERE user_id = ?",
        )
        .bind(user_id)
        .fetch_optional(p)
        .await?,
        Either::Right(p) => sqlx::query_as(
            "SELECT nickname_color_id, avatar_frame_id, profile_effect_id, profile_badge_ids_json, post_effect_id, title_prefix_id \
             FROM user_presentations WHERE user_id = ?",
        )
        .bind(user_id)
        .fetch_optional(p)
        .await?,
    };
    let Some((
        nickname_color_id,
        avatar_frame_id,
        profile_effect_id,
        profile_badge_ids_json,
        post_effect_id,
        title_prefix_id,
    )) = row
    else {
        return Ok(None);
    };

    let mut tokens = crate::users::dto::PublicPresentationTokens::default();

    if let Some(id) = nickname_color_id.as_deref() {
        if let Some((product_id, _)) =
            active_equipped_product(pool, user_id, id, "nickname_color").await?
        {
            if let Some(v) = first_public_token(pool, &product_id, "nickname.color.").await? {
                if is_registered_nickname_color(&v) {
                    tokens.nickname_color = Some(v);
                } else if let Some((kind, name, style)) =
                    super::cosmetics::resolve_def(pool, &v).await?
                {
                    if kind == "nickname_color" {
                        tokens.nickname_color = Some(v);
                        tokens.nickname_color_name = Some(name);
                        tokens.nickname_color_style = Some(style);
                    }
                }
                // 其余取值：未注册且无样式库定义 → 该槽位跳过（不渲染）。
            }
        }
    }
    if let Some(id) = avatar_frame_id.as_deref() {
        if let Some((product_id, asset_id)) =
            active_equipped_product(pool, user_id, id, "avatar_frame").await?
        {
            if let Some(v) = first_public_token(pool, &product_id, "avatar.frame.").await? {
                if let Some((kind, name, style)) = super::cosmetics::resolve_def(pool, &v).await? {
                    if kind == "avatar_frame" {
                        tokens.avatar_frame = Some(v);
                        tokens.avatar_frame_name = Some(name);
                        tokens.avatar_frame_style = Some(style);
                    }
                } else {
                    // 解析不到自定义样式时保留注册枚举值（例如 c 开头的
                    // cloud_blade/cyan_fire 等内置动效头像框），前端再做白名单渲染。
                    tokens.avatar_frame = Some(v);
                }
            }
            tokens.avatar_frame_attachment_id = asset_id;
        }
    }
    if let Some(id) = profile_effect_id.as_deref() {
        if let Some((product_id, _)) =
            active_equipped_product(pool, user_id, id, "profile_effect").await?
        {
            if let Some(v) = first_public_token(pool, &product_id, "profile.effect.").await? {
                if let Some((kind, name, style)) = super::cosmetics::resolve_def(pool, &v).await? {
                    if kind == "profile_effect" {
                        tokens.profile_effect = Some(v);
                        tokens.profile_effect_name = Some(name);
                        tokens.profile_effect_style = Some(style);
                    }
                } else {
                    tokens.profile_effect = Some(v);
                }
            }
        }
    }
    if let Some(id) = post_effect_id.as_deref() {
        if let Some((product_id, _)) =
            active_equipped_product(pool, user_id, id, "post_effect").await?
        {
            if let Some(v) = first_public_token(pool, &product_id, "post.effect.").await? {
                if let Some((kind, name, style)) = super::cosmetics::resolve_def(pool, &v).await? {
                    if kind == "post_effect" {
                        tokens.post_effect = Some(v);
                        tokens.post_effect_name = Some(name);
                        tokens.post_effect_style = Some(style);
                    }
                } else {
                    tokens.post_effect = Some(v);
                }
            }
        }
    }
    if let Some(id) = title_prefix_id.as_deref() {
        if let Some((product_id, _)) =
            active_equipped_product(pool, user_id, id, "title_prefix").await?
        {
            if let Some(v) = first_public_token(pool, &product_id, "title.prefix.").await? {
                if let Some((kind, name, style)) = super::cosmetics::resolve_def(pool, &v).await? {
                    if kind == "title_prefix" {
                        tokens.title_prefix = Some(v);
                        tokens.title_prefix_name = Some(name);
                        tokens.title_prefix_style = Some(style);
                    }
                } else {
                    tokens.title_prefix = Some(v);
                }
            }
        }
    }
    // 佩戴徽章（≤3，与 equip 流程的上限一致）
    if let Some(json_str) = profile_badge_ids_json {
        if let Ok(ids) = serde_json::from_str::<Vec<String>>(&json_str) {
            let mut arr = Vec::new();
            let mut names = Vec::new();
            let mut styles = Vec::new();
            for id in ids.iter().take(3) {
                if let Some((product_id, _)) =
                    active_equipped_product(pool, user_id, id, "profile_badges").await?
                {
                    if let Some(v) = first_public_token(pool, &product_id, "badge.").await? {
                        if let Some((kind, name, style)) =
                            super::cosmetics::resolve_def(pool, &v).await?
                        {
                            if kind == "cosmetic_badge" {
                                arr.push(v);
                                names.push(name);
                                styles.push(style);
                            }
                        } else {
                            arr.push(v);
                        }
                    }
                }
            }
            if !arr.is_empty() {
                tokens.profile_badges = Some(arr);
                if !names.is_empty() {
                    tokens.profile_badge_names = Some(names);
                    tokens.profile_badge_styles = Some(styles);
                }
            }
        }
    }

    if tokens.is_empty() {
        Ok(None)
    } else {
        Ok(Some(tokens))
    }
}

/// 校验 user_presentations 中的 entitlement 仍归属于本人、已装备、未过期，
/// 并返回商品 ID 与可用 PNG 资源 ID。公开投影不信任物化槽位中的孤立 ID。
async fn active_equipped_product(
    pool: &DatabasePool,
    user_id: &str,
    entitlement_id: &str,
    slot: &str,
) -> Result<Option<(String, Option<String>)>, ShopError> {
    let now = now_millis();
    match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT e.product_id, a.id
             FROM user_entitlements e JOIN shop_products p ON p.id = e.product_id
             LEFT JOIN attachments a ON a.id = p.asset_attachment_id
               AND a.status = 'ready' AND a.media_type = 'image/png' AND a.is_public = 1
             WHERE e.id = ? AND e.user_id = ? AND e.status = 'equipped'
               AND (e.expires_at IS NULL OR e.expires_at > ?)
               AND (p.slot = ? OR (? = 'profile_badges' AND p.slot = 'profile_badge'))
               AND p.status = 'published'",
        )
        .bind(entitlement_id)
        .bind(user_id)
        .bind(now)
        .bind(slot)
        .bind(slot)
        .fetch_optional(p)
        .await
        .map_err(ShopError::from),
        Either::Right(p) => sqlx::query_as(
            "SELECT e.product_id, a.id
             FROM user_entitlements e JOIN shop_products p ON p.id = e.product_id
             LEFT JOIN attachments a ON a.id = p.asset_attachment_id
               AND a.status = 'ready' AND a.media_type = 'image/png' AND a.is_public = 1
             WHERE e.id = ? AND e.user_id = ? AND e.status = 'equipped'
               AND (e.expires_at IS NULL OR e.expires_at > ?)
               AND (p.slot = ? OR (? = 'profile_badges' AND p.slot = 'profile_badge'))
               AND p.status = 'published'",
        )
        .bind(entitlement_id)
        .bind(user_id)
        .bind(now)
        .bind(slot)
        .bind(slot)
        .fetch_optional(p)
        .await
        .map_err(ShopError::from),
    }
}

/// 取商品（published）的 presentation_tokens 中首个匹配槽位前缀的 Token，
/// 剥离前缀返回裸值；商品缺失/未发布/解析失败/无匹配 → None。
async fn first_public_token(
    pool: &DatabasePool,
    product_id: &str,
    prefix: &str,
) -> Result<Option<String>, ShopError> {
    let json_str: Option<Option<String>> = match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT presentation_tokens_json FROM shop_products WHERE id = ? AND status = 'published'",
        )
        .bind(product_id)
        .fetch_optional(p)
        .await?,
        Either::Right(p) => sqlx::query_scalar(
            "SELECT presentation_tokens_json FROM shop_products WHERE id = ? AND status = 'published'",
        )
        .bind(product_id)
        .fetch_optional(p)
        .await?,
    };
    let Some(json_str) = json_str else {
        return Ok(None);
    };
    let Some(json_str) = json_str else {
        return Ok(None);
    };
    let Ok(tokens) = serde_json::from_str::<Vec<String>>(&json_str) else {
        return Ok(None);
    };
    Ok(tokens
        .into_iter()
        .find(|t| t.len() > prefix.len() && t.starts_with(prefix))
        .map(|t| t[prefix.len()..].to_string()))
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
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
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
    // M07-SHOP-UI-10：nickname.color.* 取值须为注册色或 active 样式库定义。
    super::cosmetics::validate_token_def_references(pool, presentation_tokens.as_deref()).await?;
    validate_slot_for_kind(kind, &slot)?;
    if let Some(asset_attachment_id) = input.get("asset_attachment_id").and_then(|v| v.as_str()) {
        validate_asset_kind_slot(kind, &slot)?;
        validate_asset_attachment(pool, asset_attachment_id).await?;
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
                     (id, kind, status, slug, title, description_safe, icon_token, presentation_tokens_json, asset_attachment_id, slot, currency_id, unit_price, quantity_limit, stock_remaining, required_level, validity_seconds, sale_start_at, sale_end_at, refund_policy, version, created_by, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?)",
            )
            .bind(&id)
            .bind(kind)
            .bind(status)
            .bind(slug)
            .bind(title)
            .bind(input.get("description_safe").and_then(|v| v.as_str()))
            .bind(icon_token)
            .bind(presentation_tokens.as_deref())
             .bind(input.get("asset_attachment_id").and_then(|v| v.as_str()))
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
                     (id, kind, status, slug, title, description_safe, icon_token, presentation_tokens_json, asset_attachment_id, slot, currency_id, unit_price, quantity_limit, stock_remaining, required_level, validity_seconds, sale_start_at, sale_end_at, refund_policy, version, created_by, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?)",
            )
            .bind(&id)
            .bind(kind)
            .bind(status)
            .bind(slug)
            .bind(title)
            .bind(input.get("description_safe").and_then(|v| v.as_str()))
            .bind(icon_token)
            .bind(presentation_tokens.as_deref())
             .bind(input.get("asset_attachment_id").and_then(|v| v.as_str()))
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
    // M07-SHOP-UI-10：nickname.color.* 取值须为注册色或 active 样式库定义。
    super::cosmetics::validate_token_def_references(pool, presentation_tokens.as_deref()).await?;
    if let Some(slot) = input.get("slot").and_then(|v| v.as_str()) {
        if slot.is_empty() {
            // 清空槽位仅对免槽位类型开放（装备类商品必须有槽位，供 equip 查询）。
            let current: Option<(String,)> = match pool {
                Either::Left(p) => {
                    sqlx::query_as("SELECT kind FROM shop_products WHERE id = ?")
                        .bind(id)
                        .fetch_optional(p)
                        .await?
                }
                Either::Right(p) => {
                    sqlx::query_as("SELECT kind FROM shop_products WHERE id = ?")
                        .bind(id)
                        .fetch_optional(p)
                        .await?
                }
            };
            let Some((kind,)) = current else {
                return Err(ShopError::NotFound(format!("product {id}")));
            };
            if !SLOT_OPTIONAL_KINDS.contains(&kind.as_str()) {
                return Err(ShopError::Invalid(
                    "cannot clear slot for equipped product kinds".into(),
                ));
            }
        } else if !is_valid_slot(slot) {
            return Err(ShopError::Invalid("invalid presentation slot".into()));
        }
    }
    if let Some(asset_attachment_id) = input.get("asset_attachment_id").and_then(|v| v.as_str()) {
        let current: Option<(String, String)> = match pool {
            Either::Left(p) => {
                sqlx::query_as("SELECT kind, slot FROM shop_products WHERE id = ?")
                    .bind(id)
                    .fetch_optional(p)
                    .await?
            }
            Either::Right(p) => {
                sqlx::query_as("SELECT kind, slot FROM shop_products WHERE id = ?")
                    .bind(id)
                    .fetch_optional(p)
                    .await?
            }
        };
        let Some((kind, current_slot)) = current else {
            return Err(ShopError::NotFound(format!("product {id}")));
        };
        validate_asset_kind_slot(
            &kind,
            input
                .get("slot")
                .and_then(|v| v.as_str())
                .unwrap_or(&current_slot),
        )?;
        validate_asset_attachment(pool, asset_attachment_id).await?;
    }
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
                     asset_attachment_id = COALESCE(?, asset_attachment_id),
                     slot = COALESCE(?, slot),
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
            .bind(input.get("asset_attachment_id").and_then(|v| v.as_str()))
            .bind(input.get("slot").and_then(|v| v.as_str()))
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
                     asset_attachment_id = COALESCE(?, asset_attachment_id),
                     slot = COALESCE(?, slot),
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
            .bind(input.get("asset_attachment_id").and_then(|v| v.as_str()))
            .bind(input.get("slot").and_then(|v| v.as_str()))
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
    if status == "published" {
        let info: Option<(String, String, Option<String>)> = match pool {
            Either::Left(p) => {
                sqlx::query_as(
                    "SELECT kind, slot, asset_attachment_id FROM shop_products WHERE id = ?",
                )
                .bind(id)
                .fetch_optional(p)
                .await?
            }
            Either::Right(p) => {
                sqlx::query_as(
                    "SELECT kind, slot, asset_attachment_id FROM shop_products WHERE id = ?",
                )
                .bind(id)
                .fetch_optional(p)
                .await?
            }
        };
        let Some((kind, slot, asset_attachment_id)) = info else {
            return Err(ShopError::NotFound(format!("product {id}")));
        };
        if let Some(asset_id) = asset_attachment_id {
            validate_asset_kind_slot(&kind, &slot)?;
            validate_asset_attachment(pool, &asset_id).await?;
        }
    }
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
                "SELECT o.id, o.user_id, o.product_id, o.product_version, o.quantity, o.currency_id, o.unit_price, o.total_amount, o.point_operation_id, o.status, o.idempotency_key, o.created_at, p.title AS product_title, c.code AS currency_code, c.name AS currency_name
                 FROM shop_orders o LEFT JOIN shop_products p ON p.id = o.product_id
                 LEFT JOIN currencies c ON c.id = o.currency_id
                 ORDER BY o.created_at DESC",
            )
            .fetch_all(p)
            .await?;
            let items: Vec<Value> = rows
                .iter()
                .map(|row| {
                    let mut v = order_json(&row_to_order(row));
                    if let Some(obj) = v.as_object_mut() {
                        obj.insert("user_id".into(), json!(row.get::<String, _>("user_id")));
                        obj.insert(
                            "product_title".into(),
                            json!(row.get::<Option<String>, _>("product_title")),
                        );
                        obj.insert(
                            "currency_code".into(),
                            json!(row.get::<Option<String>, _>("currency_code")),
                        );
                        obj.insert(
                            "currency_name".into(),
                            json!(row.get::<Option<String>, _>("currency_name")),
                        );
                    }
                    v
                })
                .collect();
            Ok(json!({ "orders": items }))
        }
        Either::Right(p) => {
            let rows = sqlx::query(
                "SELECT o.id, o.user_id, o.product_id, o.product_version, o.quantity, o.currency_id, o.unit_price, o.total_amount, o.point_operation_id, o.status, o.idempotency_key, o.created_at, p.title AS product_title, c.code AS currency_code, c.name AS currency_name
                 FROM shop_orders o LEFT JOIN shop_products p ON p.id = o.product_id
                 LEFT JOIN currencies c ON c.id = o.currency_id
                 ORDER BY o.created_at DESC",
            )
            .fetch_all(p)
            .await?;
            let items: Vec<Value> = rows
                .iter()
                .map(|row| {
                    let mut v = order_json(&row_to_order_mysql(row));
                    if let Some(obj) = v.as_object_mut() {
                        obj.insert("user_id".into(), json!(row.get::<String, _>("user_id")));
                        obj.insert(
                            "product_title".into(),
                            json!(row.get::<Option<String>, _>("product_title")),
                        );
                        obj.insert(
                            "currency_code".into(),
                            json!(row.get::<Option<String>, _>("currency_code")),
                        );
                        obj.insert(
                            "currency_name".into(),
                            json!(row.get::<Option<String>, _>("currency_name")),
                        );
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
///
/// M16-HARNESS-04：按 `ShopError::code()` 输出稳定 Problem code（与
/// docs/ERROR-CODES.md / OpenAPI Problem.code enum 一致）。
pub fn shop_error_to_app(e: ShopError, request_id: &str) -> AppError {
    use axum::http::StatusCode;
    let (status, title) = match &e {
        ShopError::Db(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error"),
        ShopError::NotFound(_) => (StatusCode::NOT_FOUND, "Not Found"),
        ShopError::Invalid(_) | ShopError::BelowLevel { .. } => {
            (StatusCode::BAD_REQUEST, "Bad Request")
        }
        ShopError::InsufficientBalance
        | ShopError::OutOfStock
        | ShopError::NotInSaleWindow
        | ShopError::PurchaseLimitExceeded
        | ShopError::IdempotencyConflict
        | ShopError::VersionConflict
        | ShopError::EntitlementNotOwned
        | ShopError::SlotConflict
        | ShopError::NotRefundable => (StatusCode::CONFLICT, "Conflict"),
        ShopError::Forbidden(_) => (StatusCode::FORBIDDEN, "Forbidden"),
    };
    let code = e.code();
    let detail = e.to_string();
    AppError::with_code(status, code, title, detail, request_id)
}
