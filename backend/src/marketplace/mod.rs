//! M12-Marketplace：第三方 Marketplace 服务层。
//!
//! 职责边界（与 `routes/marketplace.rs` / `routes/admin.rs` 路由层分离）：
//! - 领域模块不依赖 axum；数据库访问使用 `sqlx::Either` 投影（与
//!   `src/video/state.rs` 同模式）；
//! - 账务恒等式（docs/MARKETPLACE-ACCOUNTING.md §8）：
//!   `Σ(delta_balance + delta_pending + delta_frozen) = 0` 由每次购买/退款
//!   同事务写多个账本 operation（买方扣款 + 商户入账 + 平台费）保证；
//! - 买方/平台费走不可变账本 `economy::ledger::service`；商户
//!   `marketplace_merchant_accounts` 的 available/pending/frozen 是运营余额，
//!   与商户账本 operation 组（`source_type=marketplace_purchase`,
//!   `source_id=purchase_id`）对账一致；
//! - 购买锁顺序（固定）：idempotency operation → checkout intent → offer/stock
//!   → 买方 point account → 商户账户 → 平台费账户；
//! - 幂等：create intent / confirm / refund 强制 `Idempotency-Key`（复用
//!   `crate::idempotency`），同 key+摘要重放原结果、不同摘要 409；
//! - 退款只追加 reversal operation，禁止 UPDATE/DELETE 原 Purchase/流水。

pub mod balance;
pub mod checkout;
pub mod clients;
pub mod offers;
pub mod reconcile;
pub mod refunds;
pub mod webhooks;

use crate::error::AppError;

/// Marketplace 领域错误（稳定错误码见 docs/ERROR-CODES.md）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarketplaceError {
    Db(String),
    NotFound(String),
    Invalid(String),
    Forbidden(String),
    /// If-Match/version 乐观锁冲突。
    VersionConflict {
        expected: i64,
        current: i64,
    },
    /// 同幂等键不同请求摘要。
    IdempotencyConflict,
    /// Client/Scope 未激活或被禁用/紧急停用。
    MarketplaceDisabled(String),
    /// Client 不存在、非 Confidential 或凭证无效。
    InvalidClient(String),
    /// 报价不属于该 Client 或版本不符。
    OfferVersionChanged,
    /// 库存不足。
    OutOfStock,
    /// 买方余额不足。
    InsufficientFunds,
    /// 用户/Client 日累计或单笔限额超出。
    DailyLimitExceeded,
    /// Session 用户与 Intent 用户不一致。
    CheckoutUserMismatch,
    /// interaction 与 Intent 不一致或无效。
    CheckoutInteractionInvalid,
    /// Intent 已过期。
    CheckoutIntentExpired,
    /// Intent 已被消费（查询原 Purchase）。
    CheckoutIntentConsumed,
    /// 累计退款超过原购买金额。
    RefundExceedsPurchase,
    /// 退款不符合政策。
    RefundNotAllowed(String),
    /// 商户余额不足以完成退款。
    MerchantBalanceInsufficient,
    /// Webhook 签名校验失败。
    WebhookInvalidSignature,
    /// URL 被 SSRF 防护阻断。
    UrlBlocked(String),
    /// URL 非法（非 HTTPS 等）。
    InvalidUrl(String),
}

impl From<sqlx::Error> for MarketplaceError {
    fn from(e: sqlx::Error) -> Self {
        Self::Db(e.to_string())
    }
}

impl From<crate::economy::ledger::service::LedgerError> for MarketplaceError {
    fn from(e: crate::economy::ledger::service::LedgerError) -> Self {
        use crate::economy::ledger::service::LedgerError as L;
        match e {
            L::InsufficientBalance => Self::InsufficientFunds,
            L::IdempotencyConflict => Self::IdempotencyConflict,
            L::ConcurrentModification => Self::Db("concurrent modification".into()),
            L::NegativeBalance => Self::InsufficientFunds,
            other => Self::Db(other.to_string()),
        }
    }
}

