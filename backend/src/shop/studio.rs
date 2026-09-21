//! 装扮工作台复合发布（M07-SHOP-STUDIO）：样式定义 upsert + 商品创建单事务完成。
//!
//! 安全模型与既有端点一致（docs/INTERNAL-MARKETPLACE.md §9）：
//! - style 只接受 `cosmetics::validate_style` schema 校验过的结构化字段，
//!   不存在任意 CSS 通道；
//! - 商品 presentation_tokens 由服务端按定义 id 生成（`<prefix><def_id>`），
//!   仍过 `service::validate_tokens` 白名单；
//! - PNG/APNG 头像框走既有附件校验（ready + public PNG）。
//!
//! 复合语义：
//! - 单事务：def upsert 与商品 INSERT 同生共死，审计同事务写入；
//! - `client_request_id`（可选）提供 24h 幂等：同 key+摘要重放返回首次
//!   结果，不同摘要稳定 409（idempotency_records，M01-AUDIT-04）；
//! - slug 缺省时由标题派生（ASCII slugify，冲突追加序号/随机后缀）。

use serde_json::{json, Value};
use sqlx::Either;

use crate::audit::AuditEntry;
use crate::authz::decision::AUTHZ_POLICY_VERSION;
use crate::db::DatabasePool;
use crate::idempotency::{
    begin_or_replay, complete, mark_failed, request_hash, FailureCachePolicy, IdempotencyKey,
    IdempotencyOutcome,
};
use crate::outbox::now_millis;

use super::cosmetics;
use super::service::{self, ShopError};

/// 幂等 scope（idempotency_records.scope ≤ 50 字符）。
const IDEMPOTENCY_SCOPE: &str = "shop_studio_publish";
/// 幂等记录保留窗口（与 checkout 一致的 24h）。
const IDEMPOTENCY_TTL_MS: i64 = 24 * 60 * 60 * 1000;

/// 装扮类型 →（商品 kind, Token 前缀, 展示槽位）。
///
/// 与 INTERNAL-MARKETPLACE §3 槽位规则一致：reaction_pack/utility 免槽位
/// （service::SLOT_OPTIONAL_KINDS），title_prefix 用同名槽位。
fn kind_spec(kind: &str) -> Result<(&'static str, &'static str, &'static str), ShopError> {
    Ok(match kind {
        "nickname_color" => ("cosmetic_nickname", "nickname.color.", "nickname_color"),
        "avatar_frame" => ("cosmetic_avatar", "avatar.frame.", "avatar_frame"),
        "profile_effect" => ("profile_effect", "profile.effect.", "profile_effect"),
        other => {
            return Err(ShopError::Invalid(format!(
                "kind {other} is not supported; shop only provides nickname_color, avatar_frame, and profile_effect"
            )))
        }
    })
}

/// ASCII slugify：小写字母/数字保留，其余折叠为单个连字符；空结果回退 "cosmetic"。
fn slugify(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut pending_dash = false;
    for c in title.chars() {
        if c.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(c.to_ascii_lowercase());
        } else {
            pending_dash = true;
        }
    }
    let mut out: String = out.chars().take(48).collect();
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "cosmetic".to_string()
    } else {
        out
    }
}

/// 商品 slug 形状校验（与前端 pattern `[a-z0-9-]+` 对齐，首尾不允许连字符）。
fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 64
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !slug.starts_with('-')
        && !slug.ends_with('-')
}

/// 解析并校验 cosmetic 段（def 模式：name/style 必填并过 schema；
/// PNG 模式：跳过 def 字段，禁止带 def id）。
struct CosmeticInput {
    id: Option<String>,
    kind: String,
    name: Option<String>,
    style: Option<Value>,
}

struct ProductInput {
    title: String,
    slug: Option<String>,
    unit_price: i64,
    stock_remaining: Option<i64>,
    required_level: i64,
    quantity_limit: i64,
    validity_seconds: Option<i64>,
    sale_start_at: Option<i64>,
    sale_end_at: Option<i64>,
    refund_policy: String,
    status: String,
    description_safe: Option<String>,
    asset_attachment_id: Option<String>,
    currency_id: Option<String>,
}

