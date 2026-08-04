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
