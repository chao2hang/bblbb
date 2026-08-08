# OIDC 私钥分离存储与恢复设计（M15-BACKUP-04）

> 状态：设计定稿 + 恢复脚本就绪；DB 密文路径已实现（`oauth_signing_keys.private_key_ciphertext`，AES-256-GCM，主密钥 `BBLBB__OIDC_KEY_ENCRYPTION_KEY`）。

## 1. 威胁模型

OIDC signing key 一旦泄露，攻击者可伪造 ID Token 冒充任意用户。因此：

1. **单一位置的完整密钥对不存在**：明文私钥永远不出现在磁盘、备份、CI 或日志。
2. **密文与解密主密钥必须分离存储**：任何一份备份被拖走，都无法单独还原私钥。
3. 应用账号（`bblbb`）只能读密文（数据库内），不能读主密钥文件。

## 2. 存储布局（分离）

```text
DB（/var/lib/bblbb/database/bblbb.db）   ← oauth_signing_keys.private_key_ciphertext（密文）
/etc/bblbb/secrets/oidc-key-encryption   ← 解密主密钥（root:bblbb 0640，随备份走）
                                          ← 但主密钥的灾难恢复副本在独立位置：
/opt/bblbb/secrets-recovery/             ← 第二份主密钥副本（root:root 0700，不随常规备份）
异地离线介质（每季度）                    ← 主密钥副本（加密信封，口令单独保存）
```

约束：
- **主密钥不写入**数据库备份、附件备份或任何自动上传到对象存储的 bundle。
- 数据库备份 bundle 内只含密文（`oauth_signing_keys` 行）。
- 主密钥灾难恢复副本**人工/离线**制作，禁止进入自动备份管道。

## 3. 备份时

```sh
# 数据库备份自动包含密文（无需额外动作）。
ops/backup/sqlite.sh /var/lib/bblbb/database/bblbb.db /var/lib/bblbb/backups
ops/backup/manifest.sh --db /var/lib/bblbb/database/bblbb.db --storage /var/lib/bblbb/uploads \
  --out /var/lib/bblbb/backups/manifest-$(date +%F).json

# 主密钥灾难恢复副本（人工，季度/轮换后）：
install -m 0600 /etc/bblbb/secrets/oidc-key-encryption /opt/bblbb/secrets-recovery/oidc-key-encryption
# 并复制到异地离线介质（口令信封分离）。
```

## 4. 恢复时

```sh
# 1) 恢复数据库（含密文）
ops/restore/sqlite.sh <backup>/bblbb.sqlite /var/lib/bblbb/database/bblbb.db --verify

# 2) 恢复主密钥（若丢失：从 secrets-recovery 或离线介质取回）
install -m 0600 /opt/bblbb/secrets-recovery/oidc-key-encryption /etc/bblbb/secrets/oidc-key-encryption

# 3) 验证：旧 ID Token / JWKS / Refresh family / key rotation
ops/restore/verify-oidc-keys.sh --db /var/lib/bblbb/database/bblbb.db \
  --jwk-url http://127.0.0.1:8080/.well-known/jwks.json
```

## 5. 恢复验证（M15-BACKUP-09）

`verify-oidc-keys.sh` 校验：

1. `oauth_signing_keys` 有 active key，`private_key_ciphertext` 非空、`public_jwk_json` 是合法 JWK；
2. JWKS 端点返回的 kid 与 DB 一致（需要后端在 OIDC Flag 开启时运行）；
3. 主密钥可解密密文：用相同主密钥解密出 RSA 私钥并与 JWK 公钥匹配
   （脚本调用后端 key 校验逻辑或 openssl 验证 PEM↔JWK 指纹）；
4. key rotation：`/oauth/keys/rotate`（管理员 API）生成新 active key，旧 key 进入
   `retiring`，新旧 kid 并存期内旧 ID Token 仍可验签；恢复后轮换不得破坏
   `oauth_interactions`/`oauth_authorizations` 引用；
5. Refresh family：恢复后未发生 `refresh_token` 重用（`refresh_reuse_detected` 计数不增）。

> 真实恢复演练（旧 ID Token 验签 + JWKS + rotation + refresh reuse）依赖 OIDC
> 专项启用环境，与 M15-BACKUP-03 的 S3 演练一同由外部基础设施阻塞项跟踪；
> 沙箱内以 DB 密文完整性 + JWK 结构校验为最小验证集。

## 6. 不变量（测试强制）

- `oauth_signing_keys.private_key_ciphertext` 是 AES-256-GCM 密文（base64），
  不是 PEM 明文（`backend/src/oidc/keys.rs` 测试断言）。
- 主密钥缺失时 OIDC key 生成/轮换直接失败（fail closed），不临时生成新 key。
- 备份 manifest 不包含 Secret 值，只登记 Secret 名称。
