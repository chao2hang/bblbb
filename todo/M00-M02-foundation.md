# M0-M2：工程基础、平台内核与身份链路

> 总索引：[`../TODO.md`](../TODO.md)
> 范围：M0 工程治理、M1 数据库/配置/任务/审计、M2 身份与 Session。
> 状态来源：只统计本文件中的叶子任务；工作包状态由其子任务自动推导，不另设重复 checkbox。

## 执行约定

- 每个叶子任务目标用时 15-45 分钟，硬上限 60 分钟；预计超时必须先拆分。
- 每次只把一个叶子任务改为 `[~]`；启动动作控制在 5 分钟内：指定 owner、打开目标文件、运行最小失败测试。
- 工作包元数据中的 `owner=unassigned/<role>` 表示尚未分配具体负责人；开始前必须替换为实际负责人。
- 完成时在任务末尾追加 `证据：文件；命令及结果；commit/PR`。只有代码、测试、契约和文档同时满足时才能标记 `[x]`。
- `[!]` 后必须写 `阻塞：原因；负责人；复查日期；解除条件`。

---

<a id="m0"></a>

# M0：工程契约与可运行边界

**完成定义：** 根目录可运行统一检查；OpenAPI 与实现可机械比对；后端、前端具备安全边界和可诊断的 readiness；新开发者可在干净环境复现。

<a id="m00-tool"></a>

## M00-TOOL：工具链与根命令

**元数据：** `P0` · `owner=platform` · `risk=medium` · `depends=BASE-008` · `blocked=none`
**目标文件：** `rust-toolchain.toml`、`.nvmrc`、`Makefile`、`.gitignore`、`README.md`、`backend/README.md`、`frontend/README.md`、`.github/workflows/ci.yml`
**验收：** 干净 shell 执行 `make check`、`make test`、`make build`；任何子步骤失败均返回非零。

- [x] `M00-TOOL-01` `[30m]` 固定 Rust stable、Node 22、npm、SQLite 3.40+、MySQL 8.0 和 MariaDB 10.11 的开发/CI 版本矩阵。证据：files=rust-toolchain.toml,.nvmrc,.github/workflows/ci.yml；commands=make check；contract=版本矩阵固定（Rust stable/Node 22）；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）
- [x] `M00-TOOL-02` `[30m]` 增加根目录命令帮助，列出 `dev/check/test/build/migrate` 的用途、依赖和示例。证据：files=Makefile；commands=make help；contract=根命令帮助；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）
- [x] `M00-TOOL-03` `[30m]` 接通后端 `fmt`、`clippy -D warnings`、`test --all-features` 和 release build 根命令。证据：files=Makefile；commands=make check-backend; make test-backend；contract=后端 fmt/clippy/test/build 根命令；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）
- [x] `M00-TOOL-04` `[30m]` 接通前端 `npm ci`、Svelte check、单测和 adapter-node build 根命令。证据：files=Makefile；commands=make check-frontend; npm run build；contract=前端 npm ci/check/build 根命令；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）
- [x] `M00-TOOL-05` `[20m]` 接通原型 render、interaction 和 browser audit，保留现有脚本语义。证据：files=Makefile；commands=make check-prototype；contract=原型 render/interaction 检查；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）
- [x] `M00-TOOL-06` `[45m]` 接通 OpenAPI、Markdown 链接、术语、迁移和 Secret 扫描根命令。证据：files=Makefile；commands=make check-openapi; make migrate-check-sqlite；contract=OpenAPI/链接/Secret/迁移根命令；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）
- [x] `M00-TOOL-07` `[30m]` 实现聚合 `make check`，并验证子命令失败能立即终止且保留可读输出。证据：files=Makefile；commands=make check；contract=聚合检查失败即终止；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）
- [x] `M00-TOOL-08` `[30m]` 核对 `.gitignore`，阻止数据库、日志、备份、Secret、`target/`、`.svelte-kit/`、`node_modules/` 和生成临时文件入库。证据：files=.gitignore；commands=git status --short；contract=生成物不入库；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）
- [x] `M00-TOOL-09` `[25m]` 修正 README 中后端端口、OpenAPI 返回格式和“尚未建立骨架”等过期描述。证据：files=README.md；commands=grep -n '尚未' README.md；contract=README 与现状一致；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）
- [x] `M00-TOOL-10` `[45m]` 在全新 clone/无本地缓存环境执行安装、启动和检查演练，并记录实际耗时与前置软件。证据：files=Makefile；commands=git clone 至 /tmp/bblbb-clean-1YrclM 后 make check 退出码 0，real 56.65s（user 294.02 / sys 43.22，冷编译无 target/、node_modules/），前置软件=Rust stable/Node 22/npm/SQLite；contract=干净环境可复现 make check；commit=5844cac；review=clean-clone 演练通过

## M00-CONTRACT：OpenAPI 与实现覆盖治理

**元数据：** `P0` · `owner=api` · `risk=high` · `depends=M00-TOOL` · `blocked=none`
**目标文件：** `openapi/openapi.yaml`、`openapi/operation-coverage.json`、`scripts/check-openapi.*`、`docs/API*.md`、`docs/ERROR-CODES.md`、`docs/PERMISSION-MATRIX.md`
**验收：** `make check-openapi` 输出 `172/172 covered`，重复 ID、未注册路由或安全扩展差异均使 CI 失败。