impl std::fmt::Display for MarketplaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Db(msg) => write!(f, "marketplace db error: {msg}"),
            Self::NotFound(msg) => write!(f, "marketplace not found: {msg}"),
            Self::Invalid(msg) => write!(f, "invalid marketplace request: {msg}"),
            Self::Forbidden(msg) => write!(f, "marketplace forbidden: {msg}"),
            Self::VersionConflict { expected, current } => write!(
                f,
                "marketplace version conflict: expected {expected}, current {current}"
            ),
            Self::IdempotencyConflict => write!(f, "idempotency key reused with different payload"),
            Self::MarketplaceDisabled(msg) => write!(f, "marketplace disabled: {msg}"),
            Self::InvalidClient(msg) => write!(f, "invalid marketplace client: {msg}"),
            Self::OfferVersionChanged => write!(f, "offer version changed"),
            Self::OutOfStock => write!(f, "offer out of stock"),
            Self::InsufficientFunds => write!(f, "insufficient funds"),
            Self::DailyLimitExceeded => write!(f, "daily limit exceeded"),
            Self::CheckoutUserMismatch => write!(f, "checkout user mismatch"),
            Self::CheckoutInteractionInvalid => write!(f, "checkout interaction invalid"),
            Self::CheckoutIntentExpired => write!(f, "checkout intent expired"),
            Self::CheckoutIntentConsumed => write!(f, "checkout intent consumed"),
            Self::RefundExceedsPurchase => write!(f, "refund exceeds purchase"),
            Self::RefundNotAllowed(msg) => write!(f, "refund not allowed: {msg}"),
            Self::MerchantBalanceInsufficient => write!(f, "merchant balance insufficient"),
            Self::WebhookInvalidSignature => write!(f, "webhook invalid signature"),
            Self::UrlBlocked(msg) => write!(f, "url blocked: {msg}"),
            Self::InvalidUrl(msg) => write!(f, "invalid url: {msg}"),
        }
    }
}

impl std::error::Error for MarketplaceError {}

impl MarketplaceError {
    /// 稳定错误码（docs/ERROR-CODES.md；新增码必须同步注册表与 OpenAPI 说明）。
    pub fn code(&self) -> &'static str {
        match self {
            Self::Db(_) => "internal_error",
            Self::NotFound(_) => "not_found",
            Self::Invalid(_) => "invalid_request",
            Self::Forbidden(_) => "forbidden",
            Self::VersionConflict { .. } => "version_conflict",
            Self::IdempotencyConflict => "idempotency_conflict",
            Self::MarketplaceDisabled(_) => "marketplace_disabled",
            Self::InvalidClient(_) => "marketplace_invalid_client",
            Self::OfferVersionChanged => "offer_version_changed",
            Self::OutOfStock => "shop_stock_exhausted",
            Self::InsufficientFunds => "insufficient_funds",
            Self::DailyLimitExceeded => "daily_limit_exceeded",
            Self::CheckoutUserMismatch => "checkout_user_mismatch",
            Self::CheckoutInteractionInvalid => "checkout_interaction_invalid",
            Self::CheckoutIntentExpired => "checkout_intent_expired",
            Self::CheckoutIntentConsumed => "checkout_intent_consumed",
            Self::RefundExceedsPurchase => "refund_exceeds_purchase",
            Self::RefundNotAllowed(_) => "refund_not_allowed",
            Self::MerchantBalanceInsufficient => "merchant_balance_insufficient",
            Self::WebhookInvalidSignature => "webhook_invalid_signature",
            Self::UrlBlocked(_) => "invalid_url",
            Self::InvalidUrl(_) => "invalid_url",
        }
    }
}

