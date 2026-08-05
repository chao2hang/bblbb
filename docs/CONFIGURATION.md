# BBLBB — 配置字段与运行时变更矩阵

> 基线：v0.4。环境变量/Secret Store 是部署事实来源；后台可编辑项必须映射到版本化数据库 policy。配置值不能改变代码安全上限。

| 配置域 | 关键字段 | Secret | 运行时变更 | 生效范围 |
|---|---|---:|---|---|
| Server | `PUBLIC_ORIGIN`、bind、trusted proxies | 否 | 重启 | 新请求/Session |
| Database | URL、pool、busy timeout | 可能 | 重启 | 新连接 |
| Session/OIDC | cookie、issuer、key ref、TTL | 是 | 密钥轮换流程 | 新 Token；旧 Token按撤销策略 |
| Storage | backend、endpoint、bucket、region、upload limit、signed URL TTL | 是 | 候选配置 + 测试 + 审批 | 新上传/新 URL |
| Attachment quota | level max file/total bytes | 否 | 是，需 version | 新上传；已有对象不删除 |
| Download billing | mode、prices、free rules、limits、TTL | 否 | 是，需 reason/version | 新授权；历史授权不变 |
| Marketplace | Client、scope、limit、fee、settlement delay、webhook | Secret | 审批/轮换流程 | 新 Token/Offer/Intent |
| AI | Provider URL、models、data mode、budget、timeout | Secret | 是，策略递增 version | 新任务；历史 suggestion 保留来源版本 |
| Video | Provider enable、hosts、CSP、HLS budgets、duration | 否 | 是，policy version | 新 resolve/render；历史引用重检 |
| Rate limit | user/IP/object/provider buckets | 否 | 是 | 新请求 |
| Mail | SMTP host、sender、templates | 是 | Secret 轮换/模板发布 | 新任务 |
| Feature flags | capability 默认开关、灰度规则 | 否 | 是，需审计 | 新请求/用户投影 |

## 1. 统一规则

- 配置读取返回 `configured`、来源类别、版本和更新时间，不返回 Secret、完整签名 URL、内部路径或 Provider 原始响应。
- 在线修改必须先校验 schema、范围、依赖和安全上限，再写 `config_revisions`/policy 版本并审计；高风险修改需要近期认证。
- 外部 Provider 未配置或不可用时，不得阻塞核心发帖、阅读、登录和已提交账务。
- 关闭功能停止新任务/新授权/新交易；不删除历史数据、不撤销已经提交账务、不让已发布内容突然泄漏。
- Feature Flag 只负责启停和灰度，不得绕过权限、CSRF、审计、账本或安全策略。

## 1.1 当前已实现配置登记表（M01-CONFIG-01）

以下登记表是环境变量 ↔ 类型化字段的机械映射（`backend/src/config.rs` 的
`CONFIG_REGISTRY`），由测试强制不变量：变量后缀小写 = 字段名；登记项与
`backend/.env.example` 双向同步。

