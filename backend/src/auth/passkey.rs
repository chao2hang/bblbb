//! Passkey（WebAuthn/FIDO2）第二因素（M02-MFA-PK）。
//!
//! Passkey 作为 MFA 登录第二步的**第三选项**，与 TOTP/恢复码 OR 共存：
//! 第二步恰好一个因素通过即签发会话（见 `mfa_login::complete_mfa_login`）。
//! 纯增量——未注册 Passkey 的用户路径与既有 TOTP 流程完全一致。
//!
//! - 注册：`POST /api/v1/auth/passkeys`（begin，返回 creation options）→
//!   浏览器 `navigator.credentials.create()` → `POST /api/v1/auth/passkeys/confirm`；
//! - 登录：第一步密码通过后（`mfa_required`），`POST /api/v1/auth/login/mfa/passkey/options`
//!   用一次性 MFA challenge 换 request options → 浏览器
//!   `navigator.credentials.get()` → 断言随 `POST /api/v1/auth/login/mfa` 提交。
//!
//! 安全约定：
//! - challenge 一律由服务端生成（webauthn-rs 内部生成），序列化后的
//!   registration/authentication state 存 `webauthn_challenges`（5 分钟过期、
//!   一次性消费），绝不信任客户端回传的 challenge；
//! - 登录断言 challenge 绑定具体一次两步登录（`mfa_login_challenges.token_hash`），
//!   防止跨会话重放；注册 challenge 绑定当前会话用户；
//! - 断言校验失败统一 `InvalidCredential`（不泄漏失败细节，防枚举）；
//! - webauthn-rs passkey 流程强制 user verification（UV），本模块仍显式断言
//!   `AuthenticationResult::user_verified()` 双保险；
//! - `Passkey` 凭据 serde 序列化存 `credential_json`（公钥必需，不可哈希），
//!   断言成功后经 `update_credential` 回写（sign counter / backup flags）。

use sqlx::Either;
use url::Url;
use uuid::Uuid;
use webauthn_rs::prelude::{
    CreationChallengeResponse, Passkey, PasskeyAuthentication, PasskeyRegistration,
    PublicKeyCredential, RegisterPublicKeyCredential, RequestChallengeResponse, Webauthn,
    WebauthnBuilder,
};

use crate::{
    auth::{mfa::MfaError, token::hash_token},
    config::AppConfig,
    db::pool::DatabasePool,
    outbox::now_millis,
};

/// WebAuthn challenge 有效期：5 分钟（毫秒），与 MFA login challenge 对齐。
pub const PASSKEY_CHALLENGE_TTL_MS: i64 = 5 * 60 * 1000;

/// 每用户 Passkey 数量上限（防无限制膨胀；恢复码/TOTP 不受此限）。
pub const PASSKEY_MAX_PER_USER: i64 = 10;

/// `webauthn_challenges.purpose`：注册 challenge（绑定会话用户）。
pub const PURPOSE_REGISTRATION: &str = "registration";
/// `webauthn_challenges.purpose`：登录断言 challenge（绑定 MFA login challenge）。
pub const PURPOSE_AUTHENTICATION: &str = "authentication";

// ─────────────────────────── 错误 ───────────────────────────

/// Passkey 服务错误（路由层统一映射，不泄漏细节防枚举）。
#[derive(Debug)]
pub enum PasskeyError {
    /// 服务端未配置 `passkey_rp_id`/`public_origin`（Passkey 功能关闭）。
    NotConfigured,
    /// 无进行中的 challenge / 已消费 / 已过期（统一，防枚举）。
    NoPendingChallenge,
    /// 该账号未注册任何有效 Passkey（登录 options 时）。
    NoCredentials,
    /// 浏览器凭据响应校验失败（统一，不区分原因）。
    InvalidCredential,
    /// 达到每用户 Passkey 数量上限。
    LimitReached,
    /// 目标凭据不存在（撤销时）。
    NotFound,
    /// 数据库错误。
    Database(String),
    /// webauthn-rs state / 凭据序列化错误（视为服务端内部错误）。
    Serialization(String),
}