/// 领域错误 → 路由层 Problem 响应（稳定 HTTP 状态 + 稳定错误码）。
///
/// M16-HARNESS-04：所有变体按 `MarketplaceError::code()` 输出稳定 Problem code
/// （与 docs/ERROR-CODES.md / OpenAPI Problem.code enum 一致），不再退化为
/// 通用 `conflict`/`bad_request`。
pub fn marketplace_error_to_app(e: MarketplaceError, request_id: &str) -> AppError {
    use MarketplaceError as M;
    let (status, title) = match &e {
        M::Db(_) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "Internal Server Error",
        ),
        M::NotFound(_) => (axum::http::StatusCode::NOT_FOUND, "Not Found"),
        M::Invalid(_) => (axum::http::StatusCode::BAD_REQUEST, "Bad Request"),
        M::Forbidden(_) => (axum::http::StatusCode::FORBIDDEN, "Forbidden"),
        M::VersionConflict { .. }
        | M::IdempotencyConflict
        | M::OfferVersionChanged
        | M::OutOfStock
        | M::InsufficientFunds
        | M::DailyLimitExceeded
        | M::CheckoutInteractionInvalid
        | M::CheckoutIntentExpired
        | M::CheckoutIntentConsumed
        | M::RefundExceedsPurchase
        | M::RefundNotAllowed(_)
        | M::MerchantBalanceInsufficient
        | M::MarketplaceDisabled(_) => (axum::http::StatusCode::CONFLICT, "Conflict"),
        M::InvalidClient(_) | M::WebhookInvalidSignature => {
            (axum::http::StatusCode::UNAUTHORIZED, "Unauthorized")
        }
        M::CheckoutUserMismatch => (axum::http::StatusCode::FORBIDDEN, "Forbidden"),
        M::UrlBlocked(_) | M::InvalidUrl(_) => (axum::http::StatusCode::BAD_REQUEST, "Bad Request"),
    };
    let code = e.code();
    let detail = e.to_string();
    AppError::with_code(status, code, title, detail, request_id)
}

// ─────────────────────────── 共享常量与助手 ───────────────────────────

/// Checkout Intent 有效期（5 分钟，MARKETPLACE.md §4）。
pub const INTENT_TTL_MS: i64 = 5 * 60 * 1000;
/// 默认结算等待期（7 天，MARKETPLACE-ACCOUNTING.md §3）。
pub const SETTLEMENT_DELAY_MS: i64 = 7 * 24 * 3600 * 1000;
/// Webhook 签名时间窗（5 分钟，MARKETPLACE.md §8）。
pub const WEBHOOK_TTL_MS: i64 = 5 * 60 * 1000;
/// Webhook 最大投递尝试次数。
pub const WEBHOOK_MAX_ATTEMPTS: i64 = 5;

/// 商户账本合成账户 ID（`point_accounts.user_id`；不指向真实用户，无 FK）。
pub fn merchant_ledger_user(client_id: &str) -> String {
    format!("merchant:{client_id}")
}

/// 平台费账本合成账户 ID。
pub fn fee_ledger_user() -> &'static str {
    "platform:fees"
}

/// 系统账本用户（商户/平台费）需要真实的 `users` 行以满足
/// `point_accounts/point_transactions` 的 FK（0047 迁移冻结）。
/// 该行密码哈希为 `!`，无法登录，仅用于承载账本恒等式。
pub async fn ensure_ledger_user(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    now: i64,
) -> Result<(), crate::marketplace::MarketplaceError> {
    let email = format!("{user_id}@system.local");
    let rows = match pool {
        sqlx::Either::Left(p) => sqlx::query(
            "INSERT OR IGNORE INTO users
             (id, username_normalized, email_normalized, password_hash, status, level, email_verified, created_at, updated_at)
             VALUES (?, ?, ?, '!', 'active', 0, 0, ?, ?)",
        )
        .bind(user_id)
        .bind(user_id)
        .bind(&email)
        .bind(now)
        .bind(now)
        .execute(p)
        .await?
        .rows_affected(),
        sqlx::Either::Right(p) => sqlx::query(
            "INSERT IGNORE INTO users
             (id, username_normalized, email_normalized, password_hash, status, level, email_verified, created_at, updated_at)
             VALUES (?, ?, ?, '!', 'active', 0, 0, ?, ?)",
        )
        .bind(user_id)
        .bind(user_id)
        .bind(&email)
        .bind(now)
        .bind(now)
        .execute(p)
        .await?
        .rows_affected(),
    };
    let _ = rows;
    Ok(())
}

/// 当前 Unix 毫秒。
pub fn now_millis() -> i64 {
    crate::outbox::now_millis()
}
