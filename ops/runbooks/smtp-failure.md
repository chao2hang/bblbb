# Runbook：SMTP 失败、验证邮件堆积、token 日志检查、dead-letter

> 严重级别：P1（邮件域；注册/验证/通知延迟）→ P0（邮件完全不可用）
> 执行人：on-call 值班。
> 真实 SMTP 故障演练依赖外部 SMTP 基础设施（M15-RUNBOOK-03 [!] 阻塞项）；
> 本 Runbook 命令级就绪，沙箱以脚本+文档验证。

## 1. SMTP 失败

**症状**：`email.deliver` 任务重试/死信；指标 `bblbb_jobs_dead` 上升；
日志 `smtp rejected (code xxx)`。

```sh
# 1) 队列快照（metrics /jobs 状态）
curl -fsS http://127.0.0.1:8080/metrics | grep -E "bblbb_jobs_(queued|dead)"
# 2) 死信任务（last_error 已脱敏，无 token/邮箱/正文）
sqlite3 <db> "SELECT id, kind, attempts, last_error FROM jobs WHERE status='dead' ORDER BY updated_at DESC LIMIT 20;"
# 3) 分类处置
#    4xx（421/450/451/452）：临时，自动退避重试 → 观察；
#    5xx（550/551/552/553/554）：永久（地址不存在/被拒）→ 死信，人工核对模板/收件人；
#    凭据错误（535/454）：轮换 SMTP Secret（Secret 轮换流程，走审计）。
# 4) 重放死信（管理员审计操作）
#    sqlite3 <db> "UPDATE jobs SET status='queued', attempts=0, last_error=NULL, available_at=<now> WHERE id='<id>';"
```

## 2. 验证邮件堆积

**症状**：注册后收不到验证邮件；`bblbb_jobs_queued`（mail 队列）持续增长。

```sh
# 1) 确认不是全局死信：mail 队列 pending 数
curl -fsS http://127.0.0.1:8080/metrics | grep bblbb_jobs_queued
# 2) 确认 worker 在跑
systemctl status bblbb-worker
# 3) SMTP Provider 抖动导致退避积压：等待自动恢复；积压>1h 联系 SMTP 供应商
# 4) 兜底：验证邮件 token 可由用户主动重新请求（/api/v1/auth/resend-verification，
#    受限流保护），不手工放行。
```

## 3. Token 日志检查

验证/重置邮件中的一次性 token 绝不允许进入日志（M05-NOTIFY-08）。

```sh
# 例行检查（可 cron）：扫描 journald 与日志文件中的 token 形态
journalctl --since "24 hours ago" | grep -iE "reset_token=|verify_token=[a-f0-9]{16,}|token=[A-Za-z0-9]{32,}" || echo "no token leak"
grep -rniE "reset_token=|verify_token=[a-f0-9]{16,}" /var/log/bblbb/ 2>/dev/null || true
# 发现 token 出现在日志：P0 事故 → 立即轮换涉及 token、通知受影响用户，
# 走 ops/runbooks/security-incidents.md 审计保全流程。
```

## 4. Dead-letter（队列级）

**症状**：`bblbb_jobs_dead` 持续上升；`/jobs` 管理视图 dead 增加。

```sh
# 1) 按 kind 分类死信
sqlite3 <db> "SELECT kind, COUNT(*), MAX(last_error) FROM jobs WHERE status='dead' GROUP BY kind;"
# 2) 永久错误（凭据/输入/Provider 5xx）→ 修复根因后人工重放（审计）；
# 3) 重放边（dead→queued）仅管理员审计操作；
# 4) 安全队列最老任务 >5 分钟 / Outbox 堆积 → 告警（alerts.md）。
```

## 升级与确认

- SMTP 不可用 >30 分钟 → P1；>2 小时 → P0（注册/密码恢复受影响）。
- 恢复确认：`email.deliver` 成功率回升、队列回落到基线、抽样收信成功。