impl std::fmt::Display for PasskeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PasskeyError::NotConfigured => write!(f, "passkey is not configured"),
            PasskeyError::NoPendingChallenge => write!(f, "no pending webauthn challenge"),
            PasskeyError::NoCredentials => write!(f, "no registered passkey"),
            PasskeyError::InvalidCredential => write!(f, "invalid webauthn credential response"),
            PasskeyError::LimitReached => write!(f, "passkey limit reached"),
            PasskeyError::NotFound => write!(f, "passkey not found"),
            PasskeyError::Database(e) => write!(f, "database error: {e}"),
            PasskeyError::Serialization(e) => write!(f, "webauthn state serialization error: {e}"),
        }
    }
}

impl std::error::Error for PasskeyError {}

impl From<sqlx::Error> for PasskeyError {
    fn from(e: sqlx::Error) -> Self {
        PasskeyError::Database(e.to_string())
    }
}

// ─────────────────────────── WebAuthn 实例 ───────────────────────────

/// 从配置构建 [`Webauthn`] 实例。
///
/// `passkey_rp_id` 为空 = Passkey 关闭（返回 `None`，路由层返回 500）。
/// rp_id 必须是 public_origin host 本身或其父域（`WebauthnBuilder` 会校验，
/// 配置层 `validate_passkey_config` 已在启动时前置校验）。
pub fn build_webauthn(config: &AppConfig) -> Option<Webauthn> {
    let rp_id = config.passkey_rp_id.trim();
    if rp_id.is_empty() || config.public_origin.trim().is_empty() {
        return None;
    }
    let origin = Url::parse(config.public_origin.trim()).ok()?;
    let rp_name = config.passkey_rp_name.trim();
    WebauthnBuilder::new(rp_id, &origin)
        .ok()?
        .rp_name(if rp_name.is_empty() { "BBLBB" } else { rp_name })
        .build()
        .ok()
}

// ─────────────────────────── 状态查询 ───────────────────────────

/// 用户是否存在有效 Passkey（未撤销）。
pub async fn has_active_passkey(pool: &DatabasePool, user_id: &str) -> Result<bool, sqlx::Error> {
    let exists: Option<bool> = match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM passkey_credentials
             WHERE user_id = ? AND revoked_at IS NULL)",
        )
        .bind(user_id)
        .fetch_one(p)
        .await
        .ok(),
        Either::Right(p) => sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM passkey_credentials
             WHERE user_id = ? AND revoked_at IS NULL)",
        )
        .bind(user_id)
        .fetch_one(p)
        .await
        .ok(),
    };
    Ok(exists.unwrap_or(false))
}

/// 用户是否启用了任一 MFA 第二因素（TOTP 或 Passkey）。
///
/// `mfa_required`（login.rs）与 `Me.mfa_enabled`（users.rs）的唯一判定来源。
pub async fn has_second_factor(pool: &DatabasePool, user_id: &str) -> Result<bool, MfaError> {
    if crate::auth::mfa::has_confirmed_totp(pool, user_id).await? {
        return Ok(true);
    }
    has_active_passkey(pool, user_id)
        .await
        .map_err(|e| MfaError::Database(e.to_string()))
}

/// 列表投影（不含任何凭据秘密）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct PasskeyInfo {
    /// 内部记录 ID（撤销接口使用；非 WebAuthn credential id）。
    pub id: String,
    pub name: String,
    pub aaguid: Option<String>,
    pub backup_eligible: bool,
    pub backed_up: bool,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}

/// list_passkeys 行投影（sqlite/mysql 同构；布尔以 INTEGER 存储）。
#[derive(Debug, sqlx::FromRow)]
struct PasskeyListRow {
    id: String,
    name: String,
    aaguid: Option<String>,
    backup_eligible: i64,
    backed_up: i64,
    created_at: i64,
    last_used_at: Option<i64>,
}

