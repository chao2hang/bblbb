use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Algorithm, Argon2, Params, Version,
};

/// 当前推荐 Argon2id 参数（M02-IDENTITY-04）。
///
/// 参数可升级：提高任一值后，旧参数 hash 在下一次成功登录时由
/// [`needs_rehash`] 标记为需重哈希（登录流程重新 `hash_password` 并写库）。
/// 验证总是使用 PHC 字符串内嵌的参数，因此旧 hash 仍可验证。
pub const M_COST: u32 = 19_456; // 内存 19 MiB
pub const T_COST: u32 = 2; // 迭代次数
pub const P_COST: u32 = 1; // 并行度

/// 使用 Argon2id 哈希密码，返回 PHC 格式字符串（m/t/p 内嵌）。
pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let params = Params::new(M_COST, T_COST, P_COST, None)?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let hash = argon2.hash_password(password.as_bytes(), &salt)?;
    Ok(hash.to_string())
}

/// 验证结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyResult {
    /// 密码正确。
    Ok,
    /// 密码错误（Argon2id 常量时间比较，argon2 crate 内部保证）。
    Invalid,
    /// hash 格式损坏（解析失败；不进行任何比较，也不泄漏内部状态）。
    Error,
}

/// 验证密码：正确 / 错误 / 损坏三条失败路径（M02-IDENTITY-04）。
///
/// - 验证参数取自 PHC 字符串内嵌值（参数可升级不破坏旧 hash 验证）；
/// - 错误密码路径由 argon2 crate 做常量时间比较；
/// - 损坏 hash 在解析阶段即返回 `Error`，不进入比较路径。
pub fn verify_password(password: &str, hash: &str) -> VerifyResult {
    match PasswordHash::new(hash) {
        Ok(parsed) => {
            // 参数无法解析（如 `m=invalid`）→ hash 损坏，返回 Error，不进入比较路径。
            let params = match Params::try_from(&parsed) {
                Ok(params) => params,
                Err(_) => return VerifyResult::Error,
            };
            // password-hash 的 PasswordVerifier 空白实现内部以
            // `Params::try_from(hash)` 重新解析 PHC 内嵌参数来计算验证，
            // 此处实例上的参数与内嵌值一致（参数可升级不破坏旧 hash 验证）。
            let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
            match argon2.verify_password(password.as_bytes(), &parsed) {
                Ok(()) => VerifyResult::Ok,
                Err(_) => VerifyResult::Invalid,
            }
        }
        Err(_) => VerifyResult::Error,
    }
}

/// 判断 hash 是否需按当前推荐参数重哈希（参数可升级，M02-IDENTITY-04）。
///
/// 返回 `Ok(true)`：旧参数（m/t/p 任一低于当前推荐值）→ 下次登录重哈希。
/// 返回 `Err`：hash 损坏。
pub fn needs_rehash(hash: &str) -> Result<bool, argon2::password_hash::Error> {
    let parsed = PasswordHash::new(hash)?;
    // 从 PHC 字符串解析内嵌参数，与当前推荐参数比较（m/t/p 任一较低 → 重哈希）
    let params = Params::try_from(&parsed)?;
    Ok(params.m_cost() != M_COST || params.t_cost() != T_COST || params.p_cost() != P_COST)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_and_verify() {
        let password = "test_password_123";
        let hash = hash_password(password).expect("hash should succeed");
        assert!(hash.starts_with("$argon2id$"));
        assert_eq!(verify_password(password, &hash), VerifyResult::Ok);
    }

    #[test]
    fn test_wrong_password() {
        let hash = hash_password("correct_password").expect("hash should succeed");
        assert_eq!(
            verify_password("wrong_password", &hash),
            VerifyResult::Invalid
        );
    }

    #[test]
    fn test_corrupted_hash() {
        // 损坏 hash：解析失败 → Error，不进入比较路径
        for corrupt in ["not_a_valid_hash", "", "$argon2id$v=19$m=invalid"] {
            assert_eq!(verify_password("password", corrupt), VerifyResult::Error);
        }
    }

    #[test]
    fn verify_uses_params_embedded_in_hash() {
        // 用旧参数（m=8KiB, t=1）生成 hash → 验证仍成功（参数内嵌可升级）
        let old_params = Params::new(8_192, 1, 1, None).unwrap();
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, old_params);
        let salt = SaltString::generate(&mut OsRng);
        let hash = argon2
            .hash_password(b"password123", &salt)
            .unwrap()
            .to_string();

        assert!(hash.contains("m=8192"), "旧参数应内嵌在 PHC 中: {hash}");
        assert_eq!(verify_password("password123", &hash), VerifyResult::Ok);
        assert_eq!(verify_password("wrong", &hash), VerifyResult::Invalid);
    }

    #[test]
    fn needs_rehash_detects_param_upgrade() {
        // 当前参数 → 不需要重哈希
        let current = hash_password("password123").unwrap();
        assert!(!needs_rehash(&current).unwrap());

        // 旧参数 → 需要重哈希（参数可升级）
        let old_params = Params::new(8_192, 1, 1, None).unwrap();
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, old_params);
        let salt = SaltString::generate(&mut OsRng);
        let old_hash = argon2
            .hash_password(b"password123", &salt)
            .unwrap()
            .to_string();
        assert!(needs_rehash(&old_hash).unwrap(), "旧参数必须标记需重哈希");

        // 损坏 hash → Err
        assert!(needs_rehash("broken").is_err());
    }

    #[test]
    fn failure_paths_are_stable_and_deterministic() {
        // 正确/错误/损坏三条路径返回稳定结果（不 panic、不随机）
        let hash = hash_password("correct").unwrap();
        for _ in 0..20 {
            assert_eq!(verify_password("correct", &hash), VerifyResult::Ok);
            assert_eq!(verify_password("wrong", &hash), VerifyResult::Invalid);
            assert_eq!(verify_password("correct", "corrupt"), VerifyResult::Error);
        }
    }
}
