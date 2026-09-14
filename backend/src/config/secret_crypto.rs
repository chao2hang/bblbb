//! 站点设置 Secret 静态加密（P0 整改）。
//!
//! `site_settings` 中的 SMTP 密码 / OAuth Client Secret / S3 Secret Key
//! 属于敏感凭据，docs/SCHEMA.md §site_settings 要求"数据库中的秘密必须
//! 加密，不得只靠 is_secret 标识"。本模块用 `BBLBB__SETTINGS_ENCRYPTION_KEY`
//! 作为主密钥做 AES-256-GCM 静态加密（复用 [`crate::auth::mfa`] 的 AEAD
//! 实现，SHA-256 派生 32 字节密钥）。
//!
//! 兼容策略：
//! - 密文带 `enc1:` 前缀（nonce||ciphertext 的 hex）；
//! - 无前缀的历史明文在读取时原样透传（legacy 兼容），下一次保存时加密；
//! - 主密钥未配置（空）= 明文兼容模式：加密退化为原样存储、解密对密文
//!   返回空并记 error（fail closed，不输出明文）。生产模式由
//!   `validate_production` 强制要求配置。

/// 密文前缀（版本化；算法轮换时递增版本）。
const ENC_PREFIX: &str = "enc1:";

/// 判断存储值是否已是本模块密文。
pub fn is_encrypted(stored: &str) -> bool {
    stored.starts_with(ENC_PREFIX)
}

/// 加密站点设置 Secret；空值原样返回（表示"未配置"）。
///
/// 主密钥未配置时返回明文（明文兼容模式，仅限开发环境）。
pub fn encrypt_setting(key: &str, plain: &str) -> String {
    if plain.is_empty() {
        return String::new();
    }
    if key.is_empty() {
        tracing::warn!(
            "BBLBB__SETTINGS_ENCRYPTION_KEY is empty; storing settings secret in PLAINTEXT (dev only)"
        );
        return plain.to_owned();
    }
    format!(
        "{ENC_PREFIX}{}",
        crate::auth::mfa::encrypt_secret(key.as_bytes(), plain.as_bytes())
    )
}

/// 解密站点设置 Secret；空值/历史明文原样返回。
///
/// 密文解密失败（密钥错误/数据损坏）→ 返回空串并记 error（fail closed：
/// 不把明文泄漏给调用方，调用方按"未配置"处理）。
pub fn decrypt_setting(key: &str, stored: &str) -> String {
    if stored.is_empty() {
        return String::new();
    }
    let Some(ciphertext_hex) = stored.strip_prefix(ENC_PREFIX) else {
        // 历史明文（迁移前写入）：原样透传，等下次保存加密。
        return stored.to_owned();
    };
    if key.is_empty() {
        tracing::error!(
            "settings secret is encrypted but BBLBB__SETTINGS_ENCRYPTION_KEY is empty; treating as unconfigured"
        );
        return String::new();
    }
    match crate::auth::mfa::decrypt_secret(key.as_bytes(), ciphertext_hex)
        .and_then(|bytes| String::from_utf8(bytes).ok())
    {
        Some(plain) => plain,
        None => {
            tracing::error!("settings secret decryption failed (wrong key or corrupted data)");
            String::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "settings-key-material";

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let cipher = encrypt_setting(KEY, "s3cr3t-pass");
        assert!(is_encrypted(&cipher));
        assert!(!cipher.contains("s3cr3t-pass"));
        assert_eq!(decrypt_setting(KEY, &cipher), "s3cr3t-pass");
    }

    #[test]
    fn empty_stays_empty() {
        assert_eq!(encrypt_setting(KEY, ""), "");
        assert_eq!(decrypt_setting(KEY, ""), "");
    }

    #[test]
    fn legacy_plaintext_passthrough() {
        assert_eq!(decrypt_setting(KEY, "old-plain"), "old-plain");
    }

    #[test]
    fn empty_key_fails_closed_on_ciphertext() {
        let cipher = encrypt_setting(KEY, "secret");
        assert_eq!(decrypt_setting("", &cipher), "");
    }

    #[test]
    fn wrong_key_fails_closed() {
        let cipher = encrypt_setting(KEY, "secret");
        assert_eq!(decrypt_setting("other-key", &cipher), "");
    }

    #[test]
    fn no_key_plaintext_mode() {
        assert_eq!(encrypt_setting("", "dev-secret"), "dev-secret");
    }
}