| 环境变量 | 类型化字段 | 默认值 | 环境适用范围 | 运行时变更 |
|---|---|---|---|---|
| `BBLBB__BIND_ADDRESS` | `bind_address` | `127.0.0.1:8080` | all | 重启 |
| `BBLBB__LOG_FILTER` | `log_filter` | `bblbb_backend=info,tower_http=info` | all | 重启 |
| `BBLBB__OPENAPI_PATH` | `openapi_path` | `../openapi/openapi.yaml` | all | 重启 |
| `BBLBB__DATABASE_URL` | `database_url` | `sqlite://../data/bblbb.sqlite` | all | 重启 |
| `BBLBB__MIGRATIONS_DIR` | `migrations_dir` | `../migrations/sqlite` | all | 重启 |
| `BBLBB__STORAGE_DIR` | `storage_dir` | `../uploads` | all | 重启 |
| `BBLBB__AUTO_MIGRATE` | `auto_migrate` | `false` | dev, ci | 重启 |
| `BBLBB__ALLOWED_HOSTS` | `allowed_hosts` | 空 = 宽松模式（仅记录） | all | 重启 |
| `BBLBB__ALLOWED_ORIGINS` | `allowed_origins` | 空 = 宽松模式（仅记录） | all | 重启 |
| `BBLBB__DB_MAX_CONNECTIONS` | `db_max_connections` | `8` | all | 重启 |
| `BBLBB__DB_MIN_CONNECTIONS` | `db_min_connections` | `1` | all | 重启 |
| `BBLBB__DB_CONNECT_TIMEOUT_MS` | `db_connect_timeout_ms` | `10000` | all | 重启 |
| `BBLBB__DB_IDLE_TIMEOUT_MS` | `db_idle_timeout_ms` | `300000` | all | 重启 |
| `BBLBB__DB_SLOW_QUERY_MS` | `db_slow_query_ms` | `500` | all | 重启 |
| `BBLBB__ENV` | `env` | `development` | all | 重启 |
| `BBLBB__SECRETS_DIR` | `secrets_dir` | 空 = 未启用 | all | 重启 |
| `BBLBB__SECRETS_SYSTEMD_UNIT` | `secrets_systemd_unit` | 空 = 未启用 | all | 重启 |
| `BBLBB__FEATURE_KILL_SWITCH` | `feature_kill_switch` | `false` | all | 重启 |
| `BBLBB__NEW_USER_COOLDOWN_SECS` | `new_user_cooldown_secs` | `0` = 关闭 | all | 重启 |
| `BBLBB__TOTP_WINDOW_STEPS` | `totp_window_steps` | `1` | all | 重启 |

说明：

- **环境适用范围**：`all` = 开发/CI/生产通用；`dev,ci` = 仅限非生产（生产
  迁移由显式 `bblbb-migrate apply` 执行，见 M01-DB-06）。
- **运行时变更**：当前所有环境变量配置均为**重启生效**；在线热更新和 Secret
  轮换由后续 `config_revisions`/policy 版本机制提供（M01-CONFIG-03/04/05）。
- 新增环境变量必须同步登记表、`.env.example` 与本文档；登记表测试会拦截
  未登记或未同步的变量。

## 1.3 Secret Provider（M01-CONFIG-03）

`backend/src/config/secrets.rs` 定义统一 `SecretProvider` trait 与内置实现：

| Provider | 来源 | 说明 |
|---|---|---|
| `FileSecretProvider` | `BBLBB__SECRETS_DIR` | 一个 Secret 一个文件，文件名 = 名称；生产强制 owner-only 权限（0600/0400），目录建议 0700 |
| `SystemdCredentialProvider` | `BBLBB__SECRETS_SYSTEMD_UNIT` | 读取 `/run/credentials/<unit>/<name>`（`LoadCredential=`/`SetCredential=`） |
| `EnvProvider` | 环境变量 | 兜底/测试用；生产不建议作为 Secret 主来源 |
| `ChainProvider` | 多来源 | 按注册顺序尝试，第一个命中即返回；文件优先于 systemd |

约定：

- provider 只读不写；写接口由 M01-CONFIG-04 提供并只返回元数据
  （configured / source class / version / updated_at），不返回值。
- 托管 Secret（Vault / 云 Secret Manager）扩展点：实现 `SecretProvider`
  注册进 `ChainProvider` 即可，调用方不感知来源。
- `SecretValue` 的 `Debug` 不输出内容，防止误入日志。
- `AppConfig::secret_provider()` 按配置构建链；生产模式自动开启文件权限校验。

## 1.4 Secret 写接口（M01-CONFIG-04）

`SecretWriter` trait 是**只写不读**的：

- `set(name, value)` 写入/轮换后只返回 `SecretMetadata`
  （configured / source class / version / updated_at），绝不回读值；
- `configured_names()` 只返回已配置名称列表；
- trait 上没有任何返回 Secret 值的方法——从类型层面杜绝"写后又读回"。
- 元数据查询：`SecretProvider::metadata(name)` 为 stat-only（只读文件系统
  元信息，不读取内容），GET 接口只返回元数据。
