//! TOTP（RFC 6238）与 MFA enrollment（M02-MFA-02）
//!
//! - `begin_enrollment`：生成 20 字节 TOTP secret（base32 展示），加密后写入
//!   `totp_credentials`（pending 行），返回二维码所需最小数据（otpauth URI）；
//! - `confirm_enrollment`：校验 6 位 code（时间窗口内 + 未重放 step）后原子
//!   启用（confirmed_at + last_accepted_step）；
//! - `cancel_enrollment`：撤销未完成的 enrollment。
//!
//! 安全约定（M02-MFA-03 形式化时间窗口与防重放）：
//! - secret 只在数据库存 AEAD 密文（`encrypted_secret`），日志/API 一律不输出
//!   明文或 code；
//! - code 校验走时间窗口 + last_accepted_step，防重放；
//! - 一个用户同一时刻至多一个启用中的 TOTP（重复启用 = 撤销旧 + 新建）。

use std::fmt;

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use hmac::{Hmac, Mac};
use rand::{rngs::OsRng, RngCore};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use sqlx::Either;
use uuid::Uuid;

use crate::db::pool::DatabasePool;

/// TOTP 时间周期（秒，RFC 6238 默认 X=30）。
pub const TOTP_PERIOD_SECS: u64 = 30;
/// 默认校验时间窗口（步）：当前步 ±1（M02-MFA-03 形式化为配置）。
pub const DEFAULT_WINDOW_STEPS: u64 = 1;
/// TOTP 位数。
pub const TOTP_DIGITS: u32 = 6;
/// TOTP secret 字节数（RFC 6238 建议 ≥ 160 bit）。
pub const TOTP_SECRET_BYTES: usize = 20;

/// base32 字母表（RFC 4648，无填充）。
const BASE32_ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

// ─────────────────────────── TOTP 原语 ───────────────────────────

/// 生成 20 字节（160 bit）TOTP secret。
pub fn generate_totp_secret() -> [u8; TOTP_SECRET_BYTES] {
    let mut bytes = [0u8; TOTP_SECRET_BYTES];
    OsRng.fill_bytes(&mut bytes);
    bytes
}

