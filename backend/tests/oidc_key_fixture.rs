//! 一次性 fixture 生成器：产出真实 OIDC signing key 密文 + JWK，供
//! ops/restore/verify-oidc-keys.sh 演练使用（M15-BACKUP-09）。
//!
//! 运行：cargo test --test oidc_key_fixture -- --ignored --nocapture
//! 输出：<temp>/oidc-fixture-<ts>.json 与 oidc-fixture-master.key

use serde_json::json;

#[tokio::test]
#[ignore = "manual fixture generator"]
async fn generate_oidc_key_fixture() {
    use bblbb_backend::oidc::keys::{encrypt_private_key, generate_key_pair};

    let (private_key, jwk) = generate_key_pair().expect("key generation");
    let master_key = b"drill-master-key-material-0123456789abcdef";
    let ciphertext = encrypt_private_key(master_key, &private_key).expect("encrypt");
    let fixture = json!({
        "ciphertext": ciphertext,
        "jwk": jwk,
        "master_key": String::from_utf8_lossy(master_key),
    });
    let dir = std::env::temp_dir();
    let out_path = dir.join("oidc-fixture.json");
    std::fs::write(&out_path, serde_json::to_string_pretty(&fixture).unwrap()).unwrap();
    std::fs::write(dir.join("oidc-fixture-master.key"), master_key).unwrap();
    println!("FIXTURE_PATH={}", out_path.display());
}
