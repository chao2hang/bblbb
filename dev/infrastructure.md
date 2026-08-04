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
