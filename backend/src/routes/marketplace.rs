use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::Json,
    routing::{get, post},
    Router,
};
use serde_json::{json, Value};

use crate::app::AppState;
use crate::auth::session::AuthSession;
use crate::authz::decision::AUTHZ_POLICY_VERSION;
use crate::authz::enforce::authorize_action;
use crate::error::AppError;
use crate::marketplace::{checkout, clients, offers, refunds};
use crate::outbox::now_millis;

/// Marketplace 路由（M12）。
///
/// 认证模型（docs/MARKETPLACE.md §2/§4；M12 设计约束 #1）：
/// - 服务操作（offer 创建/更新、退款、服务端 Purchase 查询）使用
///   `Authorization: Basic client_id:client_secret`（Confidential Client
///   秘密）或管理员（reason + recent-auth）；普通 OIDC scope 永远不能调用；
/// - Checkout Intent 创建使用 Session 认证（OIDC scope 白名单冻结为
///   openid/profile/email，不存在可用的 user-bound marketplace.* Token）；
/// - confirm 使用 Session + CSRF + intent/user/client 一致性校验。
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/marketplace/offers", post(create_offer))
        .route(
            "/api/v1/marketplace/offers/{id}",
            get(get_offer).patch(update_offer),
        )
        .route(
            "/api/v1/marketplace/checkout-intents",
            post(create_checkout_intent),
        )
        .route(
            "/api/v1/marketplace/checkout-intents/{id}",
            get(get_checkout_intent_view),
        )
        .route(
            "/api/v1/marketplace/checkout-intents/{id}/confirm",
            post(confirm_checkout_intent),
        )
        .route("/api/v1/marketplace/purchases", get(list_purchases))
        .route("/api/v1/marketplace/purchases/{id}", get(get_purchase))
        .route(
            "/api/v1/marketplace/purchases/{id}/refund",
            post(refund_purchase),
        )
}

fn marketplace_err(e: crate::marketplace::MarketplaceError, request_id: &str) -> AppError {
    crate::marketplace::marketplace_error_to_app(e, request_id)
}

/// 服务认证（HTTP Basic client_id:secret；Confidential Client + 已批准 scope）。
async fn require_service_principal(
    state: &AppState,
    headers: &HeaderMap,
    scope: &str,
    request_id: &str,
) -> Result<clients::ServicePrincipal, AppError> {
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let basic = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    let Some((client_id, secret)) = clients::parse_basic_auth(basic) else {
        return Err(AppError::unauthorized(
            "client credentials required (Basic client_id:client_secret)",
            request_id,
        ));
    };
    clients::service_authenticate(pool, &client_id, &secret, scope)
        .await
        .map_err(|e| marketplace_err(e, request_id))
}

/// 管理员检查（admin.manage + reason + recent-auth 由调用方在需要时执行）。
async fn is_admin_user(
    state: &AppState,
    user_id: &str,
    request_id: &str,
) -> Result<bool, AppError> {
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let decision = authorize_action(pool, user_id, "admin.manage", None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    Ok(decision.is_allowed())
}

/// POST /api/v1/marketplace/offers — 服务端登记报价（Confidential Client）。
async fn create_offer(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "post_marketplace_offers";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let principal =
        require_service_principal(&state, &headers, "marketplace.offer.write", request_id).await?;
    let offer = offers::create_offer(pool, &principal, &body, now_millis())
        .await
        .map_err(|e| marketplace_err(e, request_id))?;
    Ok(Json(json!({ "offer": offers::offer_json(&offer) })))
}

/// GET /api/v1/marketplace/offers/{id} — 报价读取（任意已认证方；确认页用）。
async fn get_offer(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_marketplace_offers_id_";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    // 服务端 Client（自己的 Offer）、Session 用户或管理员均可读。
    let offer = offers::get_offer(pool, &id)
        .await
        .map_err(|e| marketplace_err(e, request_id))?
        .ok_or_else(|| AppError::not_found("offer not found", request_id))?;
    if let Some(user) = auth.user.as_ref() {
        let _ = user;
    } else if let Ok(principal) =
        require_service_principal(&state, &headers, "marketplace.purchases.read", request_id).await
    {
        if principal.client.id != offer.client_id {
            return Err(AppError::forbidden(
                "offer belongs to another client",
                request_id,
            ));
        }
    } else {
        return Err(AppError::unauthorized(
            "authentication required",
            request_id,
        ));
    }
    Ok(Json(json!({ "offer": offers::offer_json(&offer) })))
}

/// PATCH /api/v1/marketplace/offers/{id} — 更新报价（服务认证 + If-Match）。
async fn update_offer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "patch_marketplace_offers_id_";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let expected_version = headers
        .get("if-match")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok())
        .ok_or_else(|| AppError::bad_request("If-Match required", request_id, None))?;
    let principal =
        require_service_principal(&state, &headers, "marketplace.offer.write", request_id).await?;
    let offer = offers::update_offer(pool, &principal, &id, expected_version, &body, now_millis())
        .await
        .map_err(|e| marketplace_err(e, request_id))?;
    Ok(Json(json!({ "offer": offers::offer_json(&offer) })))
}

