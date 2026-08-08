# Runbook：安全事故响应（Session 撤销、密钥轮换、Webhook secret、Provider 泄漏、审计保全）

> 严重级别：P0（凭证/数据泄漏）
> 执行人：安全值班；所有动作**全程写审计**（audit_logs 只增不改）。

## 1. Session 撤销（账号接管/设备丢失）

```sh
# 撤销单个用户全部 Session（后台管理操作，需 reason + recent-auth）
# audit: auth.session_revoke_all { actor, target, reason, request_id }
sqlite3 <db> "UPDATE user_sessions SET revoked_at=<now> WHERE user_id='<uid>' AND revoked_at IS NULL;"
# 撤销单个会话
sqlite3 <db> "UPDATE user_sessions SET revoked_at=<now> WHERE id='<sid>' AND revoked_at IS NULL;"
# 验证：旧 token 立刻失效（401），TOTP/CSRF 不受影响
```

## 2. 密钥轮换

| 密钥 | 位置 | 轮换 |
|---|---|---|
| OIDC signing key | DB `oauth_signing_keys`（密文） | 管理 API rotate：新 active，旧 retiring；新旧 kid 并存期旧 Token 可验签；purge 后不可逆 |
| MFA/TOTP 加密密钥 | `BBLBB__MFA_ENCRYPTION_KEY`（Secret） | 轮换需重加密全部 `totp_credentials.encrypted_secret`（批量任务）；未完成前保留旧密钥 |
| OIDC/Marketplace Webhook 加密主密钥 | `BBLBB__OIDC_KEY_ENCRYPTION_KEY` / `BBLBB__MARKETPLACE_WEBHOOK_ENCRYPTION_KEY` | 轮换需重加密密文列；分阶段执行，失败回滚 |
| SMTP / S3 凭据 | Secret store（systemd credentials） | SecretWriter 轮换（M01-CONFIG-04）：写新值、旧值不可读、审计版本 |

轮换后运行：`ops/restore/verify-oidc-keys.sh`（OIDC）、邮件/S3 冒烟。

## 3. Webhook secret 泄漏

- 立即：管理后台轮换 `webhook_secret_hash`（可恢复密文，HMAC 签名用）；
- 校验窗口：签名时间窗内旧签名短暂接受（防双跑），随后强制新签名；
- 审计：泄漏事件 + 轮换时间线。

## 4. Provider 泄漏（AI/Video/S3/SMTP 凭据或用户数据外泄）

```sh
# 1) 立即停用 Provider（feature-disable.md §1 kill switch 或单项停用）
# 2) 轮换全部相关凭据（§2）
# 3) 审计保全（§5）
# 4) 评估影响面：涉及用户数据（AI 正文外发/视频 URL）→ 通知受影响用户
#    （安全通知不可被用户偏好关闭）
# 5) 复盘：更新威胁模型（docs/SECURITY.md）+ 本文档
```

## 5. 审计保全（不可篡改）

```sh
# audit_logs 只增不改、无删除路径（M01-AUDIT）
sqlite3 <db> "SELECT COUNT(*) FROM audit_logs WHERE actor_id='<uid>';"
# 事故窗口审计导出（供调查）：
sqlite3 <db> ".mode json" "SELECT * FROM audit_logs WHERE created_at >= <窗口起点> ORDER BY created_at;" > /root/incident-<id>.json
# 导出文件 root:root 0600，按保留策略归档（法律保留见 privacy-lifecycle.md）
# 不可删除性由 DB 备份 + WORM/版本控制兜底（M15-BACKUP-05）。
```

## 6. 通知与复盘

- 受影响用户安全通知（kind=security_incident，不可关闭）；
- 事故复盘报告 + 根因 + 修复项 + 复查日期写入 `todo/issue.md` 与 CHANGELOG。
