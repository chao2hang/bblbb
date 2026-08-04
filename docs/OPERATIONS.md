# BBLBB — 部署、升级与运维

> 版本：v0.4
> 本文面向单机生产部署。默认推荐 systemd + Caddy + SQLite；容器部署提供等价能力，但不作为唯一方式。

## 1. 生产进程

```text
caddy
bblbb-backend
bblbb-frontend
```

SQLite 模式不需要数据库进程。MySQL/MariaDB 可同机或使用托管服务，但同机建议至少 1GB 内存。

生产服务器只部署构建产物，不执行 `pnpm install/build`；前端在 CI 或构建机生成。

## 2. 目录建议

```text
/opt/bblbb/releases/<version>/
/opt/bblbb/current -> releases/<version>
/etc/bblbb/backend.env
/etc/bblbb/frontend.env
/var/lib/bblbb/database/
/var/lib/bblbb/uploads/
/var/lib/bblbb/themes/
/var/lib/bblbb/backups/
/var/log/bblbb/（若不全交给 journald）
```

- 服务用户不能写 release 目录。
- 只允许 backend 写数据库、上传和任务数据目录。
- 配置文件权限最小化，secret 不放入前端环境。

## 3. 核心配置

以下为 v0.4 冻结配置命名基线；类型、默认值、Secret 和运行时变更规则见 `CONFIGURATION.md`，实现不得静默改名：

```text
BBLBB_ENV=production
BBLBB_PUBLIC_ORIGIN=https://community.example.com
BBLBB_DATABASE_URL=sqlite:///var/lib/bblbb/database/bblbb.db
BBLBB_BIND=127.0.0.1:8080
BBLBB_FRONTEND_ORIGIN=http://127.0.0.1:3000
BBLBB_TRUSTED_PROXIES=127.0.0.1/32
BBLBB_STORAGE_BACKEND=local
BBLBB_STORAGE_LOCAL_PATH=/var/lib/bblbb/uploads
# S3 后端按需启用：
# BBLBB_S3_ENDPOINT=https://s3.amazonaws.com
# BBLBB_S3_REGION=ap-southeast-1
# BBLBB_S3_BUCKET=bblbb-attachments
# BBLBB_S3_ACCESS_KEY_ID=...
# BBLBB_S3_SECRET_ACCESS_KEY=...
# BBLBB_S3_PATH_STYLE=false
# BBLBB_S3_PRESIGNED_UPLOADS=true
# BBLBB_S3_SIGNED_URL_TTL_SECONDS=300
BBLBB_UPLOAD_MAX_BYTES=20971520
BBLBB_SECRET_KEY_FILE=/etc/bblbb/master-key
BBLBB_SMTP_*=...
```

- `PUBLIC_ORIGIN` 必须固定 HTTPS Origin，OIDC discovery 和 Cookie 依赖它。
- 启动校验 URL、目录权限、数据库版本、迁移状态和密钥。
- 使用 `s3` 时启动校验 Endpoint HTTPS、Region、Bucket、凭证来源、TTL 和大小范围，但不把第三方实时探测作为每次 readiness 的硬依赖。
- S3 Secret 只通过 systemd credentials、容器 Secret、Workload Identity 或权限受限环境文件提供；不得写入前端环境、命令行参数、日志和普通配置导出。
- 管理后台 `/admin/storage` 的“测试连接”调用受保护后端接口，检查 Bucket 可访问性、对象前缀权限与签名模式，只返回脱敏结果并写管理员审计。
- 未知配置键在生产模式报错或明确警告。
- 改变 origin、Cookie 名、OIDC issuer 或存储后端属于高风险迁移，不可随意运行时切换。

新领域配置、Secret、在线修改和生效范围的统一矩阵见 [`CONFIGURATION.md`](CONFIGURATION.md)。Marketplace、AI 和 Video 默认关闭或按 Provider 启用；外部服务不可用不能影响核心论坛健康检查。

### 新领域最低运行参数

- Marketplace：Intent TTL、单笔/日限额、平台费、结算等待期、Webhook timeout/retry、Client service key rotation。
- Download Billing：默认模式、授权 TTL、角色/等级免费条件、单次/每日/附件收入上限。
- AI：Provider allowlist、Secret ref、模型、data mode、预算、timeout、retention disclosure。
- Video：Provider hosts、embed hosts、CSP、HLS depth/segments/bytes、redirect、timeout、metadata refresh。
- Feature Flag：默认值、灰度条件、紧急关闭；Flag 不能绕过权限或账务。

