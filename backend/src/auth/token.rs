use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};

/// 生成 256-bit 熵的安全随机令牌（URL-safe base64 编码）
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    base64_url_no_pad(&bytes)
}

/// 对令牌进行 SHA-256 哈希，返回十六进制字符串
/// 数据库只存储 hash，不存储原始令牌
pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// 验证令牌是否匹配 hash（常量时间）
pub fn verify_token(token: &str, hash: &str) -> bool {
    let computed = hash_token(token);
    // 简单的常量时间比较
    if computed.len() != hash.len() {
        return false;
    }
    let mut result = 0u8;
    for (a, b) in computed.bytes().zip(hash.bytes()) {
        result |= a ^ b;
    }
    result == 0
}

/// 令牌明文的可还原密文（GA P0-2 收尾：邮件投递需要明文渲染一次性链接）。
///
/// 设计：DB 不存明文、jobs payload 不带明文（M01-JOBS-12 / M05-NOTIFY 不变）；
/// 存 `enc1:` 密文（BBLBB__SETTINGS_ENCRYPTION_KEY 加密），邮件 worker 投递时
/// 解密。key 为空时（仅 dev）`encrypt_setting` 明文回落。
pub fn seal_token(settings_key: &str, token: &str) -> String {
    crate::config::secret_crypto::encrypt_setting(settings_key, token)
}

/// 解开 [`seal_token`] 的密文；空/无法解密返回 None（调用方按不可投递处理）。
pub fn open_token(settings_key: &str, sealed: &str) -> Option<String> {
    if sealed.is_empty() {
        return None;
    }
    let plain = crate::config::secret_crypto::decrypt_setting(settings_key, sealed);
    if plain.is_empty() {
        None
    } else {
        Some(plain)
    }
}

/// URL-safe base64 编码（无填充）
fn base64_url_no_pad(bytes: &[u8]) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_generation() {
        let token = generate_token();
        assert!(!token.is_empty());
        assert!(token.len() >= 32);
    }

    #[test]
    fn test_token_uniqueness() {
        let t1 = generate_token();
        let t2 = generate_token();
        assert_ne!(t1, t2);
    }

    #[test]
    fn test_hash_and_verify() {
        let token = generate_token();
        let hash = hash_token(&token);
        assert_ne!(token, hash);
        assert!(verify_token(&token, &hash));
        assert!(!verify_token("wrong_token", &hash));
    }
}
