use axum::{
    extract::{Path, State},
    response::Json,
    routing::{get, patch, post},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Either;

use crate::app::AppState;
use crate::audit::AuditEntry;
use crate::auth::session::AuthSession;
use crate::authz::decision::AUTHZ_POLICY_VERSION;
use crate::authz::enforce::authorize_action;
use crate::error::AppError;
use crate::shop::service::{shop_error_to_app, ShopError};

/// M07-SHOP 管理路由（admin_shop 域 agent 填充）。
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/admin/shop/config",
            get(get_shop_config).patch(update_shop_config),
        )
        .route(
            "/api/v1/admin/shop/products",
            get(list_admin_products).post(create_admin_product),
        )
        .route(
            "/api/v1/admin/shop/products/{id}",
            patch(update_admin_product),
        )
        .route(
            "/api/v1/admin/shop/products/{id}/disable",
            post(disable_product),
        )
        .route(
            "/api/v1/admin/shop/products/{id}/publish",
            post(publish_product),
        )
        .route("/api/v1/admin/shop/orders", get(list_admin_orders))
        .route("/api/v1/admin/shop/orders/{id}/refund", post(refund_order))
}

#[derive(Deserialize)]
struct ReasonBody {
    reason: Option<String>,
}

async fn admin_authorize(
    state: &AppState,
    auth: &AuthSession,
    permission: &str,
    request_id: &str,
) -> Result<(), AppError> {
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let decision = authorize_action(pool, &user.id, permission, None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden(
            format!("{permission} required"),
            request_id,
        ));
    }
    Ok(())
}

/// 商城站点配置行（`shop_site_config` 单行表，0073 迁移；P0 整改：
/// 替代此前的固定返回 + PATCH 丢弃）。
#[derive(sqlx::FromRow, Clone, Debug)]
pub(crate) struct ShopSiteConfigRow {
    pub(crate) enabled: i64,
    pub(crate) max_quantity_per_order: i64,
    pub(crate) default_refund_policy: String,
    pub(crate) version: i64,
    pub(crate) updated_at: i64,
}

/// 读取商城站点配置（种子保证 singleton 行存在；缺失回退默认值）。
pub(crate) async fn load_shop_site_config(
    pool: &crate::db::DatabasePool,
) -> Result<ShopSiteConfigRow, sqlx::Error> {
    let sql = "SELECT enabled, max_quantity_per_order, default_refund_policy, version, updated_at
               FROM shop_site_config WHERE id = 'singleton'";
    let row: Option<ShopSiteConfigRow> = match pool {
        Either::Left(p) => sqlx::query_as(sql).fetch_optional(p).await?,
        Either::Right(p) => sqlx::query_as(sql).fetch_optional(p).await?,
    };
    Ok(row.unwrap_or(ShopSiteConfigRow {
        enabled: 1,
        max_quantity_per_order: 100,
        default_refund_policy: "non_refundable".to_string(),
        version: 1,
        updated_at: 0,
    }))
}

fn shop_config_json(r: &ShopSiteConfigRow) -> Value {
    json!({
        "enabled": r.enabled != 0,
        "max_quantity_per_order": r.max_quantity_per_order,
        "default_refund_policy": r.default_refund_policy,
        "version": r.version,
        "updated_at": r.updated_at,
    })
}