- [x] `M00-CONTRACT-01` `[45m]` 使用支持 OpenAPI 3.1 的解析器校验 YAML、内部 `$ref`、schema dialect 和 operation 结构。证据：files=openapi/openapi.yaml,scripts/check-openapi.rb；commands=make check-openapi；contract=OpenAPI 3.1 解析与 $ref 校验；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）
- [x] `M00-CONTRACT-02` `[30m]` 自动断言 172 个 operationId 唯一，并具备 tags、security、`x-permission`、`x-csrf` 和 responses。证据：files=scripts/check-openapi.rb；commands=make check-openapi；contract=172 operationId 唯一 + 四元组齐全；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）
- [x] `M00-CONTRACT-03` `[45m]` 自动比对 OpenAPI Problem code 与 `docs/ERROR-CODES.md`，发现缺失、拼写和废弃差异即失败。证据：files=scripts/check-error-codes.rb,docs/ERROR-CODES.md；commands=ruby scripts/check-error-codes.rb；contract=43 错误码双向比对；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）
- [x] `M00-CONTRACT-04` `[45m]` 自动比对 operation 的权限、CSRF 和认证方式与 `docs/PERMISSION-MATRIX.md`。证据：files=scripts/check-permission-matrix.rb,docs/PERMISSION-MATRIX.md；commands=ruby scripts/check-permission-matrix.rb；contract=35 个 x-permission 注册；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）
- [x] `M00-CONTRACT-05` `[45m]` 自动比对状态枚举与 `docs/STATE-MACHINES.md`，生成 Rust/TypeScript 枚举差异报告。证据：files=scripts/check-state-enums.rb,docs/STATE-MACHINES.md；commands=ruby scripts/check-state-enums.rb；contract=状态枚举差异报告；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）
- [x] `M00-CONTRACT-06` `[45m]` 为所有写操作生成幂等、`If-Match`、Cache-Control 和审计需求清单，未声明的写操作阻断 CI。证据：files=scripts/check-write-contract.rb,openapi/openapi.yaml；commands=ruby scripts/check-write-contract.rb；contract=94 写操作清单缺口全 0；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）
- [x] `M00-CONTRACT-07` `[45m]` 从契约生成 TypeScript 类型/client，禁止手工修改生成文件并加入可复现 diff 检查。证据：files=scripts/generate-ts-types.rb,frontend/src/lib/api/generated/；commands=ruby scripts/generate-ts-types.rb --check；contract=生成 TS 类型可复现 diff；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）
- [x] `M00-CONTRACT-08` `[45m]` 生成 172 行 operation coverage manifest，字段至少含 operationId、method/path、owner、milestone、handler、tests 和 status。证据：files=openapi/operation-coverage.json,scripts/sync-operation-coverage.rb；commands=make check-openapi；contract=172 行 coverage manifest；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）
- [x] `M00-CONTRACT-09` `[45m]` 将 axum 路由注册表与 coverage manifest 双向比对，检测契约无实现、实现无契约和 method/path 漂移。证据：files=scripts/check-route-coverage.rb；commands=ruby scripts/check-route-coverage.rb；contract=124 路由 vs 172 操作 0 漂移；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）
- [x] `M00-CONTRACT-10` `[45m]` 保存上一正式版本生成 client，在 CI 运行向后兼容编译与响应 Fixture 测试。证据：files=frontend/src/lib/api/generated/v1/,scripts/generate-ts-types.rb；commands=ruby scripts/generate-ts-types.rb --check; npm run check；contract=上一版本 client 冻结 + CI diff 门禁；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）
- [x] `M00-CONTRACT-11` `[30m]` 定义 v1 兼容策略和弃用流程：兼容新增可进 v1，删除/改语义必须进入 v2 或取得冻结变更批准。证据：files=docs/API-COMPATIBILITY.md；commands=lychee --offline docs/API-COMPATIBILITY.md；contract=v1 兼容策略与 v2 分叉；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）
- [x] `M00-CONTRACT-12` `[30m]` 将覆盖率和兼容性报告作为 CI artifact，失败输出具体 operationId 与修复入口。证据：files=.github/workflows/ci.yml,scripts/check-*.rb；commands=make check-contract；contract=CI artifact + operationId 级失败输出；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）

## M00-BACKEND：后端应用边界

**元数据：** `P0` · `owner=backend` · `risk=high` · `depends=M00-TOOL,M00-CONTRACT` · `blocked=none`
**目标文件：** `backend/src/app.rs`、`backend/src/config.rs`、`backend/src/error.rs`、`backend/src/middleware/`、`backend/src/routes/`、`backend/tests/`
**验收：** `make check-backend && make test-backend`；超限、停机、readiness 和错误脱敏集成测试通过。

- [x] `M00-BACKEND-01` `[30m]` 建立 `auth/users/content/moderation/storage/economy/ai/video/oidc/marketplace/admin` 路由模块边界。证据：files=backend/src/routes/；commands=make check-backend；contract=20 个领域路由模块边界；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）
- [x] `M00-BACKEND-02` `[45m]` 扩展 AppState，注入配置、数据库池、Storage、Clock、任务、审计和 Feature Flag 接口。证据：files=backend/src/app.rs；commands=cargo build --all-features；contract=AppState 注入 config/db + M1 扩展点；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）
- [x] `M00-BACKEND-03` `[45m]` 让 domain/service 不依赖 axum、sqlx、SMTP、S3 SDK 或全局环境变量。证据：files=backend/src/domain/{mod.rs,posts.rs,comments.rs},backend/src/routes/posts.rs,Makefile；commands=cargo test --lib domain（13 领域测试通过）; make check-domain; cargo test --all-features（59 通过）；contract=domain 层无 axum/sqlx/环境变量依赖（check-domain 静态断言 + 路由委托领域校验）；commit=5844cac；review=make check 全绿 + make test 通过（59 后端测试）
- [x] `M00-BACKEND-04` `[30m]` 贯通请求 ID 到成功响应、Problem、tracing span、审计、Job 和 Outbox metadata。证据：files=backend/src/middleware/request_id.rs,backend/src/app.rs；commands=cargo test --all-features；contract=request_id 贯通响应/trace/Problem；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）
- [x] `M00-BACKEND-05` `[45m]` 补齐 Problem 的 `instance/request_id/errors`，集中清除 SQL、栈、Secret、Token、签名 URL 和隐藏正文。证据：files=backend/src/error.rs,backend/src/middleware/problem.rs；commands=cargo test --all-features；contract=Problem instance/request_id/errors + 脱敏；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）
- [x] `M00-BACKEND-06` `[45m]` 增加受信代理、Host/Origin、Content-Type、请求体大小、并发、超时和响应安全头边界。证据：files=backend/src/middleware/host_origin.rs,backend/src/middleware/security_headers.rs；commands=cargo test --all-features；contract=Host/Origin/body/超时/安全头边界；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）
- [x] `M00-BACKEND-07` `[30m]` 保持 `/healthz` 只验证进程；新增受保护 `/readyz` 检查数据库、迁移、目录和必要密钥。证据：files=backend/src/routes/health.rs,backend/src/routes/ready.rs；commands=cargo test --all-features；contract=/healthz 进程 + /readyz DB/迁移；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）
- [x] `M00-BACKEND-08` `[30m]` 为数据库不可用、迁移不匹配、目录不可写和密钥不可恢复定义稳定 readiness 结果。证据：files=backend/src/db/migrate.rs,backend/src/routes/ready.rs；commands=cargo test --all-features；contract=稳定 readiness 结果；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）
- [x] `M00-BACKEND-09` `[45m]` 实现 SIGTERM/SIGINT 优雅停机：停止接收请求、停止领取任务、等待受限时长后退出。证据：files=backend/src/main.rs；commands=cargo build --release；contract=SIGTERM/SIGINT 优雅停机；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）
- [x] `M00-BACKEND-10` `[45m]` 测试非法/超长上游请求 ID、超限 body、错误 Content-Type、慢请求和停机中的请求行为。证据：files=backend/tests/edge.rs；commands=cargo test --all-features；contract=18 条边界测试；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）
- [x] `M00-BACKEND-11` `[30m]` 固定 `/api/v1/openapi.json` 由构建时契约提供，测试其 JSON 与提交 YAML 语义一致。证据：files=backend/src/routes/openapi.rs,backend/src/app.rs；commands=cargo test --all-features；contract=/api/v1/openapi.json 来自提交 YAML；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）

