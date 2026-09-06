# AGENTS.md

本文件是所有 agent 会话（及人类开发者）在本仓库工作的强制约定。
其中「构建提速政策」为硬性规则：任何构建、检查、CI 相关改动不得违反，也不得以换语言等方式绕过。

## 1. 仓库速览

| 目录 | 技术栈 | 说明 |
|---|---|---|
| `backend/` | Rust（axum + sqlx + tokio，400+ 依赖，160+ test target） | cargo 共享 target 目录固定在 `/data/cargo-target/bblbb`（见 `backend/.cargo/config.toml`） |
| `frontend/` | SvelteKit + Vite + TypeScript | `npm ci` 全量安装较慢，见 §3.3 |
| `prototype/` | 独立 npm 原型 | 同上 |
| `openapi/` | OpenAPI 3 契约 | 契约治理脚本在 `scripts/*.rb` |
| `Makefile` | 统一入口 | `make help` 查看全部目标 |

## 2. 背景：为什么这些规则是硬的

后端一次全量编译需要数十分钟（重依赖：aws-sdk、sqlx 宏代码生成、大量 proc-macro）。
历史上"构建很久"的根因是流程配置，不是语言：

1. `make build` = `cargo build --release`，本地无 release 产物时每次都是全量冷构建；
2. `make check` 里 clippy 与 check 各跑一次全量编译（`--all-targets --all-features`）；
3. 前端 build/check 目标每次无条件 `npm ci`，删除并重装 `node_modules`；
4. CI 的 Rust 步骤没有 cargo 缓存 / sccache，PR 从零构建。

**结论：禁止以"改用 Java/Go/其他语言重写后端"来回应构建速度问题。**
构建速度问题只能在本政策框架内解决（增量构建、缓存、缩小编译面）。

## 3. 构建提速政策（强制）

### 3.1 开发循环（Rust）

- 本地迭代一律用 `make dev-backend`（即 `cargo run`，debug 增量构建）或 `cargo build`（debug）。
- **禁止在迭代中跑 `make build` / `cargo build --release`**；release 仅用于出包 / release-rc / nightly。
- **禁止 `make clean` / `cargo clean` / 删除 target 目录**——会触发全量重建。需要清理单个产物时用 `cargo clean -p <crate>`。
- 保持 `backend/.cargo/config.toml` 的 `target-dir = /data/cargo-target/bblbb` 不变（避免打爆根分区，debug 产物可达数十 GB）。
- 全量构建优先 `sccache cargo <cmd>`（当机器/CI 装有 sccache 时）。

### 3.2 检查（Rust）

- 本地 check 使用最小编译面：`cargo clippy --workspace` 与 `cargo check`（**不带** `--all-targets --all-features`）。
- `--all-targets --all-features` 全量编译**仅允许出现在 CI**（CI 有缓存，且 clippy 与 test 无法共享产物，允许各跑一次）。
- 修改 `Makefile` / `.github/workflows/` 时，不得在本地目标中恢复全量编译，不得删除 CI 缓存步骤。

### 3.3 前端 / 原型

- 依赖安装必须幂等且条件化：`[ -d node_modules ] || npm ci`（或 lockfile 变更时 `npm ci`）。
- **禁止在任何 build/check 目标中无条件 `npm ci`**（CI 干净环境除外）。
- 日常用 `npm run dev` / `npm run check`，不要反复 `npm run build` 验证小改动。

### 3.4 CI / 缓存

- `ci.yml` 的 Rust 步骤必须保留 cargo 缓存（sccache 缓存目录 + `~/.cargo/registry` + `~/.cargo/git`，或 target 目录缓存）。
- 新增/修改 workflow 时，涉及编译的步骤必须接缓存；删除缓存步骤需要先给出构建时间证据并经确认。
- 缓存失效（构建变慢）应优先排查缓存 key 是否覆盖 `Cargo.lock` 与 `package-lock.json`。

## 4. 待执行落地项（按序执行，完成后将 `[ ]` 改为 `[x]` 并在行尾注明 commit）

- [x] `Makefile`：`check-backend` 本地目标改为 `cargo clippy --workspace -- -D warnings` + `cargo check`（去掉 `--all-targets --all-features`）
  证据：files=Makefile；commands=make -n check-backend（确认输出无 --all-targets/--all-features）；commit=152e05e；review=none
- [x] `Makefile`：`build-frontend` / `check-frontend` / `check-prototype` / `install` 的 `npm ci` 改为 `[ -d node_modules ] || npm ci`
  证据：files=Makefile；commands=make -n build-frontend install（确认输出为条件化安装）；commit=152e05e；review=none
- [x] 本机与 CI 接入 sccache（CI 增加 sccache setup 与缓存步骤；cargo registry/git 缓存）
  证据：files=Makefile（SCCACHE_BIN/CARGO 变量，装 sccache 后自动生效）+.github/workflows/ci.yml（rust + mysql-family job）+nightly.yml（fault-injection）+release-rc.yml（release-drills）；commands=ruby YAML 校验 3 个 workflow 通过；commit=152e05e；review=none
- [ ] （可选，需评估）AWS SDK 依赖拆为独立 cargo feature，默认 check 不编译
  评估（2026-09-02）：暂不执行。S3 与核心 `StorageConfig`、`storage::adapter`（~350 行直接引用 aws_sdk_s3）及 5+ 个测试文件深度耦合；拆 feature 需 cfg 门控核心配置类型、改 release-rc 构建参数（`--features aws`）与 deploy bundle 脚本，改动面大且 sccache/缓存落地后 aws-sdk 增量编译本就不常触发。待缓存生效后若 CI 构建时间仍不达标再重估。

> 执行约束：以上属于构建流程改动，不新增业务代码，不需要从 `todo/` 工作包选叶子任务，但必须遵守 `TODO.md §2.4` 的完成证据格式（files / commands / commit）。

## 5. 禁止事项

- 用"换语言重写"（Java、Go 等）回应构建慢的问题。
- 本地迭代跑 release 构建。
- 无条件 `cargo clean` / `npm ci`。
- 删除或绕过 cargo / sccache / npm 缓存。