/// 列出用户全部未撤销 Passkey（按注册时间倒序）。
pub async fn list_passkeys(
    pool: &DatabasePool,
    user_id: &str,
) -> Result<Vec<PasskeyInfo>, sqlx::Error> {
    let rows: Vec<PasskeyListRow> = match pool {
        Either::Left(p) => {
            sqlx::query_as(
                "SELECT id, name, aaguid, backup_eligible, backed_up, created_at, last_used_at
                 FROM passkey_credentials WHERE user_id = ? AND revoked_at IS NULL
                 ORDER BY created_at DESC",
            )
            .bind(user_id)
            .fetch_all(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_as(
                "SELECT id, name, aaguid, backup_eligible, backed_up, created_at, last_used_at
                 FROM passkey_credentials WHERE user_id = ? AND revoked_at IS NULL
                 ORDER BY created_at DESC",
            )
            .bind(user_id)
            .fetch_all(p)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| PasskeyInfo {
            id: row.id,
            name: row.name,
            aaguid: row.aaguid,
            backup_eligible: row.backup_eligible != 0,
            backed_up: row.backed_up != 0,
            created_at: row.created_at,
            last_used_at: row.last_used_at,
        })
        .collect())
}

/// 撤销指定 Passkey（本人；返回是否确实撤销了有效凭据）。
pub async fn revoke_passkey(
    pool: &DatabasePool,
    user_id: &str,
    id: &str,
) -> Result<bool, sqlx::Error> {
    let now = now_millis();
    let affected = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE passkey_credentials SET revoked_at = ?
             WHERE id = ? AND user_id = ? AND revoked_at IS NULL",
        )
        .bind(now)
        .bind(id)
        .bind(user_id)
        .execute(p)
        .await?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE passkey_credentials SET revoked_at = ?
             WHERE id = ? AND user_id = ? AND revoked_at IS NULL",
        )
        .bind(now)
        .bind(id)
        .bind(user_id)
        .execute(p)
        .await?
        .rows_affected(),
    };
    Ok(affected == 1)
}

// ─────────────────────────── 凭据加载 ───────────────────────────

/// 已存储的 Passkey（反序列化后的 webauthn-rs 结构 + 展示字段）。
#[derive(Debug, Clone)]
struct StoredPasskey {
    /// 内部记录 ID。
    id: String,
    passkey: Passkey,
}

/// 加载用户全部未撤销 Passkey 并反序列化。
async fn load_active_passkeys(
    pool: &DatabasePool,
    user_id: &str,
) -> Result<Vec<StoredPasskey>, PasskeyError> {
    let rows: Vec<(String, String)> = match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT id, credential_json FROM passkey_credentials
             WHERE user_id = ? AND revoked_at IS NULL ORDER BY created_at ASC",
        )
        .bind(user_id)
        .fetch_all(p)
        .await
        .map_err(PasskeyError::from)?,
        Either::Right(p) => sqlx::query_as(
            "SELECT id, credential_json FROM passkey_credentials
             WHERE user_id = ? AND revoked_at IS NULL ORDER BY created_at ASC",
        )
        .bind(user_id)
        .fetch_all(p)
        .await
        .map_err(PasskeyError::from)?,
    };
    rows.into_iter()
        .map(|(id, json)| {
            let passkey: Passkey = serde_json::from_str(&json)
                .map_err(|e| PasskeyError::Serialization(format!("credential {id}: {e}")))?;
            Ok(StoredPasskey { id, passkey })
        })
        .collect()
}

// ─────────────────────────── challenge 状态管理 ───────────────────────────

/// 机会主义清理过期 challenge（写入新 challenge 前顺手 GC）。
async fn gc_webauthn_challenges(pool: &DatabasePool) -> Result<(), sqlx::Error> {
    let cutoff = now_millis() - PASSKEY_CHALLENGE_TTL_MS;
    match pool {
        Either::Left(p) => {
            sqlx::query("DELETE FROM webauthn_challenges WHERE expires_at < ?")
                .bind(cutoff)
                .execute(p)
                .await?;
        }
        Either::Right(p) => {
            sqlx::query("DELETE FROM webauthn_challenges WHERE expires_at < ?")
                .bind(cutoff)
                .execute(p)
                .await?;
        }
    }
    Ok(())
}

