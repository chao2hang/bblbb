use axum::{
    extract::{Path, State},
    http::{header, HeaderMap},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::Value;

use crate::app::AppState;
use crate::auth::session::AuthSession;
use crate::authz::decision::AUTHZ_POLICY_VERSION;
use crate::authz::enforce::authorize_action;
use crate::error::AppError;
use crate::shop::service::shop_error_to_app;

/// 商城与权益路由（M07-SHOP）。
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/shop/products", get(list_shop_products))
        .route("/api/v1/shop/products/{id}", get(get_shop_product))
        // M07-SHOP-UI-10：active 装扮样式库（前端标签/预览解析用）
        .route("/api/v1/shop/cosmetics", get(list_public_cosmetics))
        .route("/api/v1/shop/orders", post(create_shop_order))
        .route("/api/v1/shop/orders/{id}", get(get_shop_order))
        .route("/api/v1/me/entitlements", get(get_me_entitlements))
        .route(
            "/api/v1/me/entitlements/{id}/equip",
            post(equip_entitlement),
        )
        .route(
            "/api/v1/me/entitlements/{id}/unequip",
            post(unequip_entitlement),
        )
        .route("/api/v1/me/presentation", get(get_me_presentation))
        // M07-SHOP-ASSETS：Steam 装扮素材统一资产路由（公开、不可变缓存）。
        .route(
            "/api/v1/steam-assets/{kind}/{file}",
            get(steam_asset_content),
        )
        // Steam 装扮目录（实时镜像；首次访问用内置快照种子）。
        .route("/api/v1/shop/steam-catalog", get(list_steam_catalog))
}

#[derive(Deserialize)]
struct SteamCatalogQuery {
    kind: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
    q: Option<String>,
}

/// GET /api/v1/shop/steam-catalog — Steam 装扮目录分页查询（公开只读）。
async fn list_steam_catalog(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<SteamCatalogQuery>,
) -> Result<Json<Value>, AppError> {
    const REQUEST_ID: &str = "list_steam_catalog";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", REQUEST_ID))?;
    let kind = match query.kind.as_deref() {
        Some("backgrounds") => crate::shop::steam_assets::KIND_BACKGROUNDS,
        _ => crate::shop::steam_assets::KIND_FRAMES,
    };
    let (items, total) = crate::shop::steam_assets::list_catalog(
        pool,
        kind,
        query.page.unwrap_or(1),
        query.page_size.unwrap_or(200),
        query.q.as_deref(),
    )
    .await
    .map_err(|e| shop_error_to_app(e, REQUEST_ID))?;
    Ok(Json(serde_json::json!({
        "kind": kind,
        "items": items,
        "total": total,
    })))
}

#[derive(Deserialize)]
struct CreateOrderBody {
    product_id: String,
    #[serde(default = "default_quantity")]
    quantity: i64,
    idempotency_key: Option<String>,
    client_request_id: Option<String>,
    expected_product_version: Option<i64>,
}

fn default_quantity() -> i64 {
    1
}

#[derive(Deserialize, Default)]
struct PresentationVersionBody {
    expected_presentation_version: Option<i64>,
}

async fn list_shop_products(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "list_shop_products";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let user = auth.require_auth(request_id)?;
    let decision = authorize_action(pool, &user.id, "shop.read", None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden("shop read not allowed", request_id));
    }
    crate::shop::service::list_products(pool, false)
        .await
        .map(Json)
        .map_err(|e| shop_error_to_app(e, request_id))
}

async fn get_shop_product(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_shop_product";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let user = auth.require_auth(request_id)?;
    let decision = authorize_action(pool, &user.id, "shop.read", None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden("shop read not allowed", request_id));
    }
    let product = crate::shop::service::get_product(pool, &id)
        .await
        .map_err(|e| shop_error_to_app(e, request_id))?;
    if product.get("status").and_then(Value::as_str) != Some("published") {
        return Err(AppError::not_found("product not found", request_id));
    }
    Ok(Json(product))
}

/// active 装扮样式库（M07-SHOP-UI-10）：衣柜标签解析与商品预览用；
/// 只暴露 id/kind/name/style，样式为服务端校验过的结构化参数。
async fn list_public_cosmetics(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "list_public_cosmetics";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let user = auth.require_auth(request_id)?;
    let decision = authorize_action(pool, &user.id, "shop.read", None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden("shop read not allowed", request_id));
    }
    crate::shop::cosmetics::list_defs(pool, false)
        .await
        .map(Json)
        .map_err(|e| shop_error_to_app(e, request_id))
}