/// RFC 4648 base32 编码（无填充）。
pub fn base32_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity((bytes.len() * 8).div_ceil(5));
    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;
    for &byte in bytes {
        buffer = (buffer << 8) | byte as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(BASE32_ALPHABET[((buffer >> bits) & 0x1f) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(BASE32_ALPHABET[((buffer << (5 - bits)) & 0x1f) as usize] as char);
    }
    out
}

/// RFC 4648 base32 解码（无填充；非法字符返回 None）。
pub fn base32_decode(input: &str) -> Option<Vec<u8>> {
    let trimmed = input.trim().to_ascii_uppercase();
    let mut out = Vec::with_capacity(trimmed.len() * 5 / 8);
    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;
    for c in trimmed.chars() {
        let value = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            '2'..='7' => c as u32 - '2' as u32 + 26,
            _ => return None,
        };
        buffer = (buffer << 5) | value;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    // 剩余 bits 必须全零（标准无填充 base32）
    if bits > 0 && (buffer & ((1 << bits) - 1)) != 0 {
        return None;
    }
    Some(out)
}

/// RFC 6238 HOTP/TOTP：计算 secret 在指定 counter 的 6 位 code。
pub fn totp_at(secret: &[u8], counter: u64) -> u32 {
    let mut mac =
        <Hmac<Sha1> as Mac>::new_from_slice(secret).expect("HMAC-SHA1 accepts any key length");
    mac.update(&counter.to_be_bytes());
    let result = mac.finalize().into_bytes();
    let offset = (result[19] & 0x0f) as usize;
    let binary = ((result[offset] as u32 & 0x7f) << 24)
        | ((result[offset + 1] as u32) << 16)
        | ((result[offset + 2] as u32) << 8)
        | (result[offset + 3] as u32);
    binary % 10u32.pow(TOTP_DIGITS)
}

/// 校验 6 位 code：在当前步 ±window 内匹配则返回接受的 step；否则 None。
///
/// code 比较使用计算值而非字符串（恒定比较 6 位数值，无早期退出）。
pub fn verify_totp(secret: &[u8], code: &str, now_secs: u64, window: u64) -> Option<u64> {
    let provided = code.trim().parse::<u32>().ok()?;
    let current = now_secs / TOTP_PERIOD_SECS;
    let low = current.saturating_sub(window);
    let high = current.saturating_add(window);
    (low..=high).find(|&step| totp_at(secret, step) == provided)
}

/// otpauth URI（二维码所需最小数据，RFC 6238 / Google Authenticator 兼容）。
pub fn otpauth_uri(issuer: &str, account: &str, secret_b32: &str) -> String {
    format!(
        "otpauth://totp/{}:{}?secret={}&issuer={}&algorithm=SHA1&digits={}&period={}",
        percent_encode(issuer),
        percent_encode(account),
        secret_b32,
        percent_encode(issuer),
        TOTP_DIGITS,
        TOTP_PERIOD_SECS,
    )
}

/// 极简 percent-encode（RFC 3986 保留字符）。
fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

// ─────────────────────────── 加密（静态） ───────────────────────────

/// 将任意长度密钥材料派生为 AES-256 密钥（SHA-256）。
fn encryption_key_32(key_material: &[u8]) -> [u8; 32] {
    let digest = Sha256::digest(key_material);
    let mut key = [0u8; 32];
    key.copy_from_slice(&digest);
    key
}

/// 加密 TOTP secret（AES-256-GCM）：返回 hex（布局 nonce(12) || 密文含 tag）。
pub fn encrypt_secret(key_material: &[u8], secret: &[u8]) -> String {
    let cipher =
        Aes256Gcm::new_from_slice(&encryption_key_32(key_material)).expect("32 字节 AES-256 密钥");
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), secret)
        .expect("AES-256-GCM 加密");
    let mut blob = Vec::with_capacity(nonce_bytes.len() + ciphertext.len());
    blob.extend_from_slice(&nonce_bytes);
    blob.extend_from_slice(&ciphertext);
    hex::encode(blob)
}

/// 解密 TOTP secret；密钥错误或密文损坏返回 None。
pub fn decrypt_secret(key_material: &[u8], blob_hex: &str) -> Option<Vec<u8>> {
    let blob = hex::decode(blob_hex).ok()?;
    if blob.len() < 12 + 16 {
        return None;
    }
    let (nonce_bytes, ciphertext) = blob.split_at(12);
    let cipher = Aes256Gcm::new_from_slice(&encryption_key_32(key_material)).ok()?;
    cipher
        .decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
        .ok()
}

// ─────────────────────────── enrollment 服务 ───────────────────────────

/// 二维码所需最小数据（M02-MFA-02）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct TotpChallenge {
    /// base32 编码的 TOTP secret（供用户手工录入或渲染二维码）。
    pub secret_base32: String,
    /// otpauth URI（二维码内容）。
    pub otpauth_uri: String,
    /// 发行方。
    pub issuer: String,
    /// 账号标识（通常是邮箱/用户名）。
    pub account: String,
}

/// MFA 服务错误。
#[derive(Debug)]
pub enum MfaError {
    /// 不存在未完成的 enrollment。
    NoPendingEnrollment,
    /// 已确认启用（重复确认）。
    AlreadyConfirmed,
    /// code 无效或已重放。
    InvalidCode,
    /// 用户未启用 TOTP。
    TotpNotEnabled,
    /// secret 解密失败（密钥轮换或数据损坏）。
    Encryption,
    /// 数据库错误。
    Database(String),
}

