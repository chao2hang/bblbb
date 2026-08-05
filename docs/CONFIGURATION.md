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

说明：

- **环境适用范围**：`all` = 开发/CI/生产通用；`dev,ci` = 仅限非生产（生产
  迁移由显式 `bblbb-migrate apply` 执行，见 M01-DB-06）。
- **运行时变更**：当前所有环境变量配置均为**重启生效**；在线热更新和 Secret
  轮换由后续 `config_revisions`/policy 版本机制提供（M01-CONFIG-03/04/05）。
- 新增环境变量必须同步登记表、`.env.example` 与本文档；登记表测试会拦截
  未登记或未同步的变量。

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