/// GET /api/v1/marketplace/checkout-intents/{id} — 托管确认页视图
/// （Session 绑定用户本人；显示商户/商品/金额/余额变化/授权期限）。
async fn get_checkout_intent_view(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_marketplace_checkout_intents_id_";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let view = checkout::intent_checkout_view(pool, &user.id, &id, now_millis())
        .await
        .map_err(|e| marketplace_err(e, request_id))?;
    Ok(Json(view))
}

/// POST /api/v1/marketplace/checkout-intents — Session 用户创建短 TTL Intent。
///
/// 请求体只接受 `client_id/offer_id/expected_offer_version/merchant_order_id/
/// quantity`；金额/货币/收款方全部服务端派生。强制 `Idempotency-Key`。
async fn create_checkout_intent(
    State(state): State<AppState>,
    auth: AuthSession,
    headers: HeaderMap,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "post_marketplace_checkout_intents";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let client_id = body
        .get("client_id")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("client_id required", request_id, None))?
        .to_string();
    let offer_id = body
        .get("offer_id")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("offer_id required", request_id, None))?
        .to_string();
    let expected_version = body
        .get("expected_offer_version")
        .and_then(Value::as_i64)
        .ok_or_else(|| {
            AppError::bad_request("expected_offer_version required", request_id, None)
        })?;
    let merchant_order_id = body
        .get("merchant_order_id")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("merchant_order_id required", request_id, None))?
        .to_string();
    let quantity = body
        .get("quantity")
        .and_then(Value::as_i64)
        .ok_or_else(|| AppError::bad_request("quantity required", request_id, None))?;
    let key = idempotency_header(&headers, request_id)?;

    let view = checkout::create_intent(
        pool,
        &user.id,
        &client_id,
        &offer_id,
        expected_version,
        &merchant_order_id,
        quantity,
        key,
    )
    .await
    .map_err(|e| marketplace_err(e, request_id))?;
    Ok(Json(json!({ "checkout_intent": view })))
}

