# 基础设施开发约定

当前迁移是用于打通工具链的**骨架**，不是 `docs/SCHEMA.md` 的完整实现。它只建立认证启动所需的 `users` 与 `user_sessions` 最小结构；业务字段与其余表必须通过后续迁移增量加入。

## 迁移目录

- `migrations/sqlite/`：SQLite 3.40+ 专用 SQL。
- `migrations/mysql/`：MySQL 8.0+ 专用 SQL。
- `migrations/mariadb/`：MariaDB 10.11+ 专用 SQL。

文件名使用零填充的单调版本和说明，例如 `0002_add_user_profile.sql`。三个目录对同一逻辑版本使用相同版本号。已合并的迁移不可修改；修正必须新增迁移。迁移按文件名字典序在空库中执行，应用层迁移器接入后应记录版本、名称、SHA-256 checksum 和应用时间。

MySQL 与 MariaDB 当前骨架有意分目录，即使 SQL 相同也分别测试，后续不得假设两者方言和行为完全一致。

## 本地骨架验证入口

SQLite：

```sh
sqlite3 /tmp/bblbb.sqlite < migrations/sqlite/0001_skeleton.sql
sqlite3 /tmp/bblbb.sqlite 'PRAGMA foreign_key_check;'
```

MySQL 8（服务已经可用时）：

```sh
mysql --host=127.0.0.1 --user=root --password bblbb < migrations/mysql/0001_skeleton.sql
```

MariaDB 10.11（服务已经可用时）：

```sh
mariadb --host=127.0.0.1 --user=root --password bblbb < migrations/mariadb/0001_skeleton.sql
```

CI 会在对应数据库服务健康后执行各目录中的全部 `*.sql`。仓库出现根 `Cargo.toml` 或 `backend/Cargo.toml`、以及 `frontend/package.json` 后，CI 会自动执行相应 Rust/frontend 检查；当前静态原型仍执行它自身声明的检查脚本。

## 本地开发服务与网络监听规范

### 1. 端口与监听地址分配

| 服务 | 默认端口 | 生产要求 | 开发多机/局域网联调 | 说明 |
|---|---|---|---|---|
| **前端应用 (SvelteKit)** | `5173` | 由 Caddy 反向代理，内部独立 Node 进程 | `npm run dev -- --host 0.0.0.0 --port 5173` | 自动检测 `dev/certs` 启用 HTTPS，保证 `__Host-` Cookie 可存入 |
| **后端服务 (Rust)** | `8080` | 生产强制 loopback 监听 (`127.0.0.1`) | `BBLBB__BIND_ADDRESS=0.0.0.0:8080 cargo run` | REST API 与 OpenAPI 契约提供方 |
| **静态原型 (Prototype)** | `8765` | 不对外发布，仅供验收比对 | `PROTOTYPE_HOST=0.0.0.0 PROTOTYPE_PORT=8765 node serve.mjs` | 原型静态页面与测试验收服务 |

### 2. HTTPS 与 `__Host-` 会话安全规范

1. **Cookie 存储标准**：BBLBB 登录会话与 CSRF 使用 `__Host-session` 与 `__Host-bblbb_csrf`。根据 IETF Cookie Prefixes 标准，带有 `__Host-` 前缀的 Cookie 必须携带 `Secure` 标记。
2. **浏览器限制**：现代浏览器仅在访问 `http://localhost` 时豁免明文 HTTP 存储 Secure Cookie；通过 IP（如 `10.10.10.10:5173`）或远程自定义域名访问时，明文 HTTP 会**直接丢弃该 Cookie**，导致无法维持登录状态。
3. **自签证书生成**：
   ```bash
   mkdir -p dev/certs
   openssl req -x509 -newkey rsa:2048 -nodes \
     -keyout dev/certs/dev.key -out dev/certs/dev.crt -days 365 \
     -subj "/CN=bblbb.com" \
     -addext "subjectAltName=DNS:bblbb.com,DNS:*.bblbb.com,DNS:localhost,IP:127.0.0.1,IP:10.10.10.10,IP:172.21.0.1"
   ```
4. **Vite 自动集成**：`frontend/vite.config.ts` 会自动探测 `dev/certs/dev.crt` 与 `dev/certs/dev.key`，存在时无缝切换为 HTTPS 模式，无需额外传递环境变量。