/// 使同一绑定（purpose + user_id / mfa_challenge_hash）的既有未消费 challenge 失效，
/// 再写入新 challenge state（保证同一绑定至多一个进行中的 challenge）。
async fn store_webauthn_challenge(
    pool: &DatabasePool,
    purpose: &str,
    user_id: Option<&str>,
    mfa_challenge_hash: Option<&str>,
    state: &impl serde::Serialize,
) -> Result<(), PasskeyError> {
    gc_webauthn_challenges(pool)
        .await
        .map_err(PasskeyError::from)?;
    let state_json =
        serde_json::to_string(state).map_err(|e| PasskeyError::Serialization(e.to_string()))?;
    let now = now_millis();
    let id = Uuid::now_v7().to_string();

    // 失效旧 challenge（同绑定）
    match (user_id, mfa_challenge_hash) {
        (Some(uid), _) => {
            invalidate_by(pool, purpose, Some(uid), None)
                .await
                .map_err(PasskeyError::from)?;
        }
        (None, Some(hash)) => {
            invalidate_by(pool, purpose, None, Some(hash))
                .await
                .map_err(PasskeyError::from)?;
        }
        _ => {}
    }

    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO webauthn_challenges
                 (id, purpose, user_id, mfa_challenge_hash, state_json, created_at, expires_at, consumed_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, NULL)",
            )
            .bind(&id)
            .bind(purpose)
            .bind(user_id)
            .bind(mfa_challenge_hash)
            .bind(&state_json)
            .bind(now)
            .bind(now + PASSKEY_CHALLENGE_TTL_MS)
            .execute(p)
            .await
            .map_err(PasskeyError::from)?;
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO webauthn_challenges
                 (id, purpose, user_id, mfa_challenge_hash, state_json, created_at, expires_at, consumed_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, NULL)",
            )
            .bind(&id)
            .bind(purpose)
            .bind(user_id)
            .bind(mfa_challenge_hash)
            .bind(&state_json)
            .bind(now)
            .bind(now + PASSKEY_CHALLENGE_TTL_MS)
            .execute(p)
            .await
            .map_err(PasskeyError::from)?;
        }
    }
    Ok(())
}

/// 使同绑定的未消费 challenge 全部失效（读旧值前先消耗，避免堆积）。
async fn invalidate_by(
    pool: &DatabasePool,
    purpose: &str,
    user_id: Option<&str>,
    mfa_challenge_hash: Option<&str>,
) -> Result<(), sqlx::Error> {
    match (user_id, mfa_challenge_hash) {
        (Some(uid), _) => match pool {
            Either::Left(p) => {
                sqlx::query(
                    "UPDATE webauthn_challenges SET consumed_at = ?
                     WHERE purpose = ? AND user_id = ? AND consumed_at IS NULL",
                )
                .bind(now_millis())
                .bind(purpose)
                .bind(uid)
                .execute(p)
                .await?;
            }
            Either::Right(p) => {
                sqlx::query(
                    "UPDATE webauthn_challenges SET consumed_at = ?
                     WHERE purpose = ? AND user_id = ? AND consumed_at IS NULL",
                )
                .bind(now_millis())
                .bind(purpose)
                .bind(uid)
                .execute(p)
                .await?;
            }
        },
        _ => match pool {
            Either::Left(p) => {
                sqlx::query(
                    "UPDATE webauthn_challenges SET consumed_at = ?
                     WHERE purpose = ? AND mfa_challenge_hash = ? AND consumed_at IS NULL",
                )
                .bind(now_millis())
                .bind(purpose)
                .bind(mfa_challenge_hash.unwrap_or_default())
                .execute(p)
                .await?;
            }
            Either::Right(p) => {
                sqlx::query(
                    "UPDATE webauthn_challenges SET consumed_at = ?
                     WHERE purpose = ? AND mfa_challenge_hash = ? AND consumed_at IS NULL",
                )
                .bind(now_millis())
                .bind(purpose)
                .bind(mfa_challenge_hash.unwrap_or_default())
                .execute(p)
                .await?;
            }
        },
    }
    Ok(())
}