/// POST /api/v1/marketplace/checkout-intents/{id}/confirm — Session + CSRF
/// 原子购买。`decision=confirm` 消费 Intent 并完成购买；`deny` 取消。
async fn confirm_checkout_intent(
    State(state): State<AppState>,
    auth: AuthSession,
    headers: HeaderMap,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "post_marketplace_checkout_intents_id_confirm";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let interaction_id = body
        .get("interaction_id")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("interaction_id required", request_id, None))?
        .to_string();
    if interaction_id != id {
        return Err(marketplace_err(
            crate::marketplace::MarketplaceError::CheckoutInteractionInvalid,
            request_id,
        ));
    }
    let decision = body
        .get("decision")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("decision required", request_id, None))?
        .to_string();
    if !matches!(decision.as_str(), "confirm" | "deny") {
        return Err(AppError::bad_request(
            "decision must be confirm or deny",
            request_id,
            None,
        ));
    }
    let expected_version = body
        .get("expected_intent_version")
        .and_then(Value::as_i64)
        .ok_or_else(|| {
            AppError::bad_request("expected_intent_version required", request_id, None)
        })?;
    let key = idempotency_header(&headers, request_id)?;

    if decision == "deny" {
        let view = checkout::deny_intent(pool, &user.id, &interaction_id, now_millis())
            .await
            .map_err(|e| marketplace_err(e, request_id))?;
        return Ok(Json(json!({ "checkout": view })));
    }
    let purchase = checkout::confirm_intent(pool, &user.id, &interaction_id, expected_version, key)
        .await
        .map_err(|e| marketplace_err(e, request_id))?;
    Ok(Json(json!({ "purchase": purchase })))
}

/// GET /api/v1/marketplace/purchases — 用户本人或 Client 服务端列表。
async fn list_purchases(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth: AuthSession,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_marketplace_purchases";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let after = params.get("after").map(String::as_str);
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(30);

    if let Some(user) = auth.user.as_ref() {
        // 用户本人列表：只显示自己的 Purchase（隐藏其他 Client 的交易）。
        let items = checkout::list_purchases(pool, Some(&user.id), None, after, limit)
            .await
            .map_err(|e| marketplace_err(e, request_id))?;
        return Ok(Json(
            json!({ "purchases": attach_refunds(pool, items, request_id).await? }),
        ));
    }
    // 服务端：Client 自己的 Purchase（按 merchant_order_id 精确查询优先）。
    if let Some(merchant_order_id) = params.get("merchant_order_id") {
        let principal =
            require_service_principal(&state, &headers, "marketplace.purchases.read", request_id)
                .await?;
        let purchase =
            checkout::get_purchase_by_merchant_order(pool, &principal.client.id, merchant_order_id)
                .await
                .map_err(|e| marketplace_err(e, request_id))?;
        return Ok(Json(json!({
            "purchases": purchase.map(|p| checkout::purchase_json(&p)).into_iter().collect::<Vec<_>>()
        })));
    }
    let principal =
        require_service_principal(&state, &headers, "marketplace.purchases.read", request_id)
            .await?;
    let items = checkout::list_purchases(pool, None, Some(&principal.client.id), after, limit)
        .await
        .map_err(|e| marketplace_err(e, request_id))?;
    Ok(Json(
        json!({ "purchases": attach_refunds(pool, items, request_id).await? }),
    ))
}

/// 为 Purchase 视图附加退款记录（用户退款入口/状态展示）。
async fn attach_refunds(
    pool: &crate::db::DatabasePool,
    items: Vec<Value>,
    request_id: &str,
) -> Result<Vec<Value>, AppError> {
    let mut out = Vec::new();
    for mut p in items {
        let purchase_id = p["id"].as_str().unwrap_or("").to_string();
        let refunds = refunds::list_refunds(pool, Some(&purchase_id), None, None, 10)
            .await
            .map_err(|e| marketplace_err(e, request_id))?;
        p["refunds"] = json!(refunds);
        out.push(p);
    }
    Ok(out)
}