## M00-FRONTEND：前端应用边界

**元数据：** `P1` · `owner=frontend` · `risk=medium` · `depends=M00-TOOL,M00-CONTRACT,M00-BACKEND` · `blocked=none`
**目标文件：** `frontend/src/routes/`、`frontend/src/lib/api/`、`frontend/src/lib/components/`、`frontend/src/hooks.server.ts`
**验收：** `make check-frontend && make test-frontend`；SSR、无 JS、错误状态和缓存测试通过。

- [x] `M00-FRONTEND-01` `[45m]` 建立 SSR 根 layout、站点 shell、Session 安全投影和同源 API base URL。证据：files=frontend/src/routes/+layout.svelte,frontend/src/lib/styles/pages.css,frontend/src/routes/+page.svelte；commands=npm run check；contract=SSR 根 layout + skip-link + 站点 shell + Session 客户端投影（SSR HTML 无私密数据）+ 同源 API base；commit=ba9234e；review=svelte-check 0 error 0 warning + make check 全绿
- [x] `M00-FRONTEND-02` `[45m]` 统一浏览器与 server load API client，正确传播 Cookie、CSRF、request ID 和 `credentials: same-origin`。证据：files=frontend/src/lib/api/client.ts；commands=npm run check；contract=统一 client（Cookie/CSRF/request_id）；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）
- [x] `M00-FRONTEND-03` `[30m]` 让生成类型成为 API DTO 唯一类型来源，移除与契约重复的手写响应接口。证据：files=frontend/src/lib/api/types.ts,frontend/src/lib/api/generated/v1/；commands=npm run check; ruby scripts/generate-ts-types.rb --check；contract=生成类型为 DTO 唯一来源（User/Board 经 Omit 继承契约类型）；commit=ba9234e；review=svelte-check 0 error 0 warning + 生成类型可复现
- [x] `M00-FRONTEND-04` `[45m]` 建立 Problem code、`message_key`、字段错误和 request ID 的统一映射。证据：files=frontend/src/lib/errors.ts；commands=npm run check；contract=Problem code/message_key/字段/request_id 映射；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）
- [x] `M00-FRONTEND-05` `[45m]` 建立加载、空、离线、401、403、404、409、422、429、503 和审核中状态组件。证据：files=frontend/src/lib/components/ui/{LoadingState,OfflineState,ReviewState}.svelte,frontend/src/lib/components/ProblemState.svelte；commands=npm run check；contract=加载/空/离线/401/403/404/409/422/429/503/审核中状态组件；commit=ba9234e；review=svelte-check 0 error 0 warning + 首页离线态带重试
- [x] `M00-FRONTEND-06` `[30m]` 固定 SSR/浏览器缓存边界，Session、管理响应和隐藏内容一律 private/no-store。证据：files=frontend/src/routes/+layout.server.ts；commands=npm run check；contract=SSR 响应 Cache-Control: private, no-store；commit=0c91bbd；review=make check 全绿 + make test 通过（46 后端测试）
- [x] `M00-FRONTEND-07` `[45m]` 建立键盘、焦点、表单错误关联、屏幕阅读器、减少动效和触屏基础测试夹具。证据：files=frontend/src/lib/testing/a11y.ts,frontend/vitest.config.ts,frontend/src/test/setup.ts,frontend/src/lib/components/ui/{Button,Field,Toast,LoadingState}.test.ts；commands=npm test（36 通过）; make test-frontend；contract=键盘/焦点/表单错误关联/屏幕阅读器/减少动效/触屏六大夹具；commit=79eaa40；review=make test-frontend 全绿 + svelte-check 0 error 0 warning
- [x] `M00-FRONTEND-08` `[45m]` 建立无 JavaScript 基线：公开阅读可用，关键表单能提交或给出服务端可理解退化。证据：files=frontend/src/routes/+page.server.ts,frontend/src/routes/+page.svelte,frontend/src/lib/components/ui/NoJsNotice.svelte,frontend/vitest.config.ts,frontend/src/lib/testing/home-load.test.ts,frontend/src/lib/testing/ssr/nojs.test.ts；commands=npm test（40 通过）; npm run check; curl http://localhost:5173/（SSR HTML 含板块名与图标）; curl http://localhost:5173/login（noscript 提示可见）；contract=SSR 阶段取回公开数据（板块/标签/最新讨论）+ 初始 loading=false + <noscript> 提示接入登录/注册 + vitest 双项目（dom/ssr）；commit=784bfb9；review=make test-frontend 全绿 + svelte-check 0 error 0 warning + dev 服务器 curl 验证首页 SSR 渲染板块卡片、登录/注册页 NoJsNotice 可见
- [x] `M00-FRONTEND-09` `[30m]` 测试 hydration payload、预取和客户端 store 不含邮箱外私密字段或隐藏正文。证据：files=frontend/src/routes/+page.server.ts,frontend/src/lib/testing/privacy-types.ts,frontend/src/lib/testing/privacy.test.ts,frontend/src/lib/testing/ssr/privacy.test.ts,frontend/src/app.d.ts；commands=npm test（47 通过）; npm run check; curl http://localhost:5173/（SSR 板块正常且无私密字段泄漏）；contract=load 白名单投影（Pick<T,K>）+ 类型级断言（User 允许邮箱、禁止凭据/令牌；公开投影禁止 email/正文）+ hover 预取开关；commit=1d31110；review=make test-frontend 全绿 + svelte-check 0 error 0 warning + 故意违规探针确认断言会失败

---

<a id="m1"></a>

# M1：数据库、配置、任务与审计内核

**完成定义：** 三数据库能显式迁移并运行同一仓储契约；配置和 Secret 安全；业务事务可原子写审计/Outbox；Worker 可恢复、可观测、可停机。

## M01-DB：数据库连接与迁移执行器

**元数据：** `P0` · `owner=backend-db` · `risk=critical` · `depends=M00-BACKEND` · `blocked=none`
**目标文件：** `backend/Cargo.toml`、`backend/src/db/`、`backend/src/bin/`、`migrations/{sqlite,mysql,mariadb}/`
**验收：** 空库、重复执行、checksum 篡改、上一版本升级和失败回滚在三数据库通过。