/// 原子读取并消费一个 challenge state（一次性：`consumed_at` 置位成功才算）。
///
/// 返回 `None`：不存在 / 已消费 / 已过期（统一，防枚举）。
async fn take_webauthn_challenge(
    pool: &DatabasePool,
    purpose: &str,
    user_id: Option<&str>,
    mfa_challenge_hash: Option<&str>,
) -> Result<Option<String>, PasskeyError> {
    let now = now_millis();
    let row: Option<(String, String)> = match (user_id, mfa_challenge_hash) {
        (Some(uid), _) => match pool {
            Either::Left(p) => sqlx::query_as(
                "SELECT id, state_json FROM webauthn_challenges
                 WHERE purpose = ? AND user_id = ? AND consumed_at IS NULL AND expires_at > ?",
            )
            .bind(purpose)
            .bind(uid)
            .bind(now)
            .fetch_optional(p)
            .await
            .map_err(PasskeyError::from)?,
            Either::Right(p) => sqlx::query_as(
                "SELECT id, state_json FROM webauthn_challenges
                 WHERE purpose = ? AND user_id = ? AND consumed_at IS NULL AND expires_at > ?",
            )
            .bind(purpose)
            .bind(uid)
            .bind(now)
            .fetch_optional(p)
            .await
            .map_err(PasskeyError::from)?,
        },
        _ => match pool {
            Either::Left(p) => sqlx::query_as(
                "SELECT id, state_json FROM webauthn_challenges
                 WHERE purpose = ? AND mfa_challenge_hash = ? AND consumed_at IS NULL AND expires_at > ?",
            )
            .bind(purpose)
            .bind(mfa_challenge_hash.unwrap_or_default())
            .bind(now)
            .fetch_optional(p)
            .await
            .map_err(PasskeyError::from)?,
            Either::Right(p) => sqlx::query_as(
                "SELECT id, state_json FROM webauthn_challenges
                 WHERE purpose = ? AND mfa_challenge_hash = ? AND consumed_at IS NULL AND expires_at > ?",
            )
            .bind(purpose)
            .bind(mfa_challenge_hash.unwrap_or_default())
            .bind(now)
            .fetch_optional(p)
            .await
            .map_err(PasskeyError::from)?,
        },
    };
    let Some((id, state_json)) = row else {
        return Ok(None);
    };
    // 原子消费（并发同 challenge 恰好一个成功）
    let consumed = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE webauthn_challenges SET consumed_at = ?
             WHERE id = ? AND consumed_at IS NULL AND expires_at > ?",
        )
        .bind(now)
        .bind(&id)
        .bind(now)
        .execute(p)
        .await
        .map_err(PasskeyError::from)?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE webauthn_challenges SET consumed_at = ?
             WHERE id = ? AND consumed_at IS NULL AND expires_at > ?",
        )
        .bind(now)
        .bind(&id)
        .bind(now)
        .execute(p)
        .await
        .map_err(PasskeyError::from)?
        .rows_affected(),
    };
    if consumed != 1 {
        return Ok(None);
    }
    Ok(Some(state_json))
}

// ─────────────────────────── 注册（enrollment） ───────────────────────────

