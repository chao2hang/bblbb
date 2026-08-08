# BBLBB 预发布（staging）环境（M17-ENV-01）

> 目标：与生产同构的单机预发布。所有 M15 生产部署产物直接复用；
> 本目录说明 staging 编排与数据/恢复点复核方式。

## 布局

```
deploy/staging/
  README.md          ← 本文件
  Caddyfile          ← 复制 deploy/Caddyfile.template 并替换站点域名
  systemd/           ← 复制 deploy/systemd/*.service（backend/frontend/worker/backup-timer）
  .env.staging       ← BBLBB_* 环境变量（全部脱敏/合成数据）
  restore-verify.md  ← 恢复点复核清单（users/ledger/hash/grant/migration/JWKS/audit）
```

## 启动顺序（与生产一致）

1. `deploy/scripts/build-release.sh` 生成 release bundle。
2. 空库安装：`bblbb-migrate apply`（migrations bundle）。
3. 以 staging env 启动 backend（systemd）→ `/readyz` 全 ok → frontend（systemd）→ Caddy。
4. 注入合成 persona 数据（`frontend/tests/playwright/fixtures/seed-personas.mjs` 同源脚本）。
5. 恢复点复核：`ops/restore/verify.sh` 全项 PASS。

## 数据纪律

- 只使用脱敏/合成数据；canary 检查 `ops/scan-log-corpus.sh` CLEAN
  （公开内容与隐藏内容 canary 不混入日志或外部 Provider）。

## 演练记录（M17-ENV-02..07，真实执行）

- 空库安装 / 上一版本升级 / 重复迁移 / 错误迁移：M16 记录（apply_ms=125, lock_events=0）。
- SQLite/附件/OIDC key 备份恢复：`ops/backup/drill-2026-08-07.log`（RPO=0, RTO=0.18s，
  verify.sh ALL PASSED；恢复后 users/账本/迁移 checksum/grant/outbox/audit 全绿）。
- MySQL/MariaDB 恢复演练：**阻塞**（沙箱无真实 MySQL/MariaDB；脚本已就绪，见 M15-BACKUP-02）。
- 优雅停机/租约/回滚/重新切流：`ops/test-graceful-shutdown.sh` PASS（HTTP 0.30s / worker 0.04s）。
- RPO/RTO 与资源基线：M15-BACKUP-06 / M16-PERF 实测记录。