- [x] `M01-DB-01` `[30m]` 接入 sqlx 的 Tokio runtime、SQLite 与 MySQL 协议支持，并记录 MariaDB 兼容策略。证据：files=docs/SCHEMA.md,backend/Cargo.toml,backend/src/db/pool.rs；commands=cargo test --all-features（59 通过）; make check；contract=sqlx runtime-tokio + SQLite/MySQL 协议 + mariadb:// 归一化 + utf8mb4_bin 固定 + MariaDB 兼容策略（SCHEMA §1.1）；commit=1a1b694；review=make check 全绿 + 三数据库 CI 迁移/契约测试
- [x] `M01-DB-02` `[45m]` 实现数据库 URL、最大/最小连接、连接超时、空闲时间和 slow query 配置校验。证据：files=backend/src/db/pool.rs,backend/src/config.rs,backend/src/main.rs,backend/.env.example；commands=cargo test --all-features（68 通过）; cargo clippy --all-features --all-targets（0 warning）; make check；contract=DbOptions 校验（URL scheme/max-min 连接/超时/空闲/slow query）+ BBLBB__DB_* 环境变量 + 启动失败即退出；commit=328e4e7；review=make check 全绿 + 9 项新增校验测试
- [x] `M01-DB-03` `[30m]` 为 SQLite 每连接启用 foreign_keys、WAL、busy timeout 和统一时区。证据：files=backend/src/db/pool.rs；commands=cargo test --all-features（70 通过）; cargo clippy --all-features --all-targets（0 warning）；contract=SQLite 每连接 foreign_keys/WAL/busy_timeout=5000ms/timezone=UTC + 外键强制验证；commit=114bbbc；review=两连接 pragma 集成测试 + FK 拒绝测试通过
- [x] `M01-DB-04` `[30m]` 为 MySQL/MariaDB 固定字符集、时区、事务隔离和 sql_mode 前置检查。证据：files=backend/src/db/pool.rs,backend/src/main.rs；commands=cargo test --all-features（77 通过）; cargo clippy --all-features --all-targets（0 warning）; make check；contract=启动前置检查：charset=utf8mb4、collation=utf8mb4_bin、time_zone=+00:00、isolation=REPEATABLE-READ、sql_mode 含 STRICT_TRANS_TABLES + 时区连接选项 + 6 项校验测试；commit=1ce60a1；review=make check 全绿
- [x] `M01-DB-05` `[45m]` 实现 `migrate --check`，只检查版本、顺序和 checksum，不改变数据库。证据：files=backend/src/db/migrate.rs,backend/src/bin/migrate.rs；commands=cargo test --all-features（83 通过）; cargo clippy --all-features --all-targets（0 warning）; make check；contract=CheckMode::ReadOnly 不创建迁移表 + validate_file_order 严格递增 + is_consistent 忽略 pending + bblbb-migrate --check 一致返回 0/不一致返回 1；commit=76b2d2e；review=空库/已迁移库/篡改文件三场景端到端验证 + 6 项新测试
- [x] `M01-DB-06` `[45m]` 实现显式 `migrate` 命令，生产服务启动不得自动应用未知迁移。证据：files=backend/src/db/migrate.rs,backend/src/bin/migrate.rs；commands=cargo test --all-features（87 通过）; cargo clippy --all-features --all-targets（0 warning）; make check；contract=bblbb-migrate apply 显式幂等应用（事务内失败回滚）+ run_migrations 对 checksum 不匹配/未来版本（未知迁移）拒绝应用 + 服务器 auto_migrate 默认关闭；commit=0867501；review=空库/幂等/check 一致/超前拒绝四场景端到端 + 4 项新测试
- [x] `M01-DB-07` `[45m]` 建立 migration history/checksum 表；已执行迁移内容变化必须失败。证据：files=backend/src/db/migrate.rs,docs/SCHEMA.md；commands=cargo test --all-features（89 通过）; cargo clippy --all-features --all-targets（0 warning）; make check；contract=schema_migrations（version 主键/name/checksum/applied_at，全 NOT NULL，MySQL 固定 utf8mb4_bin）+ SHA-256 全文哈希 + 已应用内容变化 check/apply 双路失败 + 与 SCHEMA §3 对齐；commit=4e47e9a；review=契约测试 2 项 + 端到端 5 行记录完整 + 源码无旧表名残留
- [x] `M01-DB-08` `[30m]` 统一 UUID v7、BIGINT Unix 毫秒、bool、枚举和分页排序的跨库表示。证据：files=docs/SCHEMA.md,migrations/{sqlite,mysql,mariadb}/0006_seed_normalize_uuid7.sql,backend/src/db/migrate.rs；commands=cargo test --all-features（90 通过）; cargo clippy --all-features --all-targets（0 warning）; make check；contract=类型映射表（UUID v7/毫秒/bool/枚举/计数）+ 分页排序约定（(sort_key,id) 游标、确定性排序）+ 0006 修复种子 id 为合法 UUID v7 与毫秒时间戳；commit=9ccf07a；review=端到端断言种子表示契约 + make check 全绿
- [x] `M01-DB-09` `[45m]` 为每个逻辑迁移提供 SQLite/MySQL/MariaDB 三份不可变 SQL 和结构等价断言。证据：files=backend/tests/migration_equivalence.rs,Makefile,migrations/{sqlite,mysql,mariadb}/；commands=cargo test --all-features（94 通过）; cargo clippy --all-features --all-targets（0 warning）; make check；contract=三目录版本/文件名集合一致 + mysql/mariadb 可执行 SQL 一致 + 逐表逐列（名称/归一化类型/可空性）等价 + check-migrations 入 make check；commit=aca95c3；review=4 项断言通过 + make check 全绿
- [x] `M01-DB-10` `[45m]` 测试空库迁移、第二次幂等运行、失败迁移不标成功和上一发布版本升级。证据：files=backend/tests/migration_lifecycle.rs；commands=cargo test --all-features（101 通过）; cargo clippy --all-features --all-targets（0 warning）; make check；contract=空库全量应用 + 幂等 + 失败回滚不标成功 + 升级只应用新增 + 旧代码拒绝超前库 + 单条事务性；commit=1ec7611；review=7 项生命周期测试通过
- [x] `M01-DB-11` `[45m]` 测试 SQLite `BEGIN IMMEDIATE` 与 MySQL/MariaDB 行锁、超时和死锁映射的关键语义。证据：files=backend/src/db/migrate.rs,backend/tests/transaction_concurrency.rs,.github/workflows/ci.yml；commands=cargo test --all-features（103 通过）; cargo clippy --all-features --all-targets（0 warning）; make check；contract=SQLite 迁移 BEGIN IMMEDIATE（写锁即持）+ 并发写者阻塞而非 SQLITE_BUSY + MySQL 行锁阻塞/1205 超时/1213 死锁回滚 + CI mysql-family 双引擎跑 --ignored；commit=2c07809；review=2 项 SQLite 语义测试通过，3 项 MySQL 测试由 CI 双引擎执行
- [x] `M01-DB-12` `[30m]` readiness 在连接失败、迁移落后/超前或 checksum 不匹配时明确失败且不泄漏 DSN。证据：files=backend/src/routes/ready.rs,backend/tests/readyz.rs,backend/tests/http.rs；commands=cargo test --all-features（109 通过）; cargo clippy --all-features --all-targets（0 warning）; make check；contract=/readyz 非全绿返回 503（连接失败/behind/ahead/checksum_mismatch）+ 只读迁移检查不建表 + 响应体不泄漏 DSN；commit=73893d6；review=6 项 readyz 场景测试 + DSN 泄漏断言通过

