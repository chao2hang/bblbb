//! OIDC 签名密钥管理（M11-CONSENT-03/04）。
//!
//! - RSA-2048 私钥以 PKCS#8 PEM 序列化，用主密钥 AES-256-GCM 加密后存入
//!   `oauth_signing_keys.private_key_ciphertext`（数据库不落明文私钥）；
//! - 公钥以 JWK JSON 存入 `public_jwk_json`，JWKS 端点动态输出
//!   active + retiring 密钥；轮换先发布新 key、再切换 active；
//! - 主密钥不可用（空/错误）时**不得**临时生成新 key 掩盖丢失——直接失败
//!   （生产 readiness 语义，docs/AUTH-OIDC.md §12）；
//! - 旧 key 保留至所有签发 ID Token 过期 + 安全余量后由
//!   [`purge_expired_keys`] 移除。

use rand::rngs::OsRng;
use rsa::pkcs8::{DecodePrivateKey, EncodePrivateKey, LineEnding};
use rsa::traits::PublicKeyParts;
use rsa::RsaPrivateKey;
use serde_json::{json, Value};
use sqlx::Either;

use super::{now_millis, OidcError};

/// 加密签名密钥行（`oauth_signing_keys`）。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SigningKeyRow {
    pub kid: String,
    pub status: String,
    pub private_key_ciphertext: String,
    pub public_jwk_json: String,
    pub created_at: i64,
    pub retired_at: Option<i64>,
    pub key_audit_json: Option<String>,
}

/// 生成 RSA-2048 密钥对并返回公钥 JWK（`use=sign` / `alg=RS256`）。
pub fn generate_key_pair() -> Result<(RsaPrivateKey, Value), OidcError> {
    let priv_key = RsaPrivateKey::new(&mut OsRng, 2048)
        .map_err(|e| OidcError::ServerError(format!("RSA key generation failed: {e}")))?;
    let pub_key = rsa::RsaPublicKey::from(&priv_key);
    let jwk = json!({
        "kty": "RSA",
        "use": "sig",
        "alg": "RS256",
        "n": super::protocol::base64url_encode(&pub_key.n().to_bytes_be()),
        "e": super::protocol::base64url_encode(&pub_key.e().to_bytes_be()),
    });
    Ok((priv_key, jwk))
}

/// 用主密钥加密 RSA 私钥（返回 hex：nonce(12) || AES-256-GCM 密文）。
pub fn encrypt_private_key(
    master_key: &[u8],
    priv_key: &RsaPrivateKey,
) -> Result<String, OidcError> {
    if master_key.is_empty() {
        return Err(OidcError::ServerError(
            "OIDC key encryption key is not configured".to_string(),
        ));
    }
    let pem = priv_key
        .to_pkcs8_pem(LineEnding::LF)
        .map_err(|e| OidcError::ServerError(format!("PKCS#8 encoding failed: {e}")))?;
    Ok(crate::auth::mfa::encrypt_secret(master_key, pem.as_bytes()))
}

/// 用主密钥解密 RSA 私钥；失败（主密钥缺失/错误）时返回错误，不掩盖丢失。
pub fn decrypt_private_key(
    master_key: &[u8],
    ciphertext: &str,
) -> Result<RsaPrivateKey, OidcError> {
    let pem_bytes = crate::auth::mfa::decrypt_secret(master_key, ciphertext).ok_or_else(|| {
        OidcError::ServerError(
            "OIDC signing key cannot be decrypted: master key unavailable or wrong".to_string(),
        )
    })?;
    let pem = String::from_utf8(pem_bytes)
        .map_err(|_| OidcError::ServerError("decrypted key is not UTF-8".to_string()))?;
    RsaPrivateKey::from_pkcs8_pem(&pem).map_err(|e| {
        OidcError::ServerError(format!("decrypted key is not a valid PKCS#8 key: {e}"))
    })
}