- `FileSecretWriter`：原子写（临时文件 + 落盘 + rename），Unix 上强制
  `0600`；拒绝路径穿越/非法名称（`/`、`\`、`..`、空格、空名）。

## 1.5 Feature Flag（M01-CONFIG-05）

`backend/src/config/flags.rs` 实现可选能力开关系统：

| 能力 | Flag | v1.0 默认 |
|---|---|---|
| AI 建议/逐次同意 | `ai` | 关闭 |
| Video Provider（Direct/HLS/Xigua） | `video` | 关闭 |
| 下载抵扣/计费 | `download_billing` | 关闭 |
| OIDC Provider | `oidc` | 关闭 |
| 第三方 Marketplace | `marketplace` | 关闭 |

机制：

- **默认值**：五个可选能力默认全部关闭（M01-CONFIG-06）；核心论坛不依赖
  任何 Flag。
- **作用范围**：v1.0 为全局启停；灰度规则由后续 policy 版本扩展
  （`FlagScope::Global`）。
- **生效时间**：每个 Flag 带 `effective_at`（Unix 毫秒），到达前不生效。
- **紧急关闭**：`BBLBB__FEATURE_KILL_SWITCH=true` 或运行时
  `emergency_off()` 置真后所有可选能力强制关闭，优先于一切。
- **版本**：每次变更版本 +1，采用乐观锁（`set` 需传期望版本，冲突即拒绝），
  与 `config_revisions` 版本化策略一致。
- **审计**：每次变更与紧急关闭逐 flag 写审计（actor / reason / 前后状态 /
  版本 / 时间）；持久化审计由 M01-AUDIT 接入 `audit_logs`。

## 1.6 版本化配置存储与测试（M01-CONFIG-08）

`backend/src/config/store.rs` 实现 `ConfigStore`（模拟 `config_revisions`）：

- `read` 读取生效配置；`update` 管理更新走乐观锁
  （`expected_version` 不一致返回 `VersionConflict`）；
- 变更先进入暂存区，`apply_restart`（重启）后才生效——与登记表
  "运行时变更 = 重启" 语义一致；
- Secret 轮换复用 `FileSecretWriter`：再次写入即轮换，值更新、mtime/版本
  变化、旧值不可再读，元数据仍只写不读。

覆盖测试：

- 配置读取（缺失 → None，种子后读到值）；
- 管理更新（暂存 → 重启生效，版本 +1，actor/时间记录）；
- 并发版本冲突（同一期望版本两个更新，第二个冲突；用新版本重试成功）；
- 重启生效（仅暂存变更应用，未变更键保持）；
- Secret 轮换（新值可读、旧值不可读、版本/mtime 更新、元数据不含值）。

## 1.2 生产模式校验（M01-CONFIG-02）

`BBLBB__ENV=production` 时启动强校验，任一失败立即退出：

1. **未知键**：拒绝未登记的环境变量/配置键（与 `CONFIG_REGISTRY` 比对，
   大小写与 `BBLBB__` 前缀归一化后判断）。
2. **占位 Secret**：数据库 DSN 含占位密码或示例主机
   （`changeme`/`your_password`/`example.com` 等）即拒绝。
3. **不安全 Origin**：`BBLBB__ALLOWED_ORIGINS` 必须为 `https://`
   （`http://localhost`/loopback 明文允许）。
4. **非 loopback 内部端口**：`BBLBB__BIND_ADDRESS` 必须为 loopback
   （`127.0.0.1`/`::1`），禁止 `0.0.0.0` 对外监听。
5. **冲突配置**：`BBLBB__AUTO_MIGRATE=true` 与生产迁移策略冲突，拒绝
   （迁移必须显式 `bblbb-migrate apply`，见 M01-DB-06）。
6. **非法 env 值**：`env` 只接受 `development` / `test` / `production`。
