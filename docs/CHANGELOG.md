## v1.0.0-rc.2 — 2026-08-08（M17 RC 冻结 / 预发布 / 冒烟 / Flag 记录）

### RC 冻结（M17-FREEZE）

- RC 变更清单 `reports/rc/change-list.md`：193 operations 冻结，上一版本 client
  向后兼容（`check-client-compat.rb` 193/193），无未批准破坏性变更。
- 差异文档同步 `reports/rc/doc-sync.md`：Requirements/OpenAPI/Schema/Security/Testing
  与专项文档逐项核对。
- OpenAPI 覆盖终态 `reports/rc/coverage-final.md`：193/193 全部 `verified`
  （含 profile-cover 端点补齐与 posts 反应删除端点补齐）。
- 迁移兼容/升级时长/恢复点 `reports/rc/migration-compat.md`；依赖/SBOM/Secret
  清点 `reports/rc/inventory.md`。
- 评审签字：**阻塞**（需产品/后端/前端/安全/测试/运维/运营负责人评审签字）。

### 预发布环境与数据演练（M17-ENV）

- `deploy/staging/` 生产同构编排说明；合成 persona 数据 + canary 日志扫描 CLEAN。
- 空库安装/升级/重复迁移/错误迁移演练、SQLite/附件/OIDC key 备份恢复
  （RPO=0，RTO=0.18s，verify.sh 全绿）、优雅停机（HTTP 0.30s / worker 0.04s）。
- MySQL/MariaDB 恢复演练：**阻塞**（沙箱无真实数据库；脚本就绪）。

### 全角色冒烟（M17-SMOKE）

- 九类 persona 冒烟全绿（Playwright 194 用例 + vitest 567 + 后端 147 binaries），
  报告 `reports/rc/smoke/personas.md`；人工验收清单 `reports/rc/smoke/checklist.md`。

### 专项 Flag 与启用记录（M17-FLAGS）

- 核心论坛/邮箱验证/审核/积分/本地附件默认配置上线；五项可选能力（AI/Video/
  Download Billing/OIDC/Marketplace）默认关闭，逐项启用计划与回滚记录
  `ops/feature-flags/gates.md`（P2 记录审批人/范围/阈值/观察窗口/审计）。

### 法律与上线（M17-LEGAL / M17-LAUNCH）

- 法律/运营/隐私发布确认清单 `docs/legal/README.md`：**阻塞**（需法务/运营签字）。
- 生产上线：**阻塞**（需真实生产主机执行；步骤与 Runbook 已就绪）。

---
## v0.7 — 2026-08-08（M16 测试/安全/故障/经济/性能/发布验收）

### 测试基础设施与契约（M16-HARNESS）

- 稳定错误码四方一致（docs ↔ OpenAPI ↔ backend ↔ frontend）：openapi
  `Problem.code` 同步为 106 码；backend 领域错误转换（marketplace/shop/download/
  activity/ai）输出稳定码而非通用 `conflict/bad_request`；`scripts/check-code-fixtures.rb`
  强制每个稳定码有 Fixture + 前端映射。
- 状态机合法/非法迁移矩阵 `reports/rc/state-machine-coverage.md` +
  `scripts/check-state-machine-matrix.rb`。
- 上一版本生成 client 向后兼容：`compat/frozen-client/`（M15 契约冻结）+
  `scripts/check-client-compat.rb`（操作表面/请求参数/请求体/响应 schema/enum 全兼容）。
- 契约边缘测试 `backend/tests/harness_contract.rs`（最大 limit 钳制/未知参数/非法
  游标 400/cursor 不重不漏）。
- Fixture 约定文档 `docs/FIXTURES.md`；CI 四层 `docs/CI-LAYERS.md` +
  `.github/workflows/{nightly,release-rc}.yml`（PR/nightly/RC/prod-smoke）。
- `check-openapi.rb` 基线冻结 193；`bblbb-migrate` 二进制命名统一（Cargo.toml `[[bin]]`）。

### 安全（M16-SECURITY）

- OWASP ASVS v4.0.3 基线映射 `security/ASVS-BASELINE.md`（含排除项/负责人/证据）。
- 隐藏内容防泄漏扫漏 `security/leak-sweep.md`（16 渠道，PASS）。
- 依赖/Secret/许可证/SBOM 扫描 `ops/security/scan.sh`（Secret OK；cargo audit
  4 项上游固定/无修复按风险接受并跟踪；SBOM 634 组件）。

### 存储故障与外部失败（M16-STORAGE-FAULTS）

- `backend/tests/storage/adapter.rs`：Local/S3 adapter contract + S3 mock 故障注入
  （403/404/429/5xx→稳定分类与 retryable）+ multipart 生命周期 + 预签名 URL/重签。
- `backend/tests/faults.rs`：外部失败不变量（URL 签发失败整体回滚、幂等不重复
  扣费、账本恒等式）。

### 经济（M16-ECONOMY）

- `backend/tests/economy/step_injection.rs`：每一步注入失败 → 无订单/权益/流水/
  Outbox/审计残留（余额不足/库存/限购/等级门槛/幂等冲突 5 用例）。

### 性能（M16-PERF）

- `bench/gen-synthetic.sh` 合成数据：100k 用户 / 1M 帖子 / 200k 评论，
  DB 1137MB；`bench/measure.sh` release 真实请求 p95 基线
  `reports/perf/baseline.md`（详情/搜索/登录/发帖/回复 16–18ms、SSR 19–24ms、
  无过滤列表 1207ms 已知慢查询、RSS 35MB）；阈值版本化 `bench/thresholds.md`。

### 发布验收（M16-RELEASE-TEST）

- `reports/rc/`：harness.md / release-test.md / failure-template.md /
  smoke/checklist.md / p0-p1.md / state-machine-coverage.md。
- 演练实测：迁移升级 apply_ms=125 · 备份恢复 RPO=0/RTO=0.18s · 冒烟 PASS=14 ·
  优雅停机 PASS=8 · release bundle PASS=26 · alerts PASS=71 · Playwright 194 passed。

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