async fn insert_key_sqlite<'e, E>(
    exec: E,
    kid: &str,
    ciphertext: &str,
    jwk_json: &str,
    audit: &str,
    now: i64,
) -> Result<(), sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    sqlx::query(
        "INSERT INTO oauth_signing_keys (kid, status, private_key_ciphertext, public_jwk_json, created_at, key_audit_json)
         VALUES (?, 'active', ?, ?, ?, ?)",
    )
    .bind(kid)
    .bind(ciphertext)
    .bind(jwk_json)
    .bind(now)
    .bind(audit)
    .execute(exec)
    .await?;
    Ok(())
}

async fn insert_key_mysql<'e, E>(
    exec: E,
    kid: &str,
    ciphertext: &str,
    jwk_json: &str,
    audit: &str,
    now: i64,
) -> Result<(), sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::MySql>,
{
    sqlx::query(
        "INSERT INTO oauth_signing_keys (kid, status, private_key_ciphertext, public_jwk_json, created_at, key_audit_json)
         VALUES (?, 'active', ?, ?, ?, ?)",
    )
    .bind(kid)
    .bind(ciphertext)
    .bind(jwk_json)
    .bind(now)
    .bind(audit)
    .execute(exec)
    .await?;
    Ok(())
}

/// 确保存在一个 active 签名密钥；不存在则生成（首次供给）。
///
/// 已存在但无法用主密钥解密 → 返回 `ServerError`（不临时生成新 key 掩盖丢失）。
pub async fn active_signing_key(
    pool: &crate::db::DatabasePool,
    master_key: &[u8],
) -> Result<(SigningKeyRow, RsaPrivateKey), OidcError> {
    let row: Option<SigningKeyRow> = match pool {
        Either::Left(p) => {
            let found = sqlx::query_as::<_, SigningKeyRow>(
                "SELECT kid, status, private_key_ciphertext, public_jwk_json, created_at, retired_at, key_audit_json
                 FROM oauth_signing_keys WHERE status = 'active' ORDER BY created_at DESC LIMIT 1",
            )
            .fetch_optional(p)
            .await?;
            found
        }
        Either::Right(p) => {
            let found = sqlx::query_as::<_, SigningKeyRow>(
                "SELECT kid, status, private_key_ciphertext, public_jwk_json, created_at, retired_at, key_audit_json
                 FROM oauth_signing_keys WHERE status = 'active' ORDER BY created_at DESC LIMIT 1",
            )
            .fetch_optional(p)
            .await?;
            found
        }
    };

    if let Some(row) = row {
        let priv_key = decrypt_private_key(master_key, &row.private_key_ciphertext)?;
        return Ok((row, priv_key));
    }

    // 首次供给：生成并插入。
    if master_key.is_empty() {
        return Err(OidcError::ServerError(
            "OIDC key encryption key is not configured; refusing to generate an unprotected signing key"
                .to_string(),
        ));
    }
    let (priv_key, jwk) = generate_key_pair()?;
    let kid = uuid::Uuid::now_v7().to_string();
    let ciphertext = encrypt_private_key(master_key, &priv_key)?;
    let audit = serde_json::to_string(&[json!({
        "at": now_millis(),
        "actor": "system",
        "action": "generate",
        "reason": "first key provisioning",
    })])
    .unwrap_or_default();
    match pool {
        Either::Left(p) => {
            insert_key_sqlite(p, &kid, &ciphertext, &jwk.to_string(), &audit, now_millis()).await?
        }
        Either::Right(p) => {
            insert_key_mysql(p, &kid, &ciphertext, &jwk.to_string(), &audit, now_millis()).await?
        }
    }
    let row = SigningKeyRow {
        kid,
        status: "active".to_string(),
        private_key_ciphertext: ciphertext,
        public_jwk_json: jwk.to_string(),
        created_at: now_millis(),
        retired_at: None,
        key_audit_json: Some(audit),
    };
    Ok((row, priv_key))
}