async fn create_shop_order(
    State(state): State<AppState>,
    auth: AuthSession,
    headers: HeaderMap,
    axum::Json(body): axum::Json<CreateOrderBody>,
) -> Result<Json<Value>, AppError> {
    let request_id = "create_shop_order";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let decision = authorize_action(pool, &user.id, "shop.purchase", None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden("shop purchase not allowed", request_id));
    }
    if let Some(expected_version) = body.expected_product_version {
        let product = crate::shop::service::get_product(pool, &body.product_id)
            .await
            .map_err(|e| shop_error_to_app(e, request_id))?;
        if product.get("version").and_then(Value::as_i64) != Some(expected_version) {
            return Err(AppError::conflict(
                "product version mismatch (reload and retry)",
                request_id,
            ));
        }
    }
    let key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
        .or_else(|| body.idempotency_key.clone())
        .or_else(|| body.client_request_id.clone())
        .ok_or_else(|| {
            AppError::bad_request("Idempotency-Key header is required", request_id, None)
        })?;
    if !(16..=200).contains(&key.len()) {
        return Err(AppError::bad_request(
            "Idempotency-Key must be 16..200 characters",
            request_id,
            None,
        ));
    }
    crate::shop::service::buy_product(pool, &user.id, &body.product_id, body.quantity, &key)
        .await
        .map(Json)
        .map_err(|e| shop_error_to_app(e, request_id))
}

async fn get_shop_order(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_shop_order";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    crate::shop::service::get_order(pool, &user.id, &id, false)
        .await
        .map(Json)
        .map_err(|e| shop_error_to_app(e, request_id))
}

async fn get_me_entitlements(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_me_entitlements";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    crate::shop::service::list_my_entitlements(pool, &user.id)
        .await
        .map(Json)
        .map_err(|e| shop_error_to_app(e, request_id))
}

async fn equip_entitlement(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    body: Option<axum::Json<PresentationVersionBody>>,
) -> Result<Json<Value>, AppError> {
    let request_id = "equip_entitlement";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let decision = authorize_action(
        pool,
        &user.id,
        "shop.entitlement.manage_own",
        None,
        AUTHZ_POLICY_VERSION,
    )
    .await
    .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden("not allowed", request_id));
    }
    crate::shop::service::equip_with_version(
        pool,
        &user.id,
        &id,
        body.and_then(|b| b.0.expected_presentation_version),
    )
    .await
    .map(Json)
    .map_err(|e| shop_error_to_app(e, request_id))
}

async fn unequip_entitlement(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    body: Option<axum::Json<PresentationVersionBody>>,
) -> Result<Json<Value>, AppError> {
    let request_id = "unequip_entitlement";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let decision = authorize_action(
        pool,
        &user.id,
        "shop.entitlement.manage_own",
        None,
        AUTHZ_POLICY_VERSION,
    )
    .await
    .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden("not allowed", request_id));
    }
    crate::shop::service::unequip_with_version(
        pool,
        &user.id,
        &id,
        body.and_then(|b| b.0.expected_presentation_version),
    )
    .await
    .map(Json)
    .map_err(|e| shop_error_to_app(e, request_id))
}

async fn get_me_presentation(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_me_presentation";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    crate::shop::service::get_presentation(pool, &user.id)
        .await
        .map(Json)
        .map_err(|e| shop_error_to_app(e, request_id))
}

/// GET /api/v1/steam-assets/{kind}/{file} — Steam 装扮素材读取（M07-SHOP-ASSETS）。
///
/// kind 仅限 `backgrounds` / `frames`，file 必须是 40 位内容哈希 + 受限扩展名。
/// local 后端直接流式返回字节；S3 后端优先 302 预签名下载，避免服务端中转。
/// 文件名即内容哈希，响应按不可变资源缓存；未上架/未下载 → 404。
async fn steam_asset_content(
    State(state): State<AppState>,
    Path((kind, file)): Path<(String, String)>,
) -> Result<Response, AppError> {
    const REQUEST_ID: &str = "steam_asset_content";
    let Some(storage) = state.storage.as_deref() else {
        return Err(AppError::internal("storage not configured", REQUEST_ID));
    };
    match crate::shop::steam_assets::load_asset(state.db.as_deref(), storage, &kind, &file).await {
        Ok(asset) => Ok((
            [
                (header::CONTENT_TYPE, asset.content_type.to_string()),
                (
                    header::CACHE_CONTROL,
                    "public, max-age=31536000, immutable".to_string(),
                ),
            ],
            asset.data,
        )
            .into_response()),
        Err(e) => Err(shop_error_to_app(e, REQUEST_ID)),
    }
}