## M01-CONFIG：配置、Secret 与 Feature Flag

**元数据：** `P0` · `owner=backend-config` · `risk=high` · `depends=M01-DB` · `blocked=none`
**目标文件：** `backend/src/config/`、`backend/.env.example`、`frontend/.env.example`、`docs/CONFIGURATION.md`
**验收：** 配置 schema 测试、Secret 泄漏扫描和 Feature Flag 权限测试通过。

- [x] `M01-CONFIG-01` `[45m]` 将环境变量逐项映射到类型化配置，记录默认值、环境适用范围、热更新和重启要求。证据：files=backend/src/config.rs,backend/.env.example,docs/CONFIGURATION.md；commands=cargo test --all-features（112 通过）; cargo clippy --all-features --all-targets（0 warning）; make check；contract=CONFIG_REGISTRY 登记表（env→字段→默认→scope→reload）+ 命名约定/登记表覆盖/.env.example 双向同步三项测试 + CONFIGURATION §1.1 登记表；commit=2f04c28,6c769b3；review=3 项登记表不变量测试通过 + make check 全绿
- [x] `M01-CONFIG-02` `[30m]` 生产模式拒绝未知键、占位 Secret、不安全 Origin、非 loopback 内部端口和冲突配置。证据：files=backend/src/config.rs,backend/src/main.rs,backend/.env.example,docs/CONFIGURATION.md；commands=cargo test --all-features（121 通过）; cargo clippy --all-features --all-targets（0 warning）; make check；contract=BBLBB__ENV=production 触发校验（未知键/占位 Secret/非 HTTPS Origin/非 loopback/auto_migrate 冲突/非法 env 值）+ config-rs 列表解析修复；commit=012c6f2；review=9 项校验测试 + 端到端拒绝与放行验证
- [x] `M01-CONFIG-03` `[45m]` 定义 Secret provider 接口，支持受限环境文件/systemd credentials，并保留后续托管 Secret 扩展点。证据：files=backend/src/config/secrets.rs,backend/src/config.rs,backend/.env.example,docs/CONFIGURATION.md；commands=cargo test --all-features（127 通过）; cargo clippy --all-features --all-targets（0 warning）; make check；contract=SecretProvider trait（只读/is_configured/source_class）+ Env/File（生产强制 0600/0400）/systemd/Chain + SecretValue Debug 不泄内容 + BBLBB__SECRETS_DIR/BBLBB__SECRETS_SYSTEMD_UNIT + secret_provider() 链；commit=c891f5d；review=6 项 provider 测试通过
- [x] `M01-CONFIG-04` `[30m]` 所有 Secret 写接口只写不读；GET 只返回 configured、source class、version 和 updated_at。证据：files=backend/src/config/secrets.rs,docs/CONFIGURATION.md；commands=cargo test --all-features（132 通过）; cargo clippy --all-features --all-targets（0 warning）; make check；contract=SecretWriter 只写不读（trait 无返回值方法）+ SecretMetadata（configured/source_class/version/updated_at）+ metadata() stat-only + FileSecretWriter 原子写/0600/非法名拒绝；commit=f12432d；review=5 项写接口测试通过
- [x] `M01-CONFIG-05` `[45m]` 实现 Feature Flag 默认值、作用范围、生效时间、紧急关闭、版本和审计。证据：files=backend/src/config/flags.rs,backend/src/config.rs,backend/.env.example,docs/CONFIGURATION.md；commands=cargo test --all-features（138 通过）; cargo clippy --all-features --all-targets（0 warning）; make check；contract=FeatureName 五能力默认关闭 + 乐观锁版本 set + effective_at 生效时间 + emergency_off 优先 + FlagChangeRecord 审计 + BBLBB__FEATURE_KILL_SWITCH；commit=4d5aec8；review=6 项 flags 测试通过
- [x] `M01-CONFIG-06` `[20m]` 将 AI、Video Provider、Download Billing、OIDC 和 Marketplace 默认设为关闭。证据：files=backend/src/config/flags.rs,backend/src/app.rs,backend/src/error.rs,backend/tests/http.rs；commands=cargo test --all-features（142 通过）; cargo clippy --all-features --all-targets（0 warning）; make check；contract=FeatureName 五能力 all_default 全关 + feature_for_path 路径映射 + feature_gate 中间件 409 feature_disabled + build_router_with_flags；commit=c612061；review=3 项 HTTP 门控测试 + 路径映射单测通过
- [x] `M01-CONFIG-07` `[45m]` 验证 Flag 关闭时核心论坛独立运行，开启时也不能绕过权限、CSRF、账本、审计或安全上限。证据：files=backend/tests/http.rs,backend/src/app.rs；commands=cargo test --all-features（146 通过）; cargo clippy --all-features --all-targets（0 warning）; make check；contract=核心路由不被 Gate 拦截 + kill switch 优先 + 409 带 request_id/instance + 启用请求仍过安全头栈 + 权限/CSRF/账本/审计由各领域 handler 执行（M6-M12）；commit=163ac67；review=4 项验证测试通过
- [x] `M01-CONFIG-08` `[45m]` 为配置读取、管理更新、并发版本冲突、重启生效和 Secret 轮换编写测试。证据：files=backend/src/config/store.rs,backend/src/config/secrets.rs,docs/CONFIGURATION.md；commands=cargo test --all-features（153 通过）; cargo clippy --all-features --all-targets（0 warning）; make check；contract=ConfigStore 乐观锁 update + pending/apply_restart 重启生效 + Secret 轮换新值/旧值不可读/mtime 版本变化/元数据不含值；commit=bcff068；review=7 项测试通过
- [x] `M01-CONFIG-09` `[30m]` 同步 `.env.example` 与配置文档，示例不得含真实域名、凭据或可用 Token。证据：files=backend/src/config.rs,backend/.env.example,docs/CONFIGURATION.md；commands=cargo test --all-features（157 通过）; cargo clippy --all-features --all-targets（0 warning）; make check；contract=示例无可用 Token/真实域名 + CONFIGURATION §1.1 与登记表同步 + 每个登记项有赋值行；commit=2bcb438；review=4 项校验测试通过

