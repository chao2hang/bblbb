# BBLBB — 文档变更记录

## v0.6 — 2026-08-08（M15 生产运维交付）

### 生产部署

- Release bundle 布局与最小权限（`deploy/RELEASE-BUNDLE.md`）、构建脚本
  `deploy/scripts/build-release-bundle.sh`（backend release 二进制 + frontend
  build + 三方言迁移 + 依赖锁 + METADATA/SBOM/SHA256SUMS）。
- Caddy 模板（`deploy/Caddyfile.template`）：TLS、HTTP→HTTPS、CSP、安全头、
  压缩、body limit（10MB 双层）、`/readyz` 与 `/metrics` 不公开代理。
- systemd units（`deploy/systemd/`）：backend/frontend/worker + 每日备份
  timer；服务用户 `bblbb` 对 release 目录只读（不可写）。
- 启动检查 `deploy/scripts/startup-checks.sh`（origin/Cookie/DB/目录/迁移/
  OIDC key/外部配置）；发布/回滚编排 `deploy/scripts/release.sh`。

### 观测

- `BBLBB__LOG_FORMAT` 配置（text/json）：JSON 日志字段
  timestamp/service/level/request_id/route/method；敏感字段名与值级脱敏
  （Cookie/Authorization/OAuth token/密码/完整邮箱/隐藏正文/Prompt/签名 URL）；
  `ops/scan-log-corpus.sh` 日志语料扫描自检与实测 CLEAN。
- `/metrics` Prometheus 端点（loopback-only）：HTTP p50/p95/p99、错误、
  429、DB pool、SQLite busy、连接失败 + Session/CSRF/TOTP/OAuth/上传/存储/
  账务/任务/Outbox 领域指标（`deploy/monitoring/metrics.md`）。
- 告警定义（`deploy/monitoring/alerts.md`）+ 表推演练
  （`deploy/monitoring/alerts-drill.sh`，PASS=71）。

### 备份与恢复

- `ops/backup/`：sqlite.sh（WAL checkpoint + 安全复制）、manifest.sh、
  daily.sh、mysql.sh/mariadb.sh（真实演练为外部阻塞 [!]）。
- `ops/restore/`：sqlite.sh、verify.sh（用户/账本恒等式/迁移 checksum/grant/
  outbox/audit）、verify-attachments.sh、verify-oidc-keys.sh。
- OIDC 密钥分离存储设计（`ops/backup/oidc-keys.md`）。
- 真实演练 `ops/backup/drill-sqlite.sh`：RPO=0（WAL checkpoint 一致快照）、
  RTO=0.18s（擦除→完整恢复+内容校验，实测）。

### 升级与 Runbook

- `--worker` 模式（`bblbb-backend --worker`）与任务分发
  `backend/src/jobs/dispatch.rs`；SIGTERM 优雅停机实测
  `ops/test-graceful-shutdown.sh`（HTTP 0.30s、worker 0.04s 干净退出）。
- 迁移升级演练 `deploy/scripts/drill-migration-upgrade.sh`
  （apply_ms=68、lock_events=0、幂等二次 apply 0）。
- 发布后冒烟 `ops/smoke/smoke.sh`（db/登录/发帖/回复/附件/账本/管理 API，
  PASS=14）。
- 命令级 Runbook 全套（`ops/runbooks/`）+ 值班矩阵 + 非作者执行记录。

## v0.5 — 2026-08-04

### 需求问答确认

- 产品定位确认为公开兴趣社区，并冻结匿名访问、邮箱验证、冷静期、强制 2FA、无私信和审核公开策略。
- 冻结内容可见性、B币非现金属性、内部商城、共享附件容量、AI 逐次同意、第三方应用额度市场和数据保留规则。
- 已确认扩展能力全部纳入 v1.0 目标，以内部里程碑、Feature Flag 和专项发布门槛控制启用，不再标记为 v1.1/v1.2。
- 新增 `PRODUCT-DECISIONS.md` 和 `CRAWLER-POLICY.md`，明确产品所有者决策以及搜索索引、AI 爬虫和批量访问策略。
- 产品所有者于 2026-08-04 确认冻结 v0.5，作为生产工程开发依据；生产工程实现尚未开始。

## v0.4 — 2026-08-04

### 新增能力

- Marketplace Offer、Checkout Intent、托管确认、双边站内账本、商户待结算余额、退款和 Webhook。
- 下载抵扣的策略优先级、免费授权、原子扣费和 S3 URL 重签语义。
- AI Gateway、Provider、独立同意、Task、Suggestion、预算和故障降级。
- 核心 Video Service 与 Direct/HLS/Xigua Provider Adapter、手动 URL、CSP 和 SSRF/HLS 防护。
- 内部积分商城、装扮槽位、签到任务、社区 Reaction、库存和活跃反刷规则。

### 基线补档

- 新增文档状态与功能发布矩阵。
- 新增统一术语、状态机、错误码和 Endpoint 权限矩阵。
- 新增资源 DTO 契约、Marketplace 账务决策。
- 新增配置、领域事件、数据保留和隐私矩阵。
- 统一基础文档版本为 v0.4。
- 统一 Post 的 `closed_at` 语义、Sanction 枚举、Video 管理 API 和 AI/Video 异步 Task 语义。

### 兼容说明

v0.4 仍处于首次正式开发前，没有已发布的稳定 v1 API 客户端，因此上述统一不构成生产破坏性变更。从 v1.0 发布后，删除字段、改变枚举语义、改变账务或权限规则必须按 `API.md` 的废弃周期处理。