## 4. Caddy

概念配置：

```caddyfile
community.example.com {
    encode zstd gzip

    @backend path /api/v1/* /.well-known/openid-configuration /oauth/* /healthz
    reverse_proxy @backend 127.0.0.1:8080

    reverse_proxy 127.0.0.1:3000

    header {
        X-Content-Type-Options nosniff
        Referrer-Policy strict-origin-when-cross-origin
        Permissions-Policy "camera=(), microphone=(), geolocation=()"
    }
}
```

- `/readyz` 默认不公开，供本机/受控监控访问。
- CSP 需结合 SvelteKit 构建生成，不在示例里硬写不兼容值。
- 客户 IP 只从 Caddy 可信链获取。
- 请求体大小在 Caddy 与 Rust 两层限制。

## 5. 健康检查

### `/healthz`

- 只说明进程活着。
- 不访问外部依赖。
- 返回最少信息。

### `/readyz`

检查：

- 数据库可连接和迁移版本匹配。
- OIDC 启用时 active signing key 可解密。
- 关键数据目录可访问。
- 不要求 SMTP/S3 每次实时探测，否则第三方抖动会错误摘除主站。

详细依赖状态放受保护管理员接口。

## 6. 日志、指标和追踪

- Rust 使用 `tracing` 输出 JSON：timestamp、level、service、request_id、route、status、latency。
- SvelteKit 使用同一 request ID 并输出结构化日志。
- 不记录 Cookie、Authorization、OAuth code/token、密码、完整邮箱和隐藏正文。
- 慢请求和数据库慢查询采样记录。

初始指标：

- HTTP 请求数、状态、耗时。
- DB pool 使用、等待、错误、SQLite busy。
- Session 登录失败/锁定。
- Job/Outbox 堆积和 dead 数。
- 上传处理失败。
- OAuth token 错误与 refresh reuse 事件。

可使用 Prometheus endpoint，但应限制访问；OpenTelemetry 作为可选输出。

## 7. SQLite 配置

- `journal_mode=WAL`。
- `foreign_keys=ON` 每连接设置。
- 配置合理 `busy_timeout`。
- 写事务尽可能短，禁止事务内网络/图片处理。
- 连接池写并发保守；实际值通过基准测试决定。
- 定期 checkpoint；监控 WAL 大小。
- 备份使用 SQLite Online Backup API、`.backup` 或验证过的 `VACUUM INTO` 流程，不能直接复制活跃 db 文件而忽略 WAL。

## 8. 迁移

部署步骤：

1. 备份并验证可读。
2. 上传新 release。
3. 运行 `bblbb migrate --check`。
4. 阅读迁移兼容性和所需停机窗口。
5. 显式运行 `bblbb migrate`。
6. 切换 current symlink 并重启。
7. 验证 ready、冒烟和 worker。

- 迁移有 checksum，发布后不可修改旧文件。
- 大表/破坏性变更采用 expand-contract。
- 默认不在每次服务启动自动执行生产迁移。
- 迁移失败停止部署，不启动未知 schema 的应用。

## 9. 升级与回滚

代码回滚只有在数据库迁移向后兼容时安全。

- 每个 release note 标记 migration compatibility。
- expand 阶段旧/新代码都能运行；contract 迁移延后到确认无回滚需求。
- 若迁移不可逆，回滚依赖备份恢复并意味着数据窗口损失，应事先维护模式和确认。
- 前后端版本通过 API compatibility 范围协调；部署时优先发布兼容后端，再发布前端。
- OIDC signing key 不因代码回滚而变化或丢失。

## 10. 备份

### 内容

- 数据库。
- 本地附件或 S3 bucket/version。
- 数据型主题/插件配置。
- 加密后的 OIDC 私钥。
- 配置（去除或单独安全保存 secret）。
- 解密主密钥的独立灾难恢复副本。

### 目标

初始建议：

- RPO：24 小时；重要站点提高到 1 小时/持续 binlog。
- RTO：4 小时。
- 每日备份、每周完整恢复演练抽样。
- 至少一份异地、加密、不可被应用服务账号删除的备份。