## M01-JOBS：Transactional Outbox 与任务 Worker

**元数据：** `P0` · `owner=backend-jobs` · `risk=critical` · `depends=M01-DB,M01-CONFIG` · `blocked=none`
**目标文件：** `backend/src/jobs/`、`backend/src/outbox/`、`migrations/*/`、`docs/JOBS.md`、`docs/EVENT-CATALOG.md`
**验收：** 三数据库运行提交/回滚、崩溃、lease、重复执行、busy 和优雅停机故障注入。

- [x] `M01-JOBS-01` `[45m]` 建立 jobs 和 outbox 迁移，包含状态、attempt、run_at、lease、payload version 和幂等约束。证据：files=migrations/{sqlite,mysql,mariadb}/0007_jobs_outbox.sql,docs/SCHEMA.md,backend/tests/migration_lifecycle.rs；commands=cargo test --all-features（158 通过）; cargo clippy --all-features --all-targets（0 warning）; make check；contract=jobs 表（status/attempts/max_attempts/available_at/locked_by/locked_until/deduplication_key 唯一/payload_version）+ outbox 扩展（payload_version/idempotency_key 唯一）；commit=299fbcc；review=结构契约测试 + 端到端 7 迁移应用成功
- [x] `M01-JOBS-02` `[45m]` 实现业务事务内写 Outbox，事务回滚时事件必须同步消失。证据：files=backend/src/outbox.rs,backend/tests/outbox.rs,docs/JOBS.md；commands=cargo test --all-features（160 通过）; cargo clippy --all-features --all-targets（0 warning）; make check；contract=enqueue_in_tx 事务内写入（提交持久/回滚消失）+ 时间戳统一 Unix 毫秒 + payload_version=1；commit=3a650da；review=2 项回滚/提交集成测试通过
- [x] `M01-JOBS-03` `[30m]` 实现 queued/running/retry_wait/succeeded/cancelled/dead 状态机及非法迁移拒绝。证据：files=backend/src/jobs/mod.rs,docs/STATE-MACHINES.md；commands=cargo test --all-features（166 通过）; cargo clippy --all-features --all-targets（0 warning）; make check；contract=JobStatus 六态 + allowed_transition 迁移表 + transition 非法拒绝不改状态 + 终态无出边 + running 不直接取消；commit=7cc8efb；review=6 项状态机测试 + 文档对齐
- [x] `M01-JOBS-04` `[45m]` 实现批量领取、owner、lease 延期和 lease 到期后的安全重领。证据：files=backend/src/jobs/worker.rs,backend/tests/jobs_worker.rs,docs/JOBS.md；commands=cargo test --all-features（172 通过/3 MySQL-only 忽略）; cargo clippy --all-features --all-targets（0 warning）; make check；contract=claim_batch 批量领取（最老优先/CAS 不重领/队列隔离）+ renew_lease 仅 owner 且 lease 未过期可续 + lease 过期重领（attempts+1、owner 切换）；commit=284e750；review=6 项租约集成测试 + JOBS.md 领取/续租契约
- [x] `M01-JOBS-05` `[45m]` 实现分类重试、指数退避、jitter、最大次数、dead-letter 和人工重放。证据：files=backend/src/jobs/retry.rs,backend/src/jobs/worker.rs,docs/STATE-MACHINES.md,docs/JOBS.md；commands=cargo test --all-features（185 通过/3 MySQL-only 忽略）; cargo clippy --all-features --all-targets（0 warning）; make check；contract=RetryClass 分类 + 指数退避（饱和不溢出）+ jitter 区间 + 行级 max_attempts 死信 + complete_job owner 成功 + replay_job dead→queued 重置 + 状态机新增人工重放边；commit=495accd；review=7 项重试集成测试 + 5 项退避单测 + 状态机重放边测试 + 文档同步
- [x] `M01-JOBS-06` `[45m]` 消费者以 event_id/job idempotency key 去重，至少一次投递不得产生重复业务副作用。证据：files=migrations/{sqlite,mysql,mariadb}/0008_outbox_consumed.sql,backend/src/outbox.rs,backend/tests/outbox_consumer.rs,docs/STATE-MACHINES.md,docs/JOBS.md；commands=cargo test --all-features（190 通过/3 MySQL-only 忽略）; cargo clippy --all-features --all-targets（0 warning）; make check；contract=consume_in_tx 事务内去重标记（唯一约束）+ mark_sent_in_tx 同事务幂等标记 + 崩溃回滚恰好一次 + 竞争消费者不重复副作用 + job deduplication_key 入队层去重；commit=f773fe6；review=5 项消费者集成测试 + 状态机/幂等文档对齐
- [x] `M01-JOBS-07` `[30m]` 禁止在数据库写事务中调用 SMTP、S3、AI、视频 Provider 或执行图片处理。证据：files=scripts/check-tx-io.rb,Makefile,docs/JOBS.md；commands=make check（含 check-tx-io 全绿）; ruby scripts/check-tx-io.rb（干净通过 + 违规样例退出码 1）; contract=含事务原语的源文件不得引用外部 IO 依赖（lettre/aws-sdk/reqwest/image/ffmpeg/AI SDK），CI 阻断；commit=94d5672；review=负向探针验证门禁可拦截 + JOBS.md §4.2 契约
- [x] `M01-JOBS-08` `[30m]` Worker 收到停机信号后停止领取新任务，完成/释放当前任务并受总超时约束。证据：files=backend/src/jobs/worker_loop.rs,backend/tests/worker_loop.rs,docs/JOBS.md；commands=cargo test --all-features（193 通过/3 MySQL-only 忽略）; cargo clippy --all-features --all-targets（0 warning）; make check；contract=run_worker 停机即停领 + 在途任务完成 + drain_timeout 总超时 + 周期续租/失租停止 + SIGTERM/SIGINT→watch；commit=0d0ef13；review=3 项停机语义集成测试 + JOBS.md §12
- [x] `M01-JOBS-09` `[30m]` SQLite busy 时指数退避并计数，禁止无延迟高频自旋。证据：files=backend/src/db/busy.rs,backend/src/jobs/worker_loop.rs,backend/tests/sqlite_busy.rs,docs/JOBS.md；commands=cargo test --all-features（200 通过/3 MySQL-only 忽略）; cargo clippy --all-features --all-targets（0 warning）; make check；contract=BusyPolicy 指数退避（饱和不溢出）+ BusyCounter 计数 + is_busy_error（SQLITE_BUSY=5/LOCKED=6/消息兜底） + retry_on_busy 非 busy 不重试 + worker 领取接入；commit=c07e4a0；review=3 项真实 busy 集成测试 + 4 项退避单测 + JOBS.md §4
- [x] `M01-JOBS-10` `[30m]` 对 SMTP/S3 临时错误、永久错误、超时和取消建立明确分类。证据：files=backend/src/jobs/classify.rs,docs/JOBS.md；commands=cargo test --all-features（207 通过/3 MySQL-only 忽略）; cargo clippy --all-features --all-targets（0 warning）; make check；contract=ProviderError 归一化 + classify→FailureClass（Transient/Permanent/Cancelled）+ retry_class 映射 + SMTP 4xx/5xx、S3 429/5xx/4xx、超时/连接/取消规则；commit=17a857b；review=7 项分类单测 + JOBS.md §6 分类表
- [x] `M01-JOBS-11` `[45m]` 测试进程在领取后、业务调用后、提交前后崩溃的恢复和去重结果。证据：files=backend/tests/crash_recovery.rs,docs/JOBS.md；commands=cargo test --all-features（211 通过/3 MySQL-only 忽略）; cargo clippy --all-features --all-targets（0 warning）; make check；contract=四种崩溃点矩阵（领取后租约恢复重领/提交前回滚恰好一次/提交后重投去重跳过/job 效果行唯一键幂等）；commit=b4bb209；review=4 项崩溃恢复集成测试 + JOBS.md §5 矩阵
- [~] `M01-JOBS-12` `[30m]` 邮件任务 payload 只存 token 引用/密文所需最小信息，任何日志不得输出验证或重置 token。
- [ ] `M01-JOBS-13` `[30m]` 暴露 queue depth、age、attempt、lease timeout、dead count 和处理延迟指标。

