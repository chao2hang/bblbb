# Runbook：隐私生命周期（数据导出、注销匿名化、30 天删除、法律保留、恢复误删）

> 执行人：值班 + 法务/隐私审批人（`oncall.md`）。
> 行为实现见 `docs/RETENTION-PRIVACY.md`；本文件只给命令级流程。

## 1. 数据导出（用户请求 GDPR 式导出）

```sh
# 受保护管理/用户接口导出该用户全部数据（posts/comments/uploads/ledger/...），
# 生成 zip + 清单（禁含他人数据与 Secret）。
# 审计：privacy.data_export { actor, target, reason, request_id }
# 交付：受控下载链接（短时效，不落日志）。
```

## 2. 注销匿名化（soft-delete + 匿名化）

- 注销后用户进入 `pending_delete` → 匿名化（display_name/bio/email 脱敏为
  占位符）→ 内容保留为「已注销用户」署名；
- 执行路径：`users::deletion`（`account_deletion` job 队列）——worker 驱动，
  可重试、可观测（`bblbb_jobs_queued`）；
- 匿名化后不可逆；再次登录入口关闭。

```sh
# 手动触发（管理员，reason 必填）：
# 通过后台接口发起注销；确认 pending_delete 任务入队
sqlite3 <db> "SELECT id, status, delete_requested_at FROM users WHERE status='pending_delete';"
```

## 3. 30 天删除（hard delete 窗口）

- `pending_delete` 满 30 天 → 物理删除（posts/comments/uploads/账户行 +
  附件对象）；
- 每日清理任务（worker）执行；删除前做最终备份留档（法律保留除外）。

## 4. 法律保留（legal hold）

```sh
# 设置 legal hold：冻结删除生命周期
sqlite3 <db> "UPDATE users SET legal_hold_at=<now> WHERE id='<uid>';"
# 效果：30 天 hard delete 跳过该用户；其内容与附件保留（含备份恢复不覆盖）。
# 解除：法务书面确认 + 审计（privacy.legal_hold_released）。
```

## 5. 恢复误删

- 软删阶段（`deleted`/`pending_delete` 未到 30 天）：置回 active 即可恢复；
- 已物理删除：从备份恢复（`ops/restore/sqlite.sh` + 附件 tar 回填），
  注意恢复后数据窗口损失与并发写入冲突（恢复目标为隔离库再人工合并，
  禁止直接覆盖生产库）；恢复后运行 `verify.sh` 与 `verify-attachments.sh`；
- 恢复动作全程审计（privacy.restore）。

## 6. 确认与升级

- 隐私请求 SLA：导出 ≤72h；注销 ≤24h 生效；法律保留立即生效；
- 超 SLA → P1 通知法务；涉及未成年人/跨境 → 升级法务负责人。