/// 按 kid 取密钥（用于验证旧 ID Token / id_token_hint）。
pub async fn signing_key_by_kid(
    pool: &crate::db::DatabasePool,
    kid: &str,
    master_key: &[u8],
) -> Result<Option<(SigningKeyRow, RsaPrivateKey)>, OidcError> {
    let row: Option<SigningKeyRow> = match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, SigningKeyRow>(
                "SELECT kid, status, private_key_ciphertext, public_jwk_json, created_at, retired_at, key_audit_json
                 FROM oauth_signing_keys WHERE kid = ?",
            )
            .bind(kid)
            .fetch_optional(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_as::<_, SigningKeyRow>(
                "SELECT kid, status, private_key_ciphertext, public_jwk_json, created_at, retired_at, key_audit_json
                 FROM oauth_signing_keys WHERE kid = ?",
            )
            .bind(kid)
            .fetch_optional(p)
            .await?
        }
    };
    let Some(row) = row else { return Ok(None) };
    let priv_key = decrypt_private_key(master_key, &row.private_key_ciphertext)?;
    Ok(Some((row, priv_key)))
}

/// 轮换签名密钥：先发布新 key（进入 JWKS），再切换 active；旧 key 标记
/// retiring 并保留至过期 + 安全余量。写密钥审计。
pub async fn rotate_signing_key(
    pool: &crate::db::DatabasePool,
    master_key: &[u8],
    actor: &str,
    reason: &str,
) -> Result<SigningKeyRow, OidcError> {
    if master_key.is_empty() {
        return Err(OidcError::ServerError(
            "OIDC key encryption key is not configured".to_string(),
        ));
    }
    let now = now_millis();
    let (priv_key, jwk) = generate_key_pair()?;
    let kid = uuid::Uuid::now_v7().to_string();
    let ciphertext = encrypt_private_key(master_key, &priv_key)?;
    let audit = serde_json::to_string(&[json!({
        "at": now,
        "actor": actor,
        "action": "rotate",
        "reason": reason,
    })])
    .unwrap_or_default();

    let result: Result<(), sqlx::Error> = match pool {
        Either::Left(p) => {
            let mut tx = p.begin().await?;
            // 发布新 key（进入 JWKS）后切换 active。
            sqlx::query(
                "UPDATE oauth_signing_keys SET status = 'retiring', retired_at = ? WHERE status = 'active'",
            )
            .bind(now)
            .execute(&mut *tx)
            .await?;
            insert_key_sqlite(&mut *tx, &kid, &ciphertext, &jwk.to_string(), &audit, now).await?;
            tx.commit().await?;
            Ok(())
        }
        Either::Right(p) => {
            let mut tx = p.begin().await?;
            sqlx::query(
                "UPDATE oauth_signing_keys SET status = 'retiring', retired_at = ? WHERE status = 'active'",
            )
            .bind(now)
            .execute(&mut *tx)
            .await?;
            insert_key_mysql(&mut *tx, &kid, &ciphertext, &jwk.to_string(), &audit, now).await?;
            tx.commit().await?;
            Ok(())
        }
    };
    result?;

    crate::audit::AuditEntry::user_action(actor, "oidc.signing_key.rotate")
        .with_target("signing_key", &kid)
        .with_reason(reason)
        .record(pool)
        .await
        .map_err(|e| OidcError::Db(e.to_string()))?;

    Ok(SigningKeyRow {
        kid,
        status: "active".to_string(),
        private_key_ciphertext: ciphertext,
        public_jwk_json: jwk.to_string(),
        created_at: now,
        retired_at: None,
        key_audit_json: Some(audit),
    })
}

/// 移除已超过"签发 Token 过期 + 安全余量"的 retiring 密钥。
/// 返回移除数量。
pub async fn purge_expired_keys(
    pool: &crate::db::DatabasePool,
    now: i64,
) -> Result<u64, OidcError> {
    let cutoff = now
        .saturating_sub(super::REFRESH_TOKEN_TTL_MS)
        .saturating_sub(super::KEY_RETIRE_MARGIN_MS);
    let affected = match pool {
        Either::Left(p) => {
            sqlx::query("DELETE FROM oauth_signing_keys WHERE status = 'retiring' AND retired_at IS NOT NULL AND retired_at < ?")
                .bind(cutoff)
                .execute(p)
                .await?
                .rows_affected()
        }
        Either::Right(p) => {
            sqlx::query("DELETE FROM oauth_signing_keys WHERE status = 'retiring' AND retired_at IS NOT NULL AND retired_at < ?")
                .bind(cutoff)
                .execute(p)
                .await?
                .rows_affected()
        }
    };
    Ok(affected)
}

