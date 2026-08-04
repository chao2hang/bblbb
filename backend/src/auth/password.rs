use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Algorithm, Argon2, Params, Version,
};

/// 使用 Argon2id 哈希密码，返回 PHC 格式字符串
/// 使用默认参数 (m=19456, t=2, p=1)
pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::default());
    let hash = argon2.hash_password(password.as_bytes(), &salt)?;
    Ok(hash.to_string())
}

/// 验证结果
pub enum VerifyResult {
    /// 密码正确
    Ok,
    /// 密码错误
    Invalid,
    /// hash 格式损坏
    Error,
}

/// 验证密码，使用常量时间比较
pub fn verify_password(password: &str, hash: &str) -> VerifyResult {
    match PasswordHash::new(hash) {
        Ok(parsed) => {
            let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::default());
            match argon2.verify_password(password.as_bytes(), &parsed) {
                Ok(()) => VerifyResult::Ok,
                Err(_) => VerifyResult::Invalid,
            }
        }
        Err(_) => VerifyResult::Error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_and_verify() {
        let password = "test_password_123";
        let hash = hash_password(password).expect("hash should succeed");
        assert!(hash.starts_with("$argon2id$"));

        match verify_password(password, &hash) {
            VerifyResult::Ok => {}
            _ => panic!("verification should succeed"),
        }
    }

    #[test]
    fn test_wrong_password() {
        let hash = hash_password("correct_password").expect("hash should succeed");
        match verify_password("wrong_password", &hash) {
            VerifyResult::Invalid => {}
            _ => panic!("should be invalid"),
        }
    }

    #[test]
    fn test_corrupted_hash() {
        match verify_password("password", "not_a_valid_hash") {
            VerifyResult::Error => {}
            _ => panic!("should be error"),
        }
    }
}