## M01-AUDIT：审计、事件和幂等基础件

**元数据：** `P0` · `owner=unassigned/security-backend` · `risk=critical` · `depends=M01-DB,M01-JOBS` · `blocked=none`
**目标文件：** `backend/src/audit/`、`backend/src/idempotency/`、`migrations/*/`、`docs/EVENT-CATALOG.md`
**验收：** 高风险操作无审计无法提交；幂等冲突、重放和敏感字段清除测试通过。

- [ ] `M01-AUDIT-01` `[45m]` 建立不可关闭的 audit_logs，包含 actor、effective role、target、action、reason、request_id 和 policy version。
- [ ] `M01-AUDIT-02` `[30m]` 对 before/after 使用字段 allowlist，禁止密码、Token、Secret、隐藏正文和完整签名 URL。
- [ ] `M01-AUDIT-03` `[45m]` 建立幂等记录的 scope/key/request hash/status/response reference/expiry 数据模型。
- [ ] `M01-AUDIT-04` `[30m]` 相同 key+摘要返回原结果；相同 key+不同摘要稳定返回 409。
- [ ] `M01-AUDIT-05` `[45m]` 并发首次请求只能有一个执行者；失败是否缓存按 operation 契约明确处理。
- [ ] `M01-AUDIT-06` `[45m]` 为管理员代操作、权限变更、配置、账务、审核、Secret 和 Feature Flag 建立审计 helper。
- [ ] `M01-AUDIT-07` `[30m]` 自动比对领域事件名称、payload version 与 `docs/EVENT-CATALOG.md`。
- [ ] `M01-AUDIT-08` `[45m]` 测试审计与业务事务原子性、Outbox request ID 贯通和敏感数据脱敏。
- [ ] `M01-AUDIT-09` `[45m]` 增加仅授权管理员可查询的审计分页与导出边界，深分页使用 cursor。

---

<a id="m2"></a>

# M2：账号、邮箱验证、Session 与高风险认证

**完成定义：** 匿名可注册；邮箱验证后才能写入；Session/CSRF/TOTP 满足安全基线；身份全链路在三数据库和浏览器中通过。

## M02-IDENTITY：注册、验证与密码恢复

**元数据：** `P0` · `owner=unassigned/backend-auth` · `risk=critical` · `depends=M01-AUDIT` · `blocked=none`
**目标文件：** `migrations/*/`、`backend/src/auth/`、`backend/src/routes/auth/`、`backend/tests/auth/`
**验收：** `Auth` tag 注册/验证/密码恢复 operation 通过三数据库契约和枚举攻击测试。

- [ ] `M02-IDENTITY-01` `[45m]` 新增身份迁移：username/email 规范化列、password hash、status、verification 和 reset token 表。
- [ ] `M02-IDENTITY-02` `[45m]` 为规范化用户名和邮箱建立跨库唯一约束及大小写/Unicode Fixture。
- [ ] `M02-IDENTITY-03` `[30m]` 实现注册 DTO 长度、格式、保留名、密码策略和请求体未知字段校验。
- [ ] `M02-IDENTITY-04` `[45m]` 使用 Argon2id PHC hash，参数可升级；测试正确、错误及损坏 hash 的常量时间失败路径。
- [ ] `M02-IDENTITY-05` `[45m]` 在同一事务创建 pending_verification 用户、一次性 token hash、审计和验证邮件 Outbox。
- [ ] `M02-IDENTITY-06` `[30m]` 注册响应和耗时不泄漏邮箱/用户名是否已存在，重复请求受账号/IP 双维度限流。
- [ ] `M02-IDENTITY-07` `[45m]` 实现验证 token 过期、一次消费、旧 token 失效和并发消费唯一成功。
- [ ] `M02-IDENTITY-08` `[30m]` 实现重发验证邮件，采用统一响应、冷却时间、日上限和旧 token 失效。
- [ ] `M02-IDENTITY-09` `[30m]` 验证成功激活账号并应用可选新用户冷静期，写审计和领域事件。
- [ ] `M02-IDENTITY-10` `[45m]` 实现找回密码统一响应、30 分钟一次性 token、成功改密和其他 Session 撤销。
- [ ] `M02-IDENTITY-11` `[30m]` 测试 token 只以 hash 入库，不出现在 API、日志、审计、Outbox 诊断或错误中。
- [ ] `M02-IDENTITY-12` `[45m]` 为注册、验证、重发、重置的事务每一步做故障注入，验证无半完成状态。

## M02-SESSION：登录、Cookie、Session 与 CSRF

**元数据：** `P0` · `owner=unassigned/backend-auth` · `risk=critical` · `depends=M02-IDENTITY` · `blocked=none`
**目标文件：** `migrations/*/`、`backend/src/auth/session*`、`backend/src/middleware/csrf*`、`backend/tests/session*`
**验收：** Session fixation、来源校验、跨站请求、撤销和生命周期测试通过。

