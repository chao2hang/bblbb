# Runbook：S3 故障（403/404/429/5xx）、DNS/TLS、签名 TTL、孤儿对象

> 严重级别：P1（附件/下载受影响；核心论坛不受影响）→ P0（附件数据风险）
> 执行人：on-call 值班。

## 1. S3 403 / 404 / 429 / 5xx

**症状**：附件上传/下载失败；指标 `bblbb_storage_errors_total` 上升；
日志 `ProviderError::S3 { status }`。

**分类与处置**（分类逻辑见 `backend/src/jobs/classify.rs`）：

| 状态码 | 分类 | 处置 |
|---|---|---|
| 403 | 永久（dead-letter） | 检查 `BBLBB__S3_*` 凭据/Bucket 策略/KMS 权限；测试连接走管理后台「测试连接」脱敏接口 |
| 404 | 永久（dead-letter） | 对象缺失：查 manifest 与孤儿扫描（§4）；确认 bucket 名称/前缀 |
| 429 | 临时（退避重试） | 限流：检查并发签名/上传；提高 bucket 配额或降低 worker 并发 |
| 5xx | 临时（退避重试） | 服务端故障：观察 AWS 状态页；错误持续 >15 分钟切本地存储兜底 |

```sh
# 定位（错误码 + request_id，无 Secret）
journalctl -u bblbb-backend -n 200 --no-pager | grep -iE "s3|storage"
# 手动验证凭据（管理员受保护接口或 aws cli）
aws s3 ls s3://<bucket>/ --no-sign-request   # 仅用于公开桶；私有桶必须带签名
```

**兜底**：核心论坛不依赖 S3；附件读写失败时保持现状不删除对象，临时
`BBLBB__STORAGE_BACKEND=local` 只在维护窗口按 `docs/OPERATIONS.md` §16
迁移流程切换。

## 2. DNS / TLS

**症状**：`ProviderError::Connection` / TLS 握手失败；`s3.endpoint` 不可达。

```sh
nslookup <s3-endpoint>; dig +short <s3-endpoint>
curl -vI https://<s3-endpoint>/   # 检查证书链/SNI
# DNS/TLS 属基础设施：联系 DNS/网络负责人；TLS 1.2+ 与 CA 链由 backend 强制。
# 修复后确认 aws s3 ls 成功再重试任务（worker 会自动退避重试）。
```

## 3. 签名 TTL 异常

**症状**：预签名 URL 提前过期/签发失败率上升。

- TTL 由后端签发（下载/上传预签名）；URL 过期**不删除对象、不释放容量**。
- 检查服务器时间同步（`timedatectl`/NTP）：时钟漂移是签名验证失败最常见原因。
- 调整 TTL 只影响新签发链接；历史授权不变。

## 4. 孤儿对象扫描

**症状**：对象存在但无附件记录（孤儿），或附件记录无对象。

```sh
# 数据库侧对象 key 全集 vs 存储侧（manifest 对账）
ops/backup/manifest.sh --db <db> --s3-bucket <bucket> --out /tmp/manifest.json
# 本地存储对账：
comm -23 <(cd /var/lib/bblbb/uploads && find . -type f | sort) \
         <(sqlite3 <db> "SELECT storage_key FROM attachments WHERE status != 'deleted' ORDER BY 1;" | sort)
# 孤儿 mark-and-sweep 不误删在用文件（M06-MIGRATION）；手动清理需审批并保留审计。
```

## 5. 升级与确认

- 429/5xx 持续 >15 分钟 → P1；>1 小时 → P0（附件域）。
- 恢复确认：上传/下载冒烟、`bblbb_storage_errors_total` 回落、附件引用完整。
