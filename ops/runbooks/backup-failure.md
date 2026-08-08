# Runbook：备份失败 / 磁盘空间不足 / 解密失败 / 部分恢复 / 恢复后切流（M15-BACKUP-10）

> 执行人：on-call（见 `oncall.md` 值班矩阵）。所有命令先在 `--dry-run` 或隔离
> 环境验证；涉及生产写操作需审批（维护窗口、四眼）。

## 1. 备份失败

**症状**：`systemctl status bblbb-backup` failed；`bblbb_backup_failed_total` 告警；
`/var/lib/bblbb/backups` 最新产物超过 26 小时。

**处置**：

```sh
# 1) 定位失败阶段（journald 有完整输出）
journalctl -u bblbb-backup -n 100 --no-pager

# 2) 手跑 sqlite 备份，观察 checkpoint 输出
sudo -u bblbb ops/backup/sqlite.sh /var/lib/bblbb/database/bblbb.db /tmp/bblbb-backup-test

# 3) 常见原因与修复
#    - WAL 无法 checkpoint（busy）：查慢查询/长事务，等待后重试；SQLite busy
#      持续则按 runbooks/sqlite-busy.md 处置；
#    - 磁盘满：见下文 §2；
#    - 权限：备份目录属主/ACL 异常，恢复 root:bblbb 0640；
#    - 备份文件损坏：备份脚本在复制前做 integrity_check，损坏即中止（不会产生坏备份）。

# 4) 恢复自动调度
sudo systemctl start bblbb-backup
```

**升级**：连续 2 天失败 → P1；连续 3 天 → P0（RPO 违约），按 §4 立即执行
部分恢复演练验证。

## 2. 磁盘空间不足（disk full）

**症状**：`df -h` 使用率 >90% 告警；备份/上传/数据库写入报 `No space left`。

**处置**：

```sh
# 1) 定位占用
df -h /var/lib/bblbb /opt/bblbb /var
du -sh /var/lib/bblbb/backups /var/lib/bblbb/uploads /var/lib/bblbb/database 2>/dev/null

# 2) 立即止损：按保留策略清理旧备份（root 执行；绝不删应用数据）
find /var/lib/bblbb/backups -maxdepth 3 -type f -mtime +14 -delete

# 3) WAL 截断回收（大量写入时 WAL 可能数 GB）
sqlite3 /var/lib/bblbb/database/bblbb.db "PRAGMA wal_checkpoint(TRUNCATE);"

# 4) 扩大容量或迁移存储（storage migration 流程见 docs/OPERATIONS.md §16）；
#    磁盘 <15% 时暂停每日备份并告警，不得静默丢弃备份。

# 5) 磁盘恢复后：
sudo systemctl start bblbb-backup
```

## 3. 解密失败（restore 时）

**症状**：`ops/restore/verify-oidc-keys.sh` 或解密步骤报 `decrypt failed`；
主密钥文件缺失/轮换不匹配。

**处置**：

```sh
# 1) 确认用错了密钥：比对密钥文件 mtime 与备份时间；从 secrets-recovery 取副本
install -m 0600 /opt/bblbb/secrets-recovery/oidc-key-encryption /etc/bblbb/secrets/oidc-key-encryption

# 2) 若备份本身损坏（sha256 不匹配）：
#    - 改用更早备份（保留策略 ≥14 天）；
#    - 若全部备份都无法解密（主密钥丢失且无恢复副本）：按事故升级，
#      OIDC 保持关闭，重新生成密钥（所有外部 RP 需重新授权，走
#      docs/AUTH-OIDC.md 的 key rotation 流程）。

# 3) 恢复后必须验证：
ops/restore/verify-oidc-keys.sh --db <restored.db> --key-file /etc/bblbb/secrets/oidc-key-encryption
```

## 4. 部分恢复（partial restore）

场景：数据库恢复成功，但附件/S3 对象部分缺失或主题/配置未恢复。

```sh
# 1) 用 manifest 对账缺失对象
ops/restore/verify-attachments.sh --db <restored.db> --storage /var/lib/bblbb/uploads

# 2) 按 manifest 逐项回填本地对象（对象在 tar 备份内）
tar -xzf /var/lib/bblbb/backups/attachments-<label>.tar.gz -C /var/lib/bblbb/uploads

# 3) S3 缺失：按 list-object-versions 找回历史版本（S3 versioning 演练 [!]）

# 4) 缺失 grant/outbox/audit：任何表级缺失意味着该备份不完整，禁止切流；
#    改用完整备份重来，或接受数据窗口损失并书面确认（不可逆迁移同理）。
```

## 5. 恢复后切流（switchover）

```sh
# 1) 恢复验证全绿
ops/restore/verify.sh --db <restored.db>
ops/restore/verify-attachments.sh --db <restored.db> --storage /var/lib/bblbb/uploads

# 2) 维护窗口内切流：停 backend/worker → 指向恢复库 → 启动 → ready 检查
sudo systemctl stop bblbb-backend bblbb-worker
# （配置中 DATABASE_URL 指向恢复库）
sudo systemctl start bblbb-backend bblbb-worker
curl -fsS http://127.0.0.1:8080/readyz | grep '"status":"ok"'

# 3) 冒烟（ops/smoke/smoke.sh）+ 账本抽查（Σ(delta)=balance）
# 4) 保留旧库与备份至少一个回滚窗口（≥72h）；确认无误后按保留策略归档。
# 5) 写恢复记录到 docs/CHANGELOG.md 与事故记录。
```
