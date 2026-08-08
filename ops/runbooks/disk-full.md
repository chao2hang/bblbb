# Runbook：磁盘空间不足（disk full）与 WAL 过大（wal-too-large）

> 严重级别：P0（数据写入风险）
> 执行人：on-call 值班；磁盘 <15% 视为紧急。

## 症状

- `df` 使用率 >90% 告警（alerts.md `bblbb_disk_free_ratio`）；
- 日志 `No space left on device`；备份/上传/写入失败。

## 处置（Disk Full）

```sh
# 1) 定位占用
df -h / /var/lib/bblbb /opt/bblbb
du -sh /var/lib/bblbb/* /opt/bblbb/* /var/log 2>/dev/null | sort -h | tail -10

# 2) 立即止损（按保留策略，root 执行）
find /var/lib/bblbb/backups -maxdepth 3 -type f -mtime +14 -delete
find /var/lib/bblbb/uploads -name "*.tmp" -o -name "*.part" 2>/dev/null | head
# 清空被删文件的 open 句柄（服务占用大文件时）
lsof +L1 2>/dev/null | awk '{print $1, $2, $7}' | sort -k3 -n | tail -5
# 对确认可释放的已删除但仍占用文件：重启对应服务即可释放

# 3) 数据库所在分区不足：
#    迁移数据目录（软链接）或扩容；不得删除 .db 文件。

# 4) 恢复后验证备份可写：
systemctl start bblbb-backup && journalctl -u bblbb-backup -n 20
```

## 处置（WAL 过大）

```sh
# 诊断
ls -lh /var/lib/bblbb/database/bblbb.db-wal
sqlite3 /var/lib/bblbb/database/bblbb.db "PRAGMA wal_checkpoint(PASSIVE);"

# 截断回收
sqlite3 /var/lib/bblbb/database/bblbb.db "PRAGMA wal_checkpoint(TRUNCATE);"

# 若 WAL 反复膨胀：检查是否有持续长事务/批量写（搜索重建/重渲染/删除任务），
# 错峰调度；生产建议每日备份时 checkpoint 截断（bblbb-backup.timer）。
```

## 恢复确认

- 磁盘余量回到安全水位；备份/上传成功；`/readyz` 正常。