/// GET /api/v1/marketplace/purchases/{id} — 本人 / 本 Client / 管理员。
async fn get_purchase(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_marketplace_purchases_id_";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let purchase = checkout::get_purchase(pool, &id)
        .await
        .map_err(|e| marketplace_err(e, request_id))?
        .ok_or_else(|| AppError::not_found("purchase not found", request_id))?;
    if let Some(user) = auth.user.as_ref() {
        if user.id == purchase.user_id {
            let mut view = checkout::purchase_json(&purchase);
            view["refunds"] =
                json!(
                    refunds::list_refunds(pool, Some(&purchase.id), None, None, 10)
                        .await
                        .map_err(|e| marketplace_err(e, request_id))?
                );
            return Ok(Json(view));
        }
        if is_admin_user(&state, &user.id, request_id).await? {
            let mut view = checkout::purchase_json(&purchase);
            view["refunds"] =
                json!(
                    refunds::list_refunds(pool, Some(&purchase.id), None, None, 10)
                        .await
                        .map_err(|e| marketplace_err(e, request_id))?
                );
            return Ok(Json(view));
        }
        return Err(AppError::not_found("purchase not found", request_id));
    }
    if let Ok(principal) =
        require_service_principal(&state, &headers, "marketplace.purchases.read", request_id).await
    {
        if principal.client.id == purchase.client_id {
            let mut view = checkout::purchase_json(&purchase);
            view["refunds"] =
                json!(
                    refunds::list_refunds(pool, Some(&purchase.id), None, None, 10)
                        .await
                        .map_err(|e| marketplace_err(e, request_id))?
                );
            return Ok(Json(view));
        }
        return Err(AppError::not_found("purchase not found", request_id));
    }
    Err(AppError::unauthorized(
        "authentication required",
        request_id,
    ))
}

/// POST /api/v1/marketplace/purchases/{id}/refund — Client 服务退款
/// 或管理员强制退款（reason + recent-auth，路由层校验）。
async fn refund_purchase(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth: AuthSession,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "post_marketplace_purchases_id_refund";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let input =
        refunds::validate_refund_input(&body).map_err(|e| marketplace_err(e, request_id))?;
    let key = idempotency_header(&headers, request_id)?;

    // 管理员强制退款：Session + admin.manage + reason + recent-auth + 审计。
    if let Some(user) = auth.user.as_ref() {
        if is_admin_user(&state, &user.id, request_id).await? {
            let reason = body
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            if reason.is_empty() {
                return Err(AppError::bad_request(
                    "reason is required for admin refund",
                    request_id,
                    None,
                ));
            }
            let view = refunds::create_refund(pool, &user.id, "admin", None, &id, &input, key)
                .await
                .map_err(|e| marketplace_err(e, request_id))?;
            let _ =
                AuditEntryHelper::record_admin_refund(pool, &user.id, &view, &reason, request_id)
                    .await;
            return Ok(Json(view));
        }
        return Err(AppError::forbidden(
            "admin.manage required for admin refund",
            request_id,
        ));
    }

    // Client 服务退款：Confidential Client + marketplace.refund scope。
    let principal =
        require_service_principal(&state, &headers, "marketplace.refund", request_id).await?;
    let view = refunds::create_refund_inner(
        pool,
        &principal.client.client_id,
        &principal.client.owner_user_id,
        "client",
        Some(&principal.client),
        &id,
        &input,
        key,
    )
    .await
    .map_err(|e| marketplace_err(e, request_id))?;
    Ok(Json(view))
}

/// 提取并校验 Idempotency-Key 头。
#[allow(clippy::result_large_err)] // AppError 为统一错误类型，体积固定可接受
fn idempotency_header<'a>(headers: &'a HeaderMap, request_id: &str) -> Result<&'a str, AppError> {
    let key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("Idempotency-Key required", request_id, None))?;
    if key.len() < 16 || key.len() > 200 {
        return Err(AppError::bad_request(
            "Idempotency-Key must be 16..=200 chars",
            request_id,
            None,
        ));
    }
    Ok(key)
}

/// 审计助手（管理员退款审计，reason 必填）。
struct AuditEntryHelper;

impl AuditEntryHelper {
    async fn record_admin_refund(
        pool: &crate::db::DatabasePool,
        actor_id: &str,
        view: &Value,
        reason: &str,
        request_id: &str,
    ) -> Result<(), AppError> {
        crate::audit::AuditEntry::user_action(actor_id, "marketplace.refund_admin")
            .with_target("purchase", view["purchase_id"].as_str().unwrap_or(""))
            .with_target("refund", view["id"].as_str().unwrap_or(""))
            .with_reason(reason)
            .with_policy_version(AUTHZ_POLICY_VERSION)
            .record(pool)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))
    }
}