impl fmt::Display for MfaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MfaError::NoPendingEnrollment => write!(f, "no pending totp enrollment"),
            MfaError::AlreadyConfirmed => write!(f, "totp already confirmed"),
            MfaError::InvalidCode => write!(f, "invalid totp code"),
            MfaError::TotpNotEnabled => write!(f, "totp is not enabled"),
            MfaError::Encryption => write!(f, "totp secret decryption failed"),
            MfaError::Database(e) => write!(f, "database error: {e}"),
        }
    }
}

impl std::error::Error for MfaError {}

/// 开始 TOTP enrollment：撤销该用户既有 TOTP（重复启用 = 撤销旧 + 新建），
/// 插入 pending 行（加密 secret），返回挑战数据。
pub async fn begin_enrollment(
    pool: &DatabasePool,
    user_id: &str,
    issuer: &str,
    account: &str,
    encryption_key: &[u8],
) -> Result<TotpChallenge, MfaError> {
    let secret = generate_totp_secret();
    let secret_b32 = base32_encode(&secret);
    let encrypted = encrypt_secret(encryption_key, &secret);
    let id = Uuid::now_v7().to_string();
    let now = crate::outbox::now_millis();

    revoke_all_totp(pool, user_id).await?;

    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO totp_credentials
                     (id, user_id, encrypted_secret, last_accepted_step, created_at, confirmed_at, revoked_at)
                 VALUES (?, ?, ?, 0, ?, NULL, NULL)",
            )
            .bind(&id)
            .bind(user_id)
            .bind(&encrypted)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| MfaError::Database(e.to_string()))?;
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO totp_credentials
                     (id, user_id, encrypted_secret, last_accepted_step, created_at, confirmed_at, revoked_at)
                 VALUES (?, ?, ?, 0, ?, NULL, NULL)",
            )
            .bind(&id)
            .bind(user_id)
            .bind(&encrypted)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| MfaError::Database(e.to_string()))?;
        }
    }

    Ok(TotpChallenge {
        otpauth_uri: otpauth_uri(issuer, account, &secret_b32),
        secret_base32: secret_b32,
        issuer: issuer.to_string(),
        account: account.to_string(),
    })
}

/// 确认 enrollment：校验 code（时间窗口内 + 未重放 step）后原子启用。
pub async fn confirm_enrollment(
    pool: &DatabasePool,
    user_id: &str,
    code: &str,
    encryption_key: &[u8],
    now_secs: u64,
) -> Result<(), MfaError> {
    let Some(row) = load_pending(pool, user_id).await? else {
        // 区分“从未 enrollment”与“已确认启用”（重复确认）
        if has_confirmed_totp(pool, user_id).await? {
            return Err(MfaError::AlreadyConfirmed);
        }
        return Err(MfaError::NoPendingEnrollment);
    };
    let secret =
        decrypt_secret(encryption_key, &row.encrypted_secret).ok_or(MfaError::Encryption)?;
    let Some(step) = verify_totp(&secret, code, now_secs, DEFAULT_WINDOW_STEPS) else {
        return Err(MfaError::InvalidCode);
    };
    if step <= row.last_accepted_step as u64 {
        // 已接受过该 step（重放）
        return Err(MfaError::InvalidCode);
    }

    let now = crate::outbox::now_millis();
    let affected = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE totp_credentials SET confirmed_at = ?, last_accepted_step = ?
             WHERE id = ? AND confirmed_at IS NULL AND revoked_at IS NULL",
        )
        .bind(now)
        .bind(step as i64)
        .bind(&row.id)
        .execute(p)
        .await
        .map_err(|e| MfaError::Database(e.to_string()))?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE totp_credentials SET confirmed_at = ?, last_accepted_step = ?
             WHERE id = ? AND confirmed_at IS NULL AND revoked_at IS NULL",
        )
        .bind(now)
        .bind(step as i64)
        .bind(&row.id)
        .execute(p)
        .await
        .map_err(|e| MfaError::Database(e.to_string()))?
        .rows_affected(),
    };
    if affected != 1 {
        return Err(MfaError::AlreadyConfirmed);
    }
    Ok(())
}