/// 开始 Passkey 注册：返回 creation options（`{"publicKey": …}`，浏览器直接
/// 传入 `navigator.credentials.create()`），state 存 `webauthn_challenges`。
///
/// `exclude_credentials` 为该用户已注册凭据，防止把同一认证器重复注册。
pub async fn begin_passkey_registration(
    pool: &DatabasePool,
    webauthn: &Webauthn,
    user_id: &str,
    username: &str,
    display_name: &str,
) -> Result<CreationChallengeResponse, PasskeyError> {
    // 数量上限
    let count: i64 = match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM passkey_credentials WHERE user_id = ? AND revoked_at IS NULL",
        )
        .bind(user_id)
        .fetch_one(p)
        .await
        .map_err(PasskeyError::from)?,
        Either::Right(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM passkey_credentials WHERE user_id = ? AND revoked_at IS NULL",
        )
        .bind(user_id)
        .fetch_one(p)
        .await
        .map_err(PasskeyError::from)?,
    };
    if count >= PASSKEY_MAX_PER_USER {
        return Err(PasskeyError::LimitReached);
    }

    let existing = load_active_passkeys(pool, user_id).await?;
    let exclude = if existing.is_empty() {
        None
    } else {
        Some(
            existing
                .iter()
                .map(|k| k.passkey.cred_id().clone())
                .collect::<Vec<_>>(),
        )
    };
    // user_unique_id：账号 UUID（可能被认证器存储，仅用于客户端展示流；
    // 不作为主键，认证按 credential id 定位）。
    let user_unique_id = Uuid::parse_str(user_id).unwrap_or_else(|_| Uuid::now_v7());
    let (ccr, state) = webauthn
        .start_passkey_registration(user_unique_id, username, display_name, exclude)
        .map_err(|e| PasskeyError::Serialization(e.to_string()))?;

    store_webauthn_challenge(pool, PURPOSE_REGISTRATION, Some(user_id), None, &state).await?;
    Ok(ccr)
}

/// Passkey 注册成功后的投影。
#[derive(Debug, Clone, serde::Serialize)]
pub struct PasskeyCreated {
    pub info: PasskeyInfo,
}

/// 确认注册：原子消费 challenge → 校验 attestation → 落库。
///
/// `credential` 为浏览器 `navigator.credentials.create()` 的原始 JSON。
/// credential id 全局唯一（含已撤销行，唯一约束兜底）。
pub async fn confirm_passkey_registration(
    pool: &DatabasePool,
    webauthn: &Webauthn,
    user_id: &str,
    name: Option<&str>,
    credential: &serde_json::Value,
) -> Result<PasskeyInfo, PasskeyError> {
    let state_json = take_webauthn_challenge(pool, PURPOSE_REGISTRATION, Some(user_id), None)
        .await?
        .ok_or(PasskeyError::NoPendingChallenge)?;
    let state: PasskeyRegistration = serde_json::from_str(&state_json)
        .map_err(|e| PasskeyError::Serialization(e.to_string()))?;
    let reg: RegisterPublicKeyCredential =
        serde_json::from_value(credential.clone()).map_err(|_| PasskeyError::InvalidCredential)?;

    let passkey = webauthn
        .finish_passkey_registration(&reg, &state)
        .map_err(|_| PasskeyError::InvalidCredential)?;

    let name = normalize_name(name);
    // HumanBinaryData: Deref<Target=Vec<u8>>
    let credential_id: Vec<u8> = passkey.cred_id().to_vec();
    let credential_json =
        serde_json::to_string(&passkey).map_err(|e| PasskeyError::Serialization(e.to_string()))?;
    let id = Uuid::now_v7().to_string();
    let now = now_millis();

    // credential id 全局唯一：任何用户（含已撤销行）不得重复
    let dup: Option<bool> = match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM passkey_credentials WHERE credential_id = ?)",
        )
        .bind(&credential_id)
        .fetch_one(p)
        .await
        .ok(),
        Either::Right(p) => sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM passkey_credentials WHERE credential_id = ?)",
        )
        .bind(&credential_id)
        .fetch_one(p)
        .await
        .ok(),
    };
    if dup == Some(true) {
        return Err(PasskeyError::InvalidCredential);
    }

    // 并发撞唯一约束（credential id 已被他人注册）→ 统一失败。
    // 注意：不能在 match 臂间统一 QueryResult 绑定（sqlite/mysql 类型不同），
    // 错误映射在各自臂内完成。
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO passkey_credentials
                 (id, user_id, name, credential_id, credential_json, aaguid,
                  backup_eligible, backed_up, created_at, last_used_at, revoked_at)
                 VALUES (?, ?, ?, ?, ?, NULL, 0, 0, ?, NULL, NULL)",
            )
            .bind(&id)
            .bind(user_id)
            .bind(&name)
            .bind(&credential_id)
            .bind(&credential_json)
            .bind(now)
            .execute(p)
            .await
            .map_err(map_insert_err)?;
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO passkey_credentials
                 (id, user_id, name, credential_id, credential_json, aaguid,
                  backup_eligible, backed_up, created_at, last_used_at, revoked_at)
                 VALUES (?, ?, ?, ?, ?, NULL, 0, 0, ?, NULL, NULL)",
            )
            .bind(&id)
            .bind(user_id)
            .bind(&name)
            .bind(&credential_id)
            .bind(&credential_json)
            .bind(now)
            .execute(p)
            .await
            .map_err(map_insert_err)?;
        }
    }

    Ok(PasskeyInfo {
        id,
        name,
        aaguid: None,
        backup_eligible: false,
        backed_up: false,
        created_at: now,
        last_used_at: None,
    })
}