/// product.asset_attachment_id 非空即 PNG/APNG 图片头像框模式。
fn png_mode_of(body: &Value) -> bool {
    body.get("product")
        .and_then(|p| p.get("asset_attachment_id"))
        .and_then(Value::as_str)
        .is_some_and(|s| !s.is_empty())
}

fn parse_cosmetic(body: &Value, png_mode: bool) -> Result<CosmeticInput, ShopError> {
    let c = body
        .get("cosmetic")
        .ok_or_else(|| ShopError::Invalid("cosmetic required".into()))?;
    let kind = c
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| ShopError::Invalid("cosmetic.kind required".into()))?;
    if !cosmetics::CUSTOMIZABLE_KINDS.contains(&kind) {
        return Err(ShopError::Invalid(format!(
            "kind {kind} does not support custom styles yet"
        )));
    }
    let id = match c.get("id") {
        Some(Value::String(s)) if !s.is_empty() => {
            if !cosmetics::valid_def_id(s) {
                return Err(ShopError::Invalid("invalid cosmetic def id".into()));
            }
            Some(s.clone())
        }
        _ => None,
    };
    if png_mode {
        if id.is_some() {
            return Err(ShopError::Invalid(
                "PNG avatar frame mode does not update a cosmetic def".into(),
            ));
        }
        return Ok(CosmeticInput {
            id: None,
            kind: kind.to_string(),
            name: None,
            style: None,
        });
    }
    let name = cosmetics::validate_def_name(c.get("name").and_then(Value::as_str).unwrap_or(""))?;
    let style = cosmetics::validate_style(kind, c.get("style").unwrap_or(&Value::Null))?;
    Ok(CosmeticInput {
        id,
        kind: kind.to_string(),
        name: Some(name),
        style: Some(style),
    })
}