/// 取消未完成的 enrollment：撤销 pending 行。返回是否确实取消了。
pub async fn cancel_enrollment(pool: &DatabasePool, user_id: &str) -> Result<bool, MfaError> {
    let now = crate::outbox::now_millis();
    let affected = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE totp_credentials SET revoked_at = ?
             WHERE user_id = ? AND confirmed_at IS NULL AND revoked_at IS NULL",
        )
        .bind(now)
        .bind(user_id)
        .execute(p)
        .await
        .map_err(|e| MfaError::Database(e.to_string()))?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE totp_credentials SET revoked_at = ?
             WHERE user_id = ? AND confirmed_at IS NULL AND revoked_at IS NULL",
        )
        .bind(now)
        .bind(user_id)
        .execute(p)
        .await
        .map_err(|e| MfaError::Database(e.to_string()))?
        .rows_affected(),
    };
    Ok(affected > 0)
}

/// MFA 登录验证结果。
#[derive(Debug, Clone, Copy)]
pub struct VerifyTotpOutcome {
    /// 接受的时间 step。
    pub step: u64,
}

/// MFA 登录验证（M02-MFA-03）：校验已启用 TOTP 的 6 位 code。
///
/// - 允许时间窗口：当前步 ±`window`（容忍客户端/服务器时钟漂移）；
/// - 防重放：接受的 step 必须 > `last_accepted_step`，且原子更新
///   （`WHERE last_accepted_step < ?`）——同一 step 并发验证只有一个成功；
/// - 全程不记录 code 或 secret（本模块无任何日志输出）。
pub async fn verify_totp_login(
    pool: &DatabasePool,
    user_id: &str,
    code: &str,
    encryption_key: &[u8],
    now_secs: u64,
    window: u64,
) -> Result<VerifyTotpOutcome, MfaError> {
    let row = load_confirmed(pool, user_id)
        .await?
        .ok_or(MfaError::TotpNotEnabled)?;
    let secret =
        decrypt_secret(encryption_key, &row.encrypted_secret).ok_or(MfaError::Encryption)?;
    let Some(step) = verify_totp(&secret, code, now_secs, window) else {
        return Err(MfaError::InvalidCode);
    };
    if step <= row.last_accepted_step as u64 {
        // 已接受过该 step（重放）
        return Err(MfaError::InvalidCode);
    }

    // 原子推进 last_accepted_step：并发同 step 只有一个成功
    let affected = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE totp_credentials SET last_accepted_step = ?
             WHERE id = ? AND last_accepted_step < ? AND revoked_at IS NULL",
        )
        .bind(step as i64)
        .bind(&row.id)
        .bind(step as i64)
        .execute(p)
        .await
        .map_err(|e| MfaError::Database(e.to_string()))?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE totp_credentials SET last_accepted_step = ?
             WHERE id = ? AND last_accepted_step < ? AND revoked_at IS NULL",
        )
        .bind(step as i64)
        .bind(&row.id)
        .bind(step as i64)
        .execute(p)
        .await
        .map_err(|e| MfaError::Database(e.to_string()))?
        .rows_affected(),
    };
    if affected != 1 {
        // 并发竞争：该 step 已被消费
        return Err(MfaError::InvalidCode);
    }
    Ok(VerifyTotpOutcome { step })
}

/// 撤销用户全部 TOTP（含已启用与 pending），用于重新 enrollment。
async fn revoke_all_totp(pool: &DatabasePool, user_id: &str) -> Result<(), MfaError> {
    let now = crate::outbox::now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE totp_credentials SET revoked_at = ? WHERE user_id = ? AND revoked_at IS NULL",
            )
            .bind(now)
            .bind(user_id)
            .execute(p)
            .await
            .map_err(|e| MfaError::Database(e.to_string()))?;
        }
        Either::Right(p) => {
            sqlx::query(
                "UPDATE totp_credentials SET revoked_at = ? WHERE user_id = ? AND revoked_at IS NULL",
            )
            .bind(now)
            .bind(user_id)
            .execute(p)
            .await
            .map_err(|e| MfaError::Database(e.to_string()))?;
        }
    }
    Ok(())
}