/// JWKS 文档：active + retiring 密钥全部发布（轮换期间旧 key 可用）。
pub async fn jwks_document(pool: &crate::db::DatabasePool) -> Result<Value, OidcError> {
    let keys: Vec<Value> = match pool {
        Either::Left(p) => {
            let rows: Vec<(String, String)> = sqlx::query_as(
                "SELECT kid, public_jwk_json FROM oauth_signing_keys WHERE status IN ('active', 'retiring') ORDER BY created_at",
            )
            .fetch_all(p)
            .await?;
            rows.into_iter()
                .filter_map(|(kid, jwk)| {
                    let mut v: Value = serde_json::from_str(&jwk).ok()?;
                    v["kid"] = Value::String(kid);
                    Some(v)
                })
                .collect()
        }
        Either::Right(p) => {
            let rows: Vec<(String, String)> = sqlx::query_as(
                "SELECT kid, public_jwk_json FROM oauth_signing_keys WHERE status IN ('active', 'retiring') ORDER BY created_at",
            )
            .fetch_all(p)
            .await?;
            rows.into_iter()
                .filter_map(|(kid, jwk)| {
                    let mut v: Value = serde_json::from_str(&jwk).ok()?;
                    v["kid"] = Value::String(kid);
                    Some(v)
                })
                .collect()
        }
    };
    Ok(json!({ "keys": keys }))
}

/// 签发 RS256 ID Token（使用 active 密钥），返回完整 JWT。
pub async fn sign_id_token(
    pool: &crate::db::DatabasePool,
    master_key: &[u8],
    claims: &super::protocol::IdTokenClaims,
) -> Result<(String, String), OidcError> {
    let (row, priv_key) = active_signing_key(pool, master_key).await?;
    let header = json!({
        "alg": "RS256",
        "typ": "JWT",
        "kid": row.kid,
    });
    let payload = serde_json::to_value(claims)
        .map_err(|e| OidcError::ServerError(format!("cannot serialize ID token claims: {e}")))?;
    let signing_input = super::protocol::jwt_signing_input(&header, &payload)?;
    let signature = super::protocol::rsa256_sign(&signing_input, &priv_key)?;
    let token = format!(
        "{}.{}",
        signing_input,
        super::protocol::base64url_encode(&signature)
    );
    Ok((token, row.kid))
}

/// 验证 id_token_hint（logout）：校验签名、kid、exp，返回 payload JSON。
pub async fn verify_id_token_hint(
    pool: &crate::db::DatabasePool,
    master_key: &[u8],
    token: &str,
    now_secs: i64,
) -> Result<Value, OidcError> {
    let (_, payload_b64, sig_b64) = super::protocol::split_jwt(token)?;
    let header = super::protocol::decode_jwt_header(token)?;
    let kid = header
        .get("kid")
        .and_then(Value::as_str)
        .ok_or_else(|| OidcError::InvalidRequest("id token hint has no kid".into()))?;
    let payload = super::protocol::decode_jwt_payload(token)?;
    let signing_input = format!("{}.{}", payload_b64, sig_b64);
    let signature = super::protocol::base64url_decode(sig_b64).ok_or_else(|| {
        OidcError::InvalidRequest("id token hint signature is not base64url".into())
    })?;

    let Some((_, priv_key)) = signing_key_by_kid(pool, kid, master_key).await? else {
        return Err(OidcError::InvalidRequest(
            "id token hint references an unknown key".into(),
        ));
    };
    let pub_key = rsa::RsaPublicKey::from(&priv_key);
    super::protocol::rsa256_verify(&signing_input, &signature, &pub_key)?;

    let exp = payload
        .get("exp")
        .and_then(Value::as_i64)
        .ok_or_else(|| OidcError::InvalidRequest("id token hint has no exp".into()))?;
    if exp < now_secs {
        return Err(OidcError::InvalidRequest(
            "id token hint has expired".into(),
        ));
    }
    Ok(payload)
}