/// 规范化用户可读标签：trim、限长 64、空则默认「Passkey」。
fn normalize_name(name: Option<&str>) -> String {
    let trimmed = name.unwrap_or("").trim();
    if trimmed.is_empty() {
        "Passkey".to_owned()
    } else {
        trimmed.chars().take(64).collect()
    }
}

/// INSERT 错误映射：credential id 唯一约束冲突 → InvalidCredential（统一，防枚举）。
fn map_insert_err(e: sqlx::Error) -> PasskeyError {
    match e {
        sqlx::Error::Database(ref db) if db.is_unique_violation() => {
            PasskeyError::InvalidCredential
        }
        other => PasskeyError::from(other),
    }
}

// ─────────────────────────── 登录（authentication） ───────────────────────────

/// 读取有效 MFA login challenge 的 user_id（不消费——会话在第二步成功后才签发）。
async fn mfa_challenge_user(
    pool: &DatabasePool,
    token_hash: &str,
) -> Result<Option<String>, PasskeyError> {
    let row: Option<(String,)> = match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT user_id FROM mfa_login_challenges
             WHERE token_hash = ? AND consumed_at IS NULL AND expires_at > ?",
        )
        .bind(token_hash)
        .bind(now_millis())
        .fetch_optional(p)
        .await
        .map_err(PasskeyError::from)?,
        Either::Right(p) => sqlx::query_as(
            "SELECT user_id FROM mfa_login_challenges
             WHERE token_hash = ? AND consumed_at IS NULL AND expires_at > ?",
        )
        .bind(token_hash)
        .bind(now_millis())
        .fetch_optional(p)
        .await
        .map_err(PasskeyError::from)?,
    };
    Ok(row.map(|(user_id,)| user_id))
}

/// 开始登录断言：校验 MFA login challenge 有效 → 为该用户生成 request options
/// （allowCredentials 限定其已注册 Passkey），state 绑定该 MFA challenge 存库。
///
/// 不消费 MFA login challenge（第二步成功时由 `complete_mfa_login` 消费）。
pub async fn begin_passkey_login(
    pool: &DatabasePool,
    webauthn: &Webauthn,
    challenge_token: &str,
) -> Result<RequestChallengeResponse, PasskeyError> {
    let token_hash = hash_token(challenge_token);
    let Some(user_id) = mfa_challenge_user(pool, &token_hash).await? else {
        return Err(PasskeyError::NoPendingChallenge);
    };
    let keys = load_active_passkeys(pool, &user_id).await?;
    if keys.is_empty() {
        return Err(PasskeyError::NoCredentials);
    }
    let creds: Vec<Passkey> = keys.into_iter().map(|k| k.passkey).collect();
    let (rcr, state) = webauthn
        .start_passkey_authentication(&creds)
        .map_err(|e| PasskeyError::Serialization(e.to_string()))?;
    store_webauthn_challenge(
        pool,
        PURPOSE_AUTHENTICATION,
        None,
        Some(&token_hash),
        &state,
    )
    .await?;
    Ok(rcr)
}