/// 数据库行结构
#[derive(sqlx::FromRow)]
struct PendingTotpRow {
    id: String,
    encrypted_secret: String,
    last_accepted_step: i64,
}

/// 加载用户最新 pending enrollment（未确认未撤销）。
async fn load_pending(
    pool: &DatabasePool,
    user_id: &str,
) -> Result<Option<PendingTotpRow>, MfaError> {
    match pool {
        Either::Left(p) => sqlx::query_as::<_, PendingTotpRow>(
            "SELECT id, encrypted_secret, last_accepted_step FROM totp_credentials
             WHERE user_id = ? AND confirmed_at IS NULL AND revoked_at IS NULL
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(user_id)
        .fetch_optional(p)
        .await
        .map_err(|e| MfaError::Database(e.to_string())),
        Either::Right(p) => sqlx::query_as::<_, PendingTotpRow>(
            "SELECT id, encrypted_secret, last_accepted_step FROM totp_credentials
             WHERE user_id = ? AND confirmed_at IS NULL AND revoked_at IS NULL
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(user_id)
        .fetch_optional(p)
        .await
        .map_err(|e| MfaError::Database(e.to_string())),
    }
}

/// 用户是否存在启用中的 TOTP（已确认未撤销）。
async fn has_confirmed_totp(pool: &DatabasePool, user_id: &str) -> Result<bool, MfaError> {
    match pool {
        Either::Left(p) => sqlx::query_scalar::<_, i64>(
            "SELECT EXISTS(SELECT 1 FROM totp_credentials
              WHERE user_id = ? AND confirmed_at IS NOT NULL AND revoked_at IS NULL)",
        )
        .bind(user_id)
        .fetch_one(p)
        .await
        .map(|n| n != 0)
        .map_err(|e| MfaError::Database(e.to_string())),
        Either::Right(p) => sqlx::query_scalar::<_, i64>(
            "SELECT EXISTS(SELECT 1 FROM totp_credentials
              WHERE user_id = ? AND confirmed_at IS NOT NULL AND revoked_at IS NULL)",
        )
        .bind(user_id)
        .fetch_one(p)
        .await
        .map(|n| n != 0)
        .map_err(|e| MfaError::Database(e.to_string())),
    }
}

/// 加载用户最新已确认 TOTP（未撤销）。
async fn load_confirmed(
    pool: &DatabasePool,
    user_id: &str,
) -> Result<Option<ConfirmedTotpRow>, MfaError> {
    match pool {
        Either::Left(p) => sqlx::query_as::<_, ConfirmedTotpRow>(
            "SELECT id, encrypted_secret, last_accepted_step FROM totp_credentials
             WHERE user_id = ? AND confirmed_at IS NOT NULL AND revoked_at IS NULL
             ORDER BY confirmed_at DESC LIMIT 1",
        )
        .bind(user_id)
        .fetch_optional(p)
        .await
        .map_err(|e| MfaError::Database(e.to_string())),
        Either::Right(p) => sqlx::query_as::<_, ConfirmedTotpRow>(
            "SELECT id, encrypted_secret, last_accepted_step FROM totp_credentials
             WHERE user_id = ? AND confirmed_at IS NOT NULL AND revoked_at IS NULL
             ORDER BY confirmed_at DESC LIMIT 1",
        )
        .bind(user_id)
        .fetch_optional(p)
        .await
        .map_err(|e| MfaError::Database(e.to_string())),
    }
}

