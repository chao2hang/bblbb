# BBLBB Release Bundle 布局与构建（M15-PACKAGE-01/02/05）

> 版本：v1.0.0-rc.2 基准
> 生产机只部署构建产物，不执行 `npm install`/`npm run build`/`cargo build`。

## 1. Release Bundle 产物布局

干净构建机（CI/发布机）产出**不可变** bundle（tarball + checksum），上传到
发布目录 `/opt/bblbb/releases/<version>/` 并切换 `current` 符号链接。

```text
<version>.tar.gz                        # 完整 bundle（下述目录的打包）
├── backend/
│   └── bblbb-backend                   # cargo build --release 二进制（静态运行）
│   └── bblbb-migrate                   # 迁移工具二进制（同仓库 bin/migrate.rs）
├── frontend/
│   ├── build/                          # npm run build（SvelteKit adapter-node）
│   ├── package.json
│   ├── package-lock.json               # 依赖锁（SBOM 输入）
│   └── node_modules/                   # npm ci --omit=dev（adapter-node runtime dependencies）
├── openapi/openapi.yaml                 # backend runtime contract endpoint
├── migrations/
│   ├── sqlite/*.sql                    # 三方言迁移（不可变，checksum 受保护）
│   ├── mysql/*.sql
│   └── mariadb/*.sql
├── ops/
│   ├── backup/{daily,sqlite,manifest}.sh # systemd 每日 SQLite 备份运行时
│   └── restore/{sqlite,verify,verify-attachments}.sh # SQLite 恢复与只读校验工具
├── METADATA.json                       # 构建 commit/版本/依赖锁/SBOM/checksums
├── SBOM.json                           # 依赖清单（cargo tree / npm ls）
├── SHA256SUMS                          # 全部产物校验和
└── VERSION                             # 一行版本号（如 1.0.0-rc.2+build.20260807.1）
```

Release 必须在 bundle 之外取得并核验可信 SHA-256（例如由受保护 CI artifact/发布签名通道独立提供），并通过 `BBLBB_RELEASE_EXPECTED_SHA256` 传入 `release.sh`。bundle 内 `SHA256SUMS` 仅用于完整性检查，不能认证发布者。

### 生产机布局（M15-PACKAGE-05）

```text
/opt/bblbb/
├── releases/
│   ├── 1.0.0-rc.2+build.1/             # 只读，root:bblbb 0444/0555
│   └── ...                             # 保留最近 N 个版本（默认 3）
├── current -> releases/1.0.0-rc.2+build.1   # 部署符号链接（root 维护）
├── backup/                             # 发布前备份暂存（root:root 0700）
└── lost+found-notes/                   # 故障诊断保留目录（root:root 0700）
```

运行态数据在服务用户目录，与 release 目录分离：

```text
/var/lib/bblbb/
├── database/        # SQLite 主库（bblbb:bblbb 0700）
├── uploads/         # 附件/本地存储（bblbb:bblbb 0700）
├── themes/          # 主题数据导出（bblbb:bblbb 0700）
└── backups/         # 备份产物（root:bblbb 0640，应用账号不可删）
/var/lib/bblbb-frontend/ # 前端 StateDirectory（bblbb-frontend:bblbb-frontend 0700）
/etc/bblbb/
├── backend.env      # 非 Secret 配置（root:bblbb 0640）
└── frontend.env     # 前端运行时配置（root:bblbb-frontend 0640，不含 Secret）
```

### 最小文件权限（M15-PACKAGE-05）

| 路径 | 属主 | 权限 | 说明 |
|---|---|---|---|
| `/opt/bblbb/releases/<v>/backend/bblbb-backend` | `root:bblbb` | `0555` | 服务用户可执行、不可写 |
| `/opt/bblbb/releases/<v>/**` | `root:bblbb` | `0444`/`0555` | release 目录**服务用户不可写** |
| `/opt/bblbb/current` | `root:root` | `0777`→symlink | 由发布脚本/root 切换 |
| `/var/lib/bblbb/**` | `bblbb:bblbb` | `0700` | 仅 backend/worker 数据，不含前端目录 |
| `/var/lib/bblbb-frontend/` | `bblbb-frontend:bblbb-frontend` | `0700` | systemd `StateDirectory=bblbb-frontend` 管理；与 backend 数据隔离 |
| `/etc/bblbb/backend.env` | `root:bblbb` | `0640` | 仅 backend/worker/bblbb-admin 可读；非 Secret |
| `/etc/bblbb/secrets/` | `root:root` | dir `0700`, files `0600` | 仅 systemd manager 可读并通过 credentials 注入服务；运行发行版需支持 `LoadCredential=` 与可选来源语法 |
| `/etc/bblbb/frontend.env` | `root:bblbb-frontend` | `0640` | 仅非敏感前端运行时参数 |
| `/etc/bblbb/backup.env` | `root:root` | `0600` | 必需的 `BBLBB_BACKUP_RETENTION_POLICY=<external-policy-id>`；不得含 Secret。执行前必须实配外部完整套件 retention job 与磁盘告警 |
| `/etc/bblbb/secrets/**` | `root:root` | `0700` dir / `0600` file | 仅 systemd 管理器读取并注入 backend/worker credentials；不可通过共享 bblbb 组读取 |
| `bblbb-frontend` systemd user/group | `bblbb-frontend` | 独立账户 | 不属于 `bblbb` 组；仅读取 frontend.env，不可访问 backend.env 或 secrets |
| `/var/log/bblbb/` | `bblbb:bblbb` | `0755` | journald 通常接管 |

不变量：服务用户（`bblbb`）对 `/opt/bblbb/releases/**` 与
`/opt/bblbb/current` 只有 `r`/`x`，无 `w`；发布、回滚、备份删除全部由
root/发布账号执行。该不变量由 `deploy/tests/test-release-bundle.sh` 强制。

## 2. 版本固定

- Rust：`rust-toolchain.toml`（仓库根）固定 channel/component。
- 前端依赖：`frontend/package-lock.json`（`npm ci` 消费，禁止 `npm install`）。
- 后端依赖：`backend/Cargo.lock`（cargo 默认锁定；发布构建必须用锁）。
- 迁移：与源码同 commit 打包，发布后不可修改（checksum 保护）。

## 3. 构建与校验命令

```sh
deploy/scripts/build-release-bundle.sh --version 1.0.0-rc.2 --out-dir dist
deploy/scripts/record-release-metadata.sh --bundle dist/1.0.0-rc.2.tar.gz
deploy/tests/test-release-bundle.sh --bundle dist/1.0.0-rc.2.tar.gz
```

`test-release-bundle.sh` 断言：三方言迁移文件齐全、backend 二进制可执行、
frontend build 目录存在、`METADATA.json` 字段完整、`SHA256SUMS` 与实物一致、
release 目录最小权限（模拟 /opt/bblbb 布局）。