fn parse_product(body: &Value) -> Result<ProductInput, ShopError> {
    let p = body
        .get("product")
        .ok_or_else(|| ShopError::Invalid("product required".into()))?;
    let title = p
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    // shop_products.title VARCHAR(120)。
    if title.is_empty() || title.chars().count() > 120 {
        return Err(ShopError::Invalid(
            "product.title must be 1..=120 chars".into(),
        ));
    }
    let unit_price = p
        .get("unit_price")
        .and_then(Value::as_i64)
        .ok_or_else(|| ShopError::Invalid("product.unit_price required".into()))?;
    if unit_price < 0 {
        return Err(ShopError::Invalid("product.unit_price must be >= 0".into()));
    }
    let slug = match p.get("slug").and_then(Value::as_str) {
        Some(s) if !s.trim().is_empty() => {
            let s = s.trim().to_ascii_lowercase();
            if !valid_slug(&s) {
                return Err(ShopError::Invalid(
                    "product.slug must match [a-z0-9-]+ without leading/trailing dash".into(),
                ));
            }
            Some(s)
        }
        _ => None,
    };
    let stock_remaining = p.get("stock_remaining").and_then(Value::as_i64);
    if stock_remaining.is_some_and(|v| v < 0) {
        return Err(ShopError::Invalid(
            "product.stock_remaining must be >= 0".into(),
        ));
    }
    let validity_seconds = p.get("validity_seconds").and_then(Value::as_i64);
    if validity_seconds.is_some_and(|v| v < 0) {
        return Err(ShopError::Invalid(
            "product.validity_seconds must be >= 0".into(),
        ));
    }
    let sale_start_at = p.get("sale_start_at").and_then(Value::as_i64);
    let sale_end_at = p.get("sale_end_at").and_then(Value::as_i64);
    if let (Some(start), Some(end)) = (sale_start_at, sale_end_at) {
        if start > end {
            return Err(ShopError::Invalid(
                "product.sale_start_at must be <= sale_end_at".into(),
            ));
        }
    }
    let refund_policy = p
        .get("refund_policy")
        .and_then(Value::as_str)
        .unwrap_or("non_refundable");
    if !["non_refundable", "compensation_only", "full_refund"].contains(&refund_policy) {
        return Err(ShopError::Invalid("invalid refund_policy".into()));
    }
    let status = p
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("published");
    if !["draft", "published"].contains(&status) {
        return Err(ShopError::Invalid(
            "studio publish status must be draft|published".into(),
        ));
    }
    let quantity_limit = p
        .get("quantity_limit")
        .and_then(Value::as_i64)
        .unwrap_or(1)
        .max(1);
    let required_level = p
        .get("required_level")
        .and_then(Value::as_i64)
        .unwrap_or(1)
        .max(1);
    let asset_attachment_id = match p.get("asset_attachment_id").and_then(Value::as_str) {
        Some(s) if !s.is_empty() => Some(s.to_string()),
        _ => None,
    };
    Ok(ProductInput {
        title: title.to_string(),
        slug,
        unit_price,
        stock_remaining,
        required_level,
        quantity_limit,
        validity_seconds,
        sale_start_at,
        sale_end_at,
        refund_policy: refund_policy.to_string(),
        status: status.to_string(),
        description_safe: p
            .get("description_safe")
            .and_then(Value::as_str)
            .map(str::to_string),
        asset_attachment_id,
        currency_id: p
            .get("currency_id")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

fn is_unique_violation(err: &sqlx::Error) -> bool {
    matches!(err, sqlx::Error::Database(db) if db.is_unique_violation())
}

/// 结算货币解析：接受货币 code（缺省 `coin`）或 id，落为 `currencies.id`
/// （shop_products.currency_id 的 FK 目标；历史表单直传 'coin' 会触发 FK 失败）。
async fn resolve_currency_id(
    pool: &DatabasePool,
    input: Option<&str>,
) -> Result<String, ShopError> {
    let key = input.unwrap_or("coin");
    let row: Option<(String,)> = match pool {
        Either::Left(p) => {
            sqlx::query_as(
                "SELECT id FROM currencies WHERE (code = ? OR id = ?) AND is_enabled = 1",
            )
            .bind(key)
            .bind(key)
            .fetch_optional(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_as(
                "SELECT id FROM currencies WHERE (code = ? OR id = ?) AND is_enabled = 1",
            )
            .bind(key)
            .bind(key)
            .fetch_optional(p)
            .await?
        }
    };
    row.map(|(id,)| id)
        .ok_or_else(|| ShopError::Invalid(format!("unknown or disabled currency: {key}")))
}

/// 工作台复合发布入口（reason 审计；幂等键可选）。
///
/// `storage`：上架驱动 Steam 素材入库（M07-SHOP-ASSETS）。传入存储服务时，
/// Steam 头像框/背景样式引用的文件先确保写入配置的存储后端，再把样式 URL
/// 统一改写为 `/api/v1/steam-assets/...` 稳定路由；传 None 仅改写 URL。
pub async fn publish(
    pool: &DatabasePool,
    body: &Value,
    user_id: &str,
    storage: Option<&crate::storage::StorageService>,
) -> Result<Value, ShopError> {
    let reason = body
        .get("reason")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("studio publish")
        .to_string();
    let png_mode = png_mode_of(body);
    let mut cosmetic = parse_cosmetic(body, png_mode)?;
    if let Some(style) = cosmetic.style.as_mut() {
        crate::shop::steam_assets::apply_to_cosmetic_style(storage, &cosmetic.kind, style).await;
    }
    let product = parse_product(body)?;
    let (product_kind, token_prefix, slot) = kind_spec(&cosmetic.kind)?;
    service::validate_slot_for_kind(product_kind, slot)?;
    if png_mode {
        if cosmetic.kind != "avatar_frame" {
            return Err(ShopError::Invalid(
                "asset attachments are only supported for avatar frame designs".into(),
            ));
        }
        service::validate_asset_kind_slot(product_kind, slot)?;
        service::validate_asset_attachment(
            pool,
            product.asset_attachment_id.as_deref().unwrap_or_default(),
        )
        .await?;
    }
    let currency_id = resolve_currency_id(pool, product.currency_id.as_deref()).await?;

    // ── 幂等（可选）：同 key+摘要重放原结果；不同摘要 409 ──
    let idem_key = match body
        .get("client_request_id")
        .and_then(Value::as_str)
        .map(str::trim)
    {
        Some(s) if !s.is_empty() => Some(
            IdempotencyKey::new(IDEMPOTENCY_SCOPE, s)
                .map_err(|e| ShopError::Invalid(e.to_string()))?,
        ),
        _ => None,
    };
    let mut record_id: Option<String> = None;
    if let Some(key) = &idem_key {
        let canonical = json!({
            "cosmetic": body.get("cosmetic").cloned().unwrap_or(Value::Null),
            "product": body.get("product").cloned().unwrap_or(Value::Null),
        });
        let hash = request_hash(
            serde_json::to_string(&canonical)
                .unwrap_or_default()
                .as_bytes(),
        );
        match begin_or_replay(
            pool,
            key,
            &hash,
            IDEMPOTENCY_TTL_MS,
            FailureCachePolicy::Retry,
        )
        .await
        .map_err(ShopError::from)?
        {
            IdempotencyOutcome::Created { record_id: rid } => record_id = Some(rid),
            IdempotencyOutcome::Replay { response_reference } => {
                return replay_response(pool, response_reference).await;
            }
            // 并发进行中 / 摘要冲突：稳定 409，不重复执行。
            IdempotencyOutcome::InProgress
            | IdempotencyOutcome::Conflict
            | IdempotencyOutcome::Failed { .. } => {
                return Err(ShopError::IdempotencyConflict);
            }
        }
    }

    let outcome = execute_studio_tx(
        pool,
        &cosmetic,
        &product,
        product_kind,
        token_prefix,
        slot,
        &currency_id,
        &reason,
        user_id,
    )
    .await;
    match outcome {
        Ok(v) => {
            if let Some(rid) = record_id {
                let reference = format!(
                    "product:{};cosmetic:{}",
                    v["product"]["id"].as_str().unwrap_or_default(),
                    v["cosmetic"]["id"].as_str().unwrap_or_default()
                );
                complete(pool, &rid, &reference)
                    .await
                    .map_err(ShopError::from)?;
            }
            Ok(v)
        }
        Err(e) => {
            if let Some(rid) = record_id {
                let _ = mark_failed(pool, &rid).await;
            }
            Err(e)
        }
    }
}

/// 幂等重放：按 response_reference 还原首次响应（product 走完整查询）。
async fn replay_response(
    pool: &DatabasePool,
    response_reference: Option<String>,
) -> Result<Value, ShopError> {
    let reference = response_reference.unwrap_or_default();
    let product_id = reference
        .strip_prefix("product:")
        .and_then(|rest| rest.split(';').next())
        .unwrap_or_default();
    if product_id.is_empty() {
        return Err(ShopError::IdempotencyConflict);
    }
    let product = service::get_product(pool, product_id).await?;
    let cosmetic_id = reference
        .split(';')
        .nth(1)
        .and_then(|s| s.strip_prefix("cosmetic:"))
        .unwrap_or_default();
    let cosmetic = if cosmetic_id.is_empty() {
        Value::Null
    } else {
        fetch_def_json(pool, cosmetic_id)
            .await?
            .unwrap_or(Value::Null)
    };
    Ok(json!({
        "cosmetic": cosmetic,
        "product": product,
        "replayed": true,
    }))
}

async fn fetch_def_json(pool: &DatabasePool, id: &str) -> Result<Option<Value>, ShopError> {
    if !cosmetics::valid_def_id(id) {
        return Ok(None);
    }
    type Row = (String, String, String, String, String, i64);
    let row: Option<Row> = match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT id, kind, name, style_json, status, updated_at FROM cosmetic_defs WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(p)
        .await?,
        Either::Right(p) => sqlx::query_as(
            "SELECT id, kind, name, style_json, status, updated_at FROM cosmetic_defs WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(p)
        .await?,
    };
    row.map(|(id, kind, name, style_json, status, updated_at)| {
        let style: Value = serde_json::from_str(&style_json)
            .map_err(|_| ShopError::Invalid("stored style_json is not valid JSON".into()))?;
        Ok(json!({
            "id": id, "kind": kind, "name": name, "style": style,
            "status": status, "updatedAt": updated_at
        }))
    })
    .transpose()
}

/// 事务体：def upsert → token/slug → 商品 INSERT → 同事务审计。
/// SQLite 分支用 `BEGIN IMMEDIATE`（与 buy_product 一致的写事务语义）。
#[allow(clippy::too_many_arguments, clippy::explicit_auto_deref)]
async fn execute_studio_tx(
    pool: &DatabasePool,
    cosmetic: &CosmeticInput,
    product: &ProductInput,
    product_kind: &str,
    token_prefix: &str,
    slot: &str,
    currency_id: &str,
    reason: &str,
    user_id: &str,
) -> Result<Value, ShopError> {
    let now = now_millis();
    let style_json = cosmetic.style.as_ref().map(|s| s.to_string());
    match pool {
        Either::Left(p) => {
            let mut conn = p.acquire().await?;
            sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
            let outcome: Result<Value, ShopError> = async {
                let def_id = upsert_def_sqlite(&mut *conn, cosmetic, style_json.as_deref(), user_id, now).await?;
                // 商品 Token 由服务端按 def id 生成；PNG 模式无 token。
                let tokens_json = def_id
                    .as_ref()
                    .map(|id| json!([format!("{token_prefix}{id}")]).to_string());
                service::validate_tokens(None, tokens_json.as_deref())?;
                let slug = resolve_slug_sqlite(&mut *conn, product).await?;
                let product_id = uuid::Uuid::now_v7().to_string();
                insert_product_sqlite(&mut *conn, &product_id, product_kind, &slug, tokens_json.as_deref(), slot, currency_id, product, user_id, now).await?;
                AuditEntry::user_action(user_id, "shop.studio.publish")
                    .with_target("product", &product_id)
                    .with_reason(reason)
                    .with_metadata(json!({
                        "cosmetic_def_id": def_id,
                        "mode": if product.asset_attachment_id.is_some() { "asset" } else { "style" },
                    }))
                    .with_policy_version(AUTHZ_POLICY_VERSION)
                    .record_into_sqlite(&mut *conn)
                    .await
                    .map_err(ShopError::from)?;
                Ok(studio_response(cosmetic, def_id, product_kind, &product_id, &slug, product, now))
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
                let def_id = upsert_def_mysql(&mut tx, cosmetic, style_json.as_deref(), user_id, now).await?;
                let tokens_json = def_id
                    .as_ref()
                    .map(|id| json!([format!("{token_prefix}{id}")]).to_string());
                service::validate_tokens(None, tokens_json.as_deref())?;
                let slug = resolve_slug_mysql(&mut tx, product).await?;
                let product_id = uuid::Uuid::now_v7().to_string();
                insert_product_mysql(&mut tx, &product_id, product_kind, &slug, tokens_json.as_deref(), slot, currency_id, product, user_id, now).await?;
                AuditEntry::user_action(user_id, "shop.studio.publish")
                    .with_target("product", &product_id)
                    .with_reason(reason)
                    .with_metadata(json!({
                        "cosmetic_def_id": def_id,
                        "mode": if product.asset_attachment_id.is_some() { "asset" } else { "style" },
                    }))
                    .with_policy_version(AUTHZ_POLICY_VERSION)
                    .record_into_mysql(&mut tx)
                    .await
                    .map_err(ShopError::from)?;
                Ok(studio_response(cosmetic, def_id, product_kind, &product_id, &slug, product, now))
            }
            .await;
            match outcome {
                Ok(v) => {
                    tx.commit().await?;
                    Ok(v)
                }
                Err(e) => Err(e),
            }
        }
    }
}

fn studio_response(
    cosmetic: &CosmeticInput,
    def_id: Option<String>,
    product_kind: &str,
    product_id: &str,
    slug: &str,
    product: &ProductInput,
    now: i64,
) -> Value {
    json!({
        "cosmetic": def_id.map(|id| json!({
            "id": id,
            "kind": cosmetic.kind,
            "name": cosmetic.name,
            "style": cosmetic.style,
            "status": "active",
            "updatedAt": now,
        })),
        "product": {
            "id": product_id,
            "kind": product_kind,
            "slug": slug,
            "title": product.title,
            "status": product.status,
            "unit_price": product.unit_price,
            "validity_seconds": product.validity_seconds,
        },
        "replayed": false,
    })
}

// ── def upsert（SQLite/MySQL 双臂；PNG 模式跳过返回 None）──

async fn upsert_def_sqlite(
    conn: &mut sqlx::SqliteConnection,
    cosmetic: &CosmeticInput,
    style_json: Option<&str>,
    user_id: &str,
    now: i64,
) -> Result<Option<String>, ShopError> {
    let (Some(name), Some(style_json)) = (cosmetic.name.as_deref(), style_json) else {
        return Ok(None); // PNG 模式：不写 def。
    };
    if let Some(id) = &cosmetic.id {
        let existing: Option<(String,)> =
            sqlx::query_as("SELECT kind FROM cosmetic_defs WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *conn)
                .await?;
        let Some((existing_kind,)) = existing else {
            return Err(ShopError::NotFound(format!("cosmetic def {id}")));
        };
        if existing_kind != cosmetic.kind {
            return Err(ShopError::Invalid("cosmetic def kind mismatch".into()));
        }
        // 工作台发布即启用：归档定义被引用时恢复 active（历史 Token 引用保持有效）。
        sqlx::query("UPDATE cosmetic_defs SET name = ?, style_json = ?, status = 'active', updated_at = ? WHERE id = ?")
            .bind(name)
            .bind(style_json)
            .bind(now)
            .bind(id)
            .execute(&mut *conn)
            .await?;
        return Ok(Some(id.clone()));
    }
    let id = format!("c{}", uuid::Uuid::now_v7().simple());
    sqlx::query(
        "INSERT INTO cosmetic_defs (id, kind, name, style_json, status, created_by, created_at, updated_at)
         VALUES (?, ?, ?, ?, 'active', ?, ?, ?)",
    )
    .bind(&id)
    .bind(&cosmetic.kind)
    .bind(name)
    .bind(style_json)
    .bind(user_id)
    .bind(now)
    .bind(now)
    .execute(&mut *conn)
    .await?;
    Ok(Some(id))
}

async fn upsert_def_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    cosmetic: &CosmeticInput,
    style_json: Option<&str>,
    user_id: &str,
    now: i64,
) -> Result<Option<String>, ShopError> {
    let (Some(name), Some(style_json)) = (cosmetic.name.as_deref(), style_json) else {
        return Ok(None);
    };
    if let Some(id) = &cosmetic.id {
        let existing: Option<(String,)> =
            sqlx::query_as("SELECT kind FROM cosmetic_defs WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut **tx)
                .await?;
        let Some((existing_kind,)) = existing else {
            return Err(ShopError::NotFound(format!("cosmetic def {id}")));
        };
        if existing_kind != cosmetic.kind {
            return Err(ShopError::Invalid("cosmetic def kind mismatch".into()));
        }
        sqlx::query("UPDATE cosmetic_defs SET name = ?, style_json = ?, status = 'active', updated_at = ? WHERE id = ?")
            .bind(name)
            .bind(style_json)
            .bind(now)
            .bind(id)
            .execute(&mut **tx)
            .await?;
        return Ok(Some(id.clone()));
    }
    let id = format!("c{}", uuid::Uuid::now_v7().simple());
    sqlx::query(
        "INSERT INTO cosmetic_defs (id, kind, name, style_json, status, created_by, created_at, updated_at)
         VALUES (?, ?, ?, ?, 'active', ?, ?, ?)",
    )
    .bind(&id)
    .bind(&cosmetic.kind)
    .bind(name)
    .bind(style_json)
    .bind(user_id)
    .bind(now)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(Some(id))
}

// ── slug 解析（给定校验冲突；缺省由标题派生 + 序号/随机后缀）──

async fn resolve_slug_sqlite(
    conn: &mut sqlx::SqliteConnection,
    product: &ProductInput,
) -> Result<String, ShopError> {
    if let Some(slug) = &product.slug {
        let taken: Option<i64> = sqlx::query_scalar("SELECT 1 FROM shop_products WHERE slug = ?")
            .bind(slug)
            .fetch_optional(&mut *conn)
            .await?;
        if taken.is_some() {
            return Err(ShopError::Invalid(format!("slug {slug} already taken")));
        }
        return Ok(slug.clone());
    }
    let base = slugify(&product.title);
    for attempt in 0..24u32 {
        let candidate = match attempt {
            0 => base.clone(),
            1..=7 => format!("{base}-{}", attempt + 1),
            _ => format!(
                "{}-{}",
                base,
                &uuid::Uuid::now_v7().simple().to_string()[..6]
            ),
        };
        let taken: Option<i64> = sqlx::query_scalar("SELECT 1 FROM shop_products WHERE slug = ?")
            .bind(&candidate)
            .fetch_optional(&mut *conn)
            .await?;
        if taken.is_none() {
            return Ok(candidate);
        }
    }
    Err(ShopError::Invalid("slug unavailable".into()))
}

async fn resolve_slug_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    product: &ProductInput,
) -> Result<String, ShopError> {
    if let Some(slug) = &product.slug {
        let taken: Option<i64> = sqlx::query_scalar("SELECT 1 FROM shop_products WHERE slug = ?")
            .bind(slug)
            .fetch_optional(&mut **tx)
            .await?;
        if taken.is_some() {
            return Err(ShopError::Invalid(format!("slug {slug} already taken")));
        }
        return Ok(slug.clone());
    }
    let base = slugify(&product.title);
    for attempt in 0..24u32 {
        let candidate = match attempt {
            0 => base.clone(),
            1..=7 => format!("{base}-{}", attempt + 1),
            _ => format!(
                "{}-{}",
                base,
                &uuid::Uuid::now_v7().simple().to_string()[..6]
            ),
        };
        let taken: Option<i64> = sqlx::query_scalar("SELECT 1 FROM shop_products WHERE slug = ?")
            .bind(&candidate)
            .fetch_optional(&mut **tx)
            .await?;
        if taken.is_none() {
            return Ok(candidate);
        }
    }
    Err(ShopError::Invalid("slug unavailable".into()))
}

// ── 商品 INSERT（列与 service::create_product 一致；工作台固定 coin 结算）──

#[allow(clippy::too_many_arguments)]
async fn insert_product_sqlite(
    conn: &mut sqlx::SqliteConnection,
    product_id: &str,
    product_kind: &str,
    slug: &str,
    tokens_json: Option<&str>,
    slot: &str,
    currency_id: &str,
    product: &ProductInput,
    user_id: &str,
    now: i64,
) -> Result<(), ShopError> {
    sqlx::query(
        "INSERT INTO shop_products
             (id, kind, status, slug, title, description_safe, icon_token, presentation_tokens_json, asset_attachment_id, slot, currency_id, unit_price, quantity_limit, stock_remaining, required_level, validity_seconds, sale_start_at, sale_end_at, refund_policy, version, created_by, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, NULL, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?)",
    )
    .bind(product_id)
    .bind(product_kind)
    .bind(&product.status)
    .bind(slug)
    .bind(&product.title)
    .bind(product.description_safe.as_deref())
    .bind(tokens_json)
    .bind(product.asset_attachment_id.as_deref())
    .bind(slot)
    .bind(currency_id)
    .bind(product.unit_price)
    .bind(product.quantity_limit)
    .bind(product.stock_remaining)
    .bind(product.required_level)
    .bind(product.validity_seconds)
    .bind(product.sale_start_at)
    .bind(product.sale_end_at)
    .bind(&product.refund_policy)
    .bind(user_id)
    .bind(now)
    .bind(now)
    .execute(&mut *conn)
    .await
    .map_err(|e| {
        if is_unique_violation(&e) {
            ShopError::Invalid(format!("slug {slug} already taken"))
        } else {
            ShopError::from(e)
        }
    })?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_product_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    product_id: &str,
    product_kind: &str,
    slug: &str,
    tokens_json: Option<&str>,
    slot: &str,
    currency_id: &str,
    product: &ProductInput,
    user_id: &str,
    now: i64,
) -> Result<(), ShopError> {
    sqlx::query(
        "INSERT INTO shop_products
             (id, kind, status, slug, title, description_safe, icon_token, presentation_tokens_json, asset_attachment_id, slot, currency_id, unit_price, quantity_limit, stock_remaining, required_level, validity_seconds, sale_start_at, sale_end_at, refund_policy, version, created_by, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, NULL, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?)",
    )
    .bind(product_id)
    .bind(product_kind)
    .bind(&product.status)
    .bind(slug)
    .bind(&product.title)
    .bind(product.description_safe.as_deref())
    .bind(tokens_json)
    .bind(product.asset_attachment_id.as_deref())
    .bind(slot)
    .bind(currency_id)
    .bind(product.unit_price)
    .bind(product.quantity_limit)
    .bind(product.stock_remaining)
    .bind(product.required_level)
    .bind(product.validity_seconds)
    .bind(product.sale_start_at)
    .bind(product.sale_end_at)
    .bind(&product.refund_policy)
    .bind(user_id)
    .bind(now)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(|e| {
        if is_unique_violation(&e) {
            ShopError::Invalid(format!("slug {slug} already taken"))
        } else {
            ShopError::from(e)
        }
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_ascii_and_fallback() {
        assert_eq!(slugify("Neon Nick"), "neon-nick");
        assert_eq!(slugify("  Spooky  Glow!! "), "spooky-glow");
        assert_eq!(slugify("樱花粉渐变"), "cosmetic");
        assert_eq!(slugify("a".repeat(80).as_str()), "a".repeat(48));
        assert_eq!(slugify("Mix 中文 Ab12"), "mix-ab12");
    }

    #[test]
    fn valid_slug_shape() {
        assert!(valid_slug("sakura-nick"));
        assert!(valid_slug("a1"));
        assert!(!valid_slug("-lead"));
        assert!(!valid_slug("trail-"));
        assert!(!valid_slug("Upper"));
        assert!(!valid_slug("has space"));
        assert!(!valid_slug(""));
    }

    #[test]
    fn kind_spec_maps_all_customizable_kinds() {
        for kind in cosmetics::CUSTOMIZABLE_KINDS {
            let (product_kind, prefix, _slot) = kind_spec(kind).unwrap();
            assert!(!product_kind.is_empty());
            assert!(prefix.ends_with('.') || prefix.ends_with("..") == false);
        }
        assert!(kind_spec("unknown").is_err());
    }

    #[test]
    fn parse_product_validates_numbers_and_enums() {
        let base = json!({
            "product": { "title": "T", "unit_price": 0 }
        });
        let p = parse_product(&base).unwrap();
        assert_eq!(p.status, "published");
        assert_eq!(p.refund_policy, "non_refundable");

        for bad in [
            json!({"product": {"title": "", "unit_price": 1}}),
            json!({"product": {"title": "T", "unit_price": -1}}),
            json!({"product": {"title": "T", "unit_price": 1, "refund_policy": "refundable"}}),
            json!({"product": {"title": "T", "unit_price": 1, "status": "disabled"}}),
            json!({"product": {"title": "T", "unit_price": 1, "slug": "-bad"}}),
            json!({"product": {"title": "T", "unit_price": 1, "sale_start_at": 2, "sale_end_at": 1}}),
        ] {
            assert!(parse_product(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn parse_cosmetic_requires_name_style_unless_png() {
        let body = json!({
            "cosmetic": { "kind": "nickname_color", "name": "粉", "style": {"mode": "solid", "color": "#f472b6"} },
            "product": { "title": "T", "unit_price": 1 }
        });
        let c = parse_cosmetic(&body, false).unwrap();
        assert_eq!(c.name.as_deref(), Some("粉"));

        // PNG 模式不要求 name/style，但拒绝带 def id。
        let png = json!({"cosmetic": {"kind": "avatar_frame"}});
        assert!(parse_cosmetic(&png, true).is_ok());
        let png_with_id =
            json!({"cosmetic": {"kind": "avatar_frame", "id": "cabc1234567890abcdef"}});
        assert!(parse_cosmetic(&png_with_id, true).is_err());

        // def 模式缺 name / style 非法。
        assert!(parse_cosmetic(&json!({"cosmetic": {"kind": "nickname_color", "style": {"mode": "solid", "color": "#ffffff"}}}), false).is_err());
        assert!(parse_cosmetic(&json!({"cosmetic": {"kind": "nickname_color", "name": "x", "style": {"mode": "solid", "color": "red"}}}), false).is_err());
    }
}