### MySQL/MariaDB

使用一致性 dump/物理备份和 binlog 策略，记录工具版本；MariaDB 与 MySQL 恢复分别测试。

## 11. 恢复流程

1. 新隔离环境准备相同/兼容版本。
2. 恢复数据库。
3. 恢复附件和密钥。
4. 运行迁移状态、行数、外键和附件 hash 校验。
5. 使用测试域名执行登录、发帖、下载、积分和 OIDC 签名验证。
6. 再切换生产流量。

恢复演练不能只验证“备份文件存在”。

## 12. systemd 加固

建议：

```text
User=bblbb
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/bblbb
RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6
```

具体沙箱需对 SMTP/S3/监听方式测试；不能复制配置后导致密钥或网络不可用。

## 13. 优雅停机

- 收到 SIGTERM 后 readiness 立即失败。
- HTTP 停止接新请求并等待配置期限。
- worker 停止领取任务，释放/让租约过期。
- 数据库事务在期限内完成，不强杀到一半。
- Caddy 配合健康检查或重启顺序减少错误窗口。

## 14. 容量与告警

至少告警：

- 磁盘可用空间 < 20%/10%。
- SQLite WAL 异常增长或 busy 错误。
- MySQL 连接池等待。
- 5xx/延迟上升。
- 备份失败或长时间未成功。
- Outbox/mail/security job 堆积。
- OIDC key 即将过期/无法解密。
- 附件 quarantined/处理失败异常上升。
- 数据库计费容量与物理对象字节差异。
- 用户配额拒绝率异常上升，避免等级配置错误导致全站无法上传。
- S3 临时链接签发失败率、异常 TTL 和重复鉴权失败。

S3 临时链接到期不触发对象清理。对象删除只由用户主动删除、业务保留策略或管理员清理触发，并按软删、保留期、幂等物理删除流程执行。

## 15. 安装与首个管理员

- 首次运行生成一次性 bootstrap token，输出到 root 可读文件/终端，不写普通日志。
- 管理员通过本机命令或受限页面使用 token 初始化。
- 成功后 token 立即失效并删除。
- 禁止在默认镜像中内置管理员密码。
- 初始化创建站点 origin、默认角色、权限、货币和主题。

## 16. 存储后端迁移

本地磁盘与 S3 之间切换不是简单修改环境变量，必须在维护窗口按以下流程执行：

1. 冻结新上传和附件删除，等待 `pending/processing` 清零；数据库和源存储创建可恢复备份。
2. 使用受控迁移命令按 attachment ID 复制对象到目标后端，目标 key 由系统映射；支持断点续传且不修改源对象。
3. 比对 ready 附件数量、字节数、size/hash、variant 和随机抽样下载；目标 Bucket 保持私有。
4. 在隔离环境或只读验证模式执行头像、封面、公开附件、受限附件和 Range 下载冒烟测试。
5. 修改 `BBLBB_STORAGE_BACKEND` 并重启，验证 `/readyz`、上传、处理 worker、下载和清理任务后再解除维护模式。
6. 保留源存储至少一个回滚窗口；若错误率、缺失对象或权限异常超过阈值，立即恢复旧配置。回滚期间产生的新对象必须有双写、增量回迁或明确停写策略，不能静默丢失。
7. 回滚窗口结束并完成第二次一致性校验后，才可按审批流程清理源对象；清理必须与备份保留策略协调。

建议提供：

```text
bblbb storage test
bblbb storage migrate --from local --to s3 --resume
bblbb storage verify --source local --target s3
```

迁移进度和错误只记录 attachment ID、错误码和 request ID，不记录 Secret 或完整签名 URL。详细对象契约见 [`STORAGE.md`](STORAGE.md)。

## 17. 跨数据库迁移

未来提供：

```text
bblbb export --format domain-jsonl
bblbb import --format domain-jsonl --database-url ...
bblbb verify-migration ...
```

- 按依赖顺序导出稳定领域对象，不直接转换 SQL dump。
- 校验行数、UUID、账本余额、附件 hash、grant 和 OAuth consent。
- OIDC token 可选择全部撤销，降低跨环境泄漏风险。
- 迁移在维护窗口完成，不宣传为零停机连接串切换。