/// 已确认 TOTP 行结构。
#[derive(sqlx::FromRow)]
struct ConfirmedTotpRow {
    id: String,
    encrypted_secret: String,
    last_accepted_step: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 6238 附录 B（SHA-1，T0=0，X=30，6 位截断）测试向量。
    #[test]
    fn totp_matches_rfc6238_test_vectors() {
        let secret = b"12345678901234567890";
        // counter = T / 30
        let cases = [
            (59u64 / 30, 287_082u32),
            (1_111_111_109u64 / 30, 81_804),
            (1_111_111_111u64 / 30, 50_471),
            (1_234_567_890u64 / 30, 5_924),
            (2_000_000_000u64 / 30, 279_037),
            (20_000_000_000u64 / 30, 353_130),
        ];
        for (counter, expected) in cases {
            assert_eq!(
                totp_at(secret, counter),
                expected,
                "counter={counter} 应匹配 RFC 6238 向量"
            );
        }
    }

    #[test]
    fn base32_roundtrip() {
        // RFC 4648 §10 示例：foobar → MZXW6YTBOI（无填充）
        assert_eq!(base32_encode(b"foobar"), "MZXW6YTBOI");
        assert_eq!(base32_decode("MZXW6YTBOI").unwrap(), b"foobar".to_vec());

        // 任意字节往返
        let bytes = [0x00u8, 0x66, 0xff, 0x2e, 0x1c, 0x90, 0x5a];
        let encoded = base32_encode(&bytes);
        assert_eq!(base32_decode(&encoded).unwrap(), bytes.to_vec());

        // 非法字符拒绝
        assert_eq!(base32_decode("MZXW6YTBO0"), None, "0 不在 base32 字母表");
        assert_eq!(base32_decode("MZXW6YTB-O"), None);
    }

    #[test]
    fn generate_secret_is_20_bytes_and_32_base32_chars() {
        let secret = generate_totp_secret();
        assert_eq!(secret.len(), 20);
        assert_eq!(base32_encode(&secret).len(), 32);
    }

    #[test]
    fn verify_totp_accepts_within_window_and_rejects_wrong() {
        let secret = b"verify-secret-key-123";
        let now = 1_700_000_000u64;
        let current_step = now / TOTP_PERIOD_SECS;
        // 生成当前步 code → 校验通过
        let code = format!("{:06}", totp_at(secret, current_step));
        assert_eq!(verify_totp(secret, &code, now, 1), Some(current_step));
        // 生成 ±1 步 code → 窗口内通过
        assert_eq!(
            verify_totp(
                secret,
                &format!("{:06}", totp_at(secret, current_step + 1)),
                now,
                1
            ),
            Some(current_step + 1)
        );
        // 错误 code → None
        assert_eq!(verify_totp(secret, "000000", now, 1), None);
        // 非数字 → None
        assert_eq!(verify_totp(secret, "abcdef", now, 1), None);
    }

    #[test]
    fn encrypt_decrypt_roundtrip_and_wrong_key_fails() {
        let secret = b"my-totp-secret";
        let blob = encrypt_secret(b"correct-key-material", secret);
        assert!(!blob.contains("my-totp-secret"), "密文不得含明文");
        assert_eq!(
            decrypt_secret(b"correct-key-material", &blob).unwrap(),
            secret.to_vec()
        );
        assert_eq!(
            decrypt_secret(b"wrong-key", &blob),
            None,
            "错误密钥必须解密失败"
        );
        assert_eq!(decrypt_secret(b"key", "not-hex"), None);
        assert_eq!(decrypt_secret(b"key", "abcd"), None, "过短 blob 必须失败");
    }

    #[test]
    fn otpauth_uri_contains_minimal_qr_data() {
        let uri = otpauth_uri("BBLBB", "alice@example.com", "JBSWY3DPEHPK3PXP");
        assert!(uri.starts_with("otpauth://totp/"));
        assert!(uri.contains("secret=JBSWY3DPEHPK3PXP"));
        assert!(uri.contains("issuer=BBLBB"));
        assert!(uri.contains("algorithm=SHA1"));
        assert!(uri.contains("digits=6"));
        assert!(uri.contains("period=30"));
    }
}