async fn get_shop_config(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_shop_config";
    admin_authorize(&state, &auth, "shop.manage", request_id).await?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let row = load_shop_site_config(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    Ok(Json(shop_config_json(&row)))
}

#[derive(Deserialize)]
struct ShopConfigBody {
    enabled: Option<bool>,
    max_quantity_per_order: Option<i64>,
    default_refund_policy: Option<String>,
    reason: Option<String>,
}

async fn update_shop_config(
    State(state): State<AppState>,
    auth: AuthSession,
    headers: axum::http::HeaderMap,
    axum::Json(body): axum::Json<ShopConfigBody>,
) -> Result<Json<Value>, AppError> {
    let request_id = "update_shop_config";
    let user = auth.require_auth(request_id)?;
    admin_authorize(&state, &auth, "shop.manage", request_id).await?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let reason = body
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .ok_or_else(|| {
            AppError::bad_request("reason is required for admin operation", request_id, None)
        })?;
    // 版本门：If-Match 头必填（乐观锁；与 settings/storage 同约定）。
    let expected_version: i64 = headers
        .get("if-match")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("If-Match header is required", request_id, None))?
        .trim()
        .trim_matches('"')
        .parse()
        .map_err(|_| {
            AppError::bad_request(
                "If-Match must be the current version integer",
                request_id,
                None,
            )
        })?;

    if let Some(max) = body.max_quantity_per_order {
        if !(1..=1000).contains(&max) {
            return Err(AppError::bad_request(
                "max_quantity_per_order must be between 1 and 1000",
                request_id,
                None,
            ));
        }
    }
    if let Some(policy) = &body.default_refund_policy {
        if !["non_refundable", "compensation_only", "full_refund"].contains(&policy.as_str()) {
            return Err(AppError::bad_request(
                "invalid default_refund_policy",
                request_id,
                None,
            ));
        }
    }

    let now = crate::outbox::now_millis();
    let sql = "UPDATE shop_site_config
               SET enabled = COALESCE(?, enabled),
                   max_quantity_per_order = COALESCE(?, max_quantity_per_order),
                   default_refund_policy = COALESCE(?, default_refund_policy),
                   version = version + 1, updated_by = ?, updated_at = ?
               WHERE id = 'singleton' AND version = ?";
    let affected = match pool {
        Either::Left(p) => sqlx::query(sql)
            .bind(body.enabled.map(|b| b as i64))
            .bind(body.max_quantity_per_order)
            .bind(&body.default_refund_policy)
            .bind(&user.id)
            .bind(now)
            .bind(expected_version)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?
            .rows_affected(),
        Either::Right(p) => sqlx::query(sql)
            .bind(body.enabled.map(|b| b as i64))
            .bind(body.max_quantity_per_order)
            .bind(&body.default_refund_policy)
            .bind(&user.id)
            .bind(now)
            .bind(expected_version)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?
            .rows_affected(),
    };
    if affected == 0 {
        return Err(AppError::conflict(
            "shop config version mismatch (reload and retry)",
            request_id,
        ));
    }

    let mut changed: Vec<&str> = Vec::new();
    if body.enabled.is_some() {
        changed.push("enabled");
    }
    if body.max_quantity_per_order.is_some() {
        changed.push("max_quantity_per_order");
    }
    if body.default_refund_policy.is_some() {
        changed.push("default_refund_policy");
    }
    AuditEntry::user_action(&user.id, "shop.config.update")
        .with_target("shop_site_config", "singleton")
        .with_reason(reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .with_metadata(json!({ "changed": changed, "version": expected_version + 1 }))
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let row = load_shop_site_config(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    Ok(Json(shop_config_json(&row)))
}

async fn list_admin_products(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "list_admin_products";
    admin_authorize(&state, &auth, "shop.manage", request_id).await?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    crate::shop::service::list_admin_products(pool)
        .await
        .map(Json)
        .map_err(|e| shop_error_to_app(e, request_id))
}

async fn create_admin_product(
    State(state): State<AppState>,
    auth: AuthSession,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "create_admin_product";
    let user = auth.require_auth(request_id)?;
    admin_authorize(&state, &auth, "shop.manage", request_id).await?;
    let reason = body
        .get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or("create product")
        .to_string();
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    // 新商品未显式指定退款策略时采用站点默认（shop_site_config）。
    let body = if body.get("refund_policy").is_none() {
        let cfg = load_shop_site_config(pool)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        let mut body = body.clone();
        body["refund_policy"] = json!(cfg.default_refund_policy);
        body
    } else {
        body
    };
    let result = crate::shop::service::create_product(pool, &body, &user.id).await;
    match result {
        Ok(v) => {
            AuditEntry::user_action(&user.id, "shop.product.create")
                .with_target("product", v["id"].as_str().unwrap_or(""))
                .with_reason(&reason)
                .with_policy_version(AUTHZ_POLICY_VERSION)
                .record(pool)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            Ok(Json(v))
        }
        Err(e) => Err(shop_error_to_app(e, request_id)),
    }
}

async fn update_admin_product(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    headers: axum::http::HeaderMap,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "update_admin_product";
    let user = auth.require_auth(request_id)?;
    admin_authorize(&state, &auth, "shop.manage", request_id).await?;
    let reason = body
        .get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or("update product");
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let expected_version: i64 = headers
        .get("if-match")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().trim_matches('"').parse().ok())
        .ok_or_else(|| AppError::bad_request("If-Match header is required", request_id, None))?;
    let current_version: Option<i64> = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT version FROM shop_products WHERE id = ?")
            .bind(&id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_scalar("SELECT version FROM shop_products WHERE id = ?")
            .bind(&id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    let Some(current_version) = current_version else {
        return Err(AppError::not_found("product not found", request_id));
    };
    if current_version != expected_version {
        return Err(AppError::conflict(
            "product version mismatch (reload and retry)",
            request_id,
        ));
    }
    let result = crate::shop::service::update_product(pool, &id, &body).await;
    match result {
        Ok(v) => {
            AuditEntry::user_action(&user.id, "shop.product.update")
                .with_target("product", &id)
                .with_reason(reason)
                .with_policy_version(AUTHZ_POLICY_VERSION)
                .record(pool)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            Ok(Json(v))
        }
        Err(e) => Err(shop_error_to_app(e, request_id)),
    }
}

async fn disable_product(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<ReasonBody>,
) -> Result<Json<Value>, AppError> {
    let request_id = "disable_product";
    let user = auth.require_auth(request_id)?;
    admin_authorize(&state, &auth, "shop.manage", request_id).await?;
    let reason = body.reason.as_deref().unwrap_or("disable product");
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let result = crate::shop::service::disable_product(pool, &id).await;
    match result {
        Ok(v) => {
            AuditEntry::user_action(&user.id, "shop.product.disable")
                .with_target("product", &id)
                .with_reason(reason)
                .with_policy_version(AUTHZ_POLICY_VERSION)
                .record(pool)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            Ok(Json(v))
        }
        Err(e) => Err(shop_error_to_app(e, request_id)),
    }
}

async fn publish_product(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<ReasonBody>,
) -> Result<Json<Value>, AppError> {
    let request_id = "publish_product";
    let user = auth.require_auth(request_id)?;
    admin_authorize(&state, &auth, "shop.manage", request_id).await?;
    let reason = body.reason.as_deref().unwrap_or("publish product");
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let result = crate::shop::service::publish_product(pool, &id).await;
    match result {
        Ok(v) => {
            AuditEntry::user_action(&user.id, "shop.product.publish")
                .with_target("product", &id)
                .with_reason(reason)
                .with_policy_version(AUTHZ_POLICY_VERSION)
                .record(pool)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            Ok(Json(v))
        }
        Err(e) => Err(shop_error_to_app(e, request_id)),
    }
}

async fn list_admin_orders(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "list_admin_orders";
    admin_authorize(&state, &auth, "shop.manage", request_id).await?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    crate::shop::service::list_admin_orders(pool)
        .await
        .map(Json)
        .map_err(|e| shop_error_to_app(e, request_id))
}

#[derive(Deserialize)]
struct RefundBody {
    reason: Option<String>,
}

async fn refund_order(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<RefundBody>,
) -> Result<Json<Value>, AppError> {
    let request_id = "refund_order";
    let user = auth.require_auth(request_id)?;
    admin_authorize(&state, &auth, "shop.refund", request_id).await?;
    let reason = body.reason.as_deref().unwrap_or("admin refund");
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    crate::shop::service::refund_order(pool, &id, &user.id, reason)
        .await
        .map(Json)
        .map_err(|e| shop_error_to_app(e, request_id))
}

// 锚定 ShopError 类型供路由层签名使用（避免未使用导入告警）。
#[allow(dead_code)]
fn _anchor(_: ShopError) {}