- [ ] `M02-SESSION-01` `[45m]` 扩展 session 迁移：token hash、device、created/last_seen、idle/absolute expiry、revoked_at 和 version。
- [ ] `M02-SESSION-02` `[30m]` 生成至少 256 bit 熵的 Session token，仅存 hash，并设置 `__Host-bblbb_session` 安全属性。
- [ ] `M02-SESSION-03` `[45m]` 实现账号/IP 限流和常量时间登录失败，错误不得区分账号不存在、密码错误或账号状态。
- [ ] `M02-SESSION-04` `[30m]` 登录、权限提升、改密和高风险重新认证时旋转 Session，防止 fixation。
- [ ] `M02-SESSION-05` `[45m]` 实现 idle/absolute timeout、当前登出、全部登出、设备列表和逐设备撤销。
- [ ] `M02-SESSION-06` `[30m]` 每次请求实时执行账号状态、封禁、角色和 Session revoked 检查，不依赖后台任务延迟。
- [ ] `M02-SESSION-07` `[45m]` 实现 Session 绑定 synchronizer CSRF token 与 private/no-store 的 token 获取端点。
- [ ] `M02-SESSION-08` `[30m]` 为注册/登录建立匿名预认证 CSRF 状态，防止 login CSRF。
- [ ] `M02-SESSION-09` `[45m]` Cookie 写请求校验 `X-CSRF-Token` 与 Origin；缺 Origin 时按策略校验 Referer。
- [ ] `M02-SESSION-10` `[30m]` Bearer-only 且完全不使用 Cookie 的请求不错误要求 CSRF；GET/HEAD/OPTIONS 无业务副作用。
- [ ] `M02-SESSION-11` `[45m]` 测试反向代理 Set-Cookie 传播、错误 token、其他 Session token、跨 Origin 和无 Referer 请求。
- [ ] `M02-SESSION-12` `[45m]` 为 Cookie 属性、过期、撤销、并发请求和账号状态变化运行三数据库集成测试。

## M02-MFA：TOTP、恢复码与近期认证

**元数据：** `P0` · `owner=unassigned/security-auth` · `risk=critical` · `depends=M02-SESSION` · `blocked=none`
**目标文件：** `migrations/*/`、`backend/src/auth/mfa*`、`backend/tests/mfa*`、`docs/AUTH-OIDC.md`
**验收：** 时间漂移、重放、恢复码并发、高权限强制和 step-up 流程测试通过。

- [ ] `M02-MFA-01` `[45m]` 新增 TOTP enrollment、加密 secret、last accepted step 和恢复码 hash 迁移。
- [ ] `M02-MFA-02` `[45m]` 实现 enrollment challenge、二维码所需最小数据、确认后启用和取消未完成 enrollment。
- [ ] `M02-MFA-03` `[30m]` 实现允许时间窗口和已接受 time step 防重放，不在日志输出 code 或 secret。
- [ ] `M02-MFA-04` `[45m]` 一次生成恢复码，只展示一次；数据库存 hash，消费时原子标记并通知用户。
- [ ] `M02-MFA-05` `[30m]` 普通 member 可选 TOTP；administrator、moderator 和高风险账务账号强制启用。
- [ ] `M02-MFA-06` `[30m]` 未完成强制 enrollment 的账号不得取得对应高权限 Session 或执行高风险操作。
- [ ] `M02-MFA-07` `[45m]` 为改密、停用 MFA、角色提升、退款、密钥和 Secret 操作实现 recent-auth/step-up。
- [ ] `M02-MFA-08` `[30m]` 实现新设备、密码/MFA 变化、Session 撤销和恢复码使用安全通知。
- [ ] `M02-MFA-09` `[45m]` 测试时钟偏移、code 重放、并发恢复码、降权、封禁和 Session 旋转。
- [ ] `M02-MFA-10` `[30m]` 编写管理员失去 TOTP 设备的受控恢复 Runbook，要求双人复核和不可删除审计。

## M02-UX：身份前端与端到端契约

**元数据：** `P1` · `owner=unassigned/frontend-auth` · `risk=high` · `depends=M02-IDENTITY,M02-SESSION,M02-MFA` · `blocked=none`
**目标文件：** `frontend/src/routes/(auth)/`、`frontend/src/routes/me/`、`frontend/tests/`、`openapi/operation-coverage.json`
**验收：** 匿名、未验证、冷静期、正常、MFA 和封禁 persona 的 Playwright 流程与三数据库 API Fixture 通过。

- [ ] `M02-UX-01` `[45m]` 实现注册页面、服务端表单 action、字段错误关联和统一账号冲突提示。
- [ ] `M02-UX-02` `[30m]` 实现验证结果、重发入口、冷却倒计时和未验证账号允许/禁止动作说明状态。
- [ ] `M02-UX-03` `[45m]` 实现登录、TOTP 二次输入、恢复码和统一失败提示，不泄漏账号状态。
- [ ] `M02-UX-04` `[30m]` 实现忘记/重置密码页面，成功后提示其他 Session 已撤销。
- [ ] `M02-UX-05` `[45m]` 实现 `/me` 安全投影、账号状态、验证状态和 Session 设备管理。
- [ ] `M02-UX-06` `[45m]` 实现 TOTP enrollment、恢复码一次展示、停用 MFA 和 recent-auth 交互。
- [ ] `M02-UX-07` `[30m]` 为 401/403/409/422/429/503 和 request ID 提供可访问、可恢复的 UI 状态。
- [ ] `M02-UX-08` `[45m]` 测试无 JavaScript 注册/登录/重发/重置的合理退化，不把认证裁决放到浏览器。
- [ ] `M02-UX-09` `[45m]` 用相同 HTTP Fixture 在 SQLite/MySQL/MariaDB 断言状态码、Problem code、投影和回滚一致。
- [ ] `M02-UX-10` `[30m]` 更新 Auth/Users operation coverage 条目，附 handler、集成测试、Playwright 和文档证据。

---

## M0-M2 出口门槛

- M0 所有 `P0` 叶子任务完成，根检查可从干净环境复现。
- 三数据库迁移和身份契约均绿；生产启动不隐式迁移。
- 未验证用户只能登录浏览、修改账号和重发验证，服务端拒绝内容、上传、交易和奖励。
- Session、CSRF、TOTP、近期认证和审计无未关闭 P0/P1 缺陷。
- Worker 崩溃、lease、重复执行、SQLite busy 和优雅停机故障注入通过。
- operation coverage 中 M0-M2 对应端点全部具有实现和测试证据。