/// 校验登录断言：原子消费绑定该 MFA login challenge 的 webauthn challenge →
/// 校验签名 / origin / rpIdHash / UV → 更新凭据（counter、backup flags、last_used_at）。
///
/// 任意失败统一 [`PasskeyError::InvalidCredential`]；MFA login challenge 本身
/// 无效由调用方（`complete_mfa_login`）统一处理。
pub async fn verify_passkey_login(
    pool: &DatabasePool,
    webauthn: &Webauthn,
    challenge_token: &str,
    user_id: &str,
    assertion: &serde_json::Value,
) -> Result<(), PasskeyError> {
    let token_hash = hash_token(challenge_token);

    // 1) 原子消费 webauthn challenge（一次性；失败后需重新获取 options）
    let state_json = take_webauthn_challenge(pool, PURPOSE_AUTHENTICATION, None, Some(&token_hash))
        .await?
        .ok_or(PasskeyError::NoPendingChallenge)?;

    // 2) 反序列化 state 与浏览器断言
    let state: PasskeyAuthentication = serde_json::from_str(&state_json)
        .map_err(|e| PasskeyError::Serialization(e.to_string()))?;
    let response: PublicKeyCredential =
        serde_json::from_value(assertion.clone()).map_err(|_| PasskeyError::InvalidCredential)?;

    let keys = load_active_passkeys(pool, user_id).await?;
    if keys.is_empty() {
        return Err(PasskeyError::InvalidCredential);
    }

    // 3) 校验断言（签名 / origin / rpIdHash / counter 由 webauthn-rs 完成）
    let result = webauthn
        .finish_passkey_authentication(&response, &state)
        .map_err(|_| PasskeyError::InvalidCredential)?;

    // 4) UV 必须成立（passkey 流程默认强制 UV；此处显式断言双保险）
    if !result.user_verified() {
        return Err(PasskeyError::InvalidCredential);
    }

    // 5) 更新使用的凭据（update_credential 对 cred id 不匹配的返回 None）
    let now = now_millis();
    for key in &keys {
        let mut pk = key.passkey.clone();
        if pk.update_credential(&result).is_some() {
            let json = serde_json::to_string(&pk)
                .map_err(|e| PasskeyError::Serialization(e.to_string()))?;
            update_passkey_after_auth(pool, &key.id, &json, result.backup_state(), now).await?;
            break;
        }
    }
    Ok(())
}

/// 断言成功后回写凭据：credential_json（counter 等内部状态）+ backed_up + last_used_at。
async fn update_passkey_after_auth(
    pool: &DatabasePool,
    id: &str,
    credential_json: &str,
    backed_up: bool,
    now: i64,
) -> Result<(), sqlx::Error> {
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE passkey_credentials
                 SET credential_json = ?, backed_up = ?, last_used_at = ?
                 WHERE id = ? AND revoked_at IS NULL",
            )
            .bind(credential_json)
            .bind(if backed_up { 1 } else { 0 })
            .bind(now)
            .bind(id)
            .execute(p)
            .await?;
        }
        Either::Right(p) => {
            sqlx::query(
                "UPDATE passkey_credentials
                 SET credential_json = ?, backed_up = ?, last_used_at = ?
                 WHERE id = ? AND revoked_at IS NULL",
            )
            .bind(credential_json)
            .bind(if backed_up { 1 } else { 0 })
            .bind(now)
            .bind(id)
            .execute(p)
            .await?;
        }
    }
    Ok(())
}
