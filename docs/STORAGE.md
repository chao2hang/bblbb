# BBLBB — 附件与对象存储

> 版本：v0.3
> v1 支持本地磁盘；可选 S3 兼容对象存储。所有附件先通过 Rust 元数据和权限层，不直接信任上传路径或对象 key。

## 1. 存储适配器

统一接口概念：

```text
put_temp
commit
open_stream
presign_upload（可选）
presign_download（可选）
delete
head
```

实现：

- `LocalStorage`：默认，小机器单机目录。
- `S3Storage`：S3/MinIO/R2 等兼容服务，具体兼容性需测试。

业务层只使用 attachment ID，不暴露物理路径作为资源 ID。

### 1.1 支持范围与兼容目标

`S3Storage` 以 AWS S3 API 的必要子集为契约，目标服务包括：

- AWS S3。
- MinIO。
- Cloudflare R2。
- 其他通过适配器契约测试的 S3 兼容服务。

不承诺所有厂商扩展功能。上线前必须分别验证 `PutObject`、`HeadObject`、`GetObject`、`DeleteObject`、Multipart/预签名行为、签名版本、Path-style 与 Virtual-hosted-style 地址模式。

### 1.2 配置项

正式服务通过环境变量或受保护配置源读取以下配置；Secret 不进入前端、普通配置导出或日志：

| 配置 | 示例 | 说明 |
|---|---|---|
| `BBLBB_STORAGE_BACKEND` | `local` / `s3` | 安装时选择；默认 `local` |
| `BBLBB_STORAGE_LOCAL_PATH` | `/var/lib/bblbb/uploads` | 本地目录，必须位于 Web 根目录外 |
| `BBLBB_S3_ENDPOINT` | `https://s3.amazonaws.com` | 自定义服务必须使用 HTTPS；开发环境 MinIO 可例外 |
| `BBLBB_S3_REGION` | `ap-southeast-1` / `auto` | R2 可使用厂商要求值 |
| `BBLBB_S3_BUCKET` | `bblbb-attachments` | Bucket 默认私有 |
| `BBLBB_S3_ACCESS_KEY_ID` | — | 仅服务端读取 |
| `BBLBB_S3_SECRET_ACCESS_KEY` | — | Secret，不回显、不记录 |
| `BBLBB_S3_PATH_STYLE` | `false` | MinIO 等兼容服务可开启 |
| `BBLBB_S3_PUBLIC_BASE_URL` | `https://cdn.example.com` | 仅公开、不可变资源可使用 |
| `BBLBB_S3_PRESIGNED_UPLOADS` | `true` | 是否启用浏览器预签名直传 |
| `BBLBB_S3_SIGNED_URL_TTL_SECONDS` | `300` | 私有下载建议 60–3600 秒 |
| `BBLBB_UPLOAD_MAX_BYTES` | `20971520` | 站点硬上限；用途限制仍取更小值 |

生产环境优先使用实例角色、Workload Identity 或短期凭据；必须使用静态密钥时，应设置最小权限并支持轮换。后台 `/admin/storage` 只展示脱敏状态，保存 Secret 后不得再次读取明文。

配置必须有单一事实来源：环境变量/Workload Identity 可设为只读；若允许后台修改，则非 Secret 配置写入受保护系统配置，Secret 使用主密钥加密后存储。环境变量覆盖数据库配置时，后台必须标记“由部署配置管理”并禁用对应输入，不能展示已保存但实际不生效的值。

### 1.3 最小 IAM/Bucket 权限

应用服务账号只允许访问指定 Bucket 和前缀，至少遵循：

- 允许对象 `Put/Get/Head/Delete`，仅限 `attachments/*`、临时上传和受控主题包前缀。
- 只有启用 Multipart 时才授予对应 Multipart 权限。
- 不授予创建/删除 Bucket、修改 Bucket Policy、公开 ACL 或访问其他 Bucket 的权限。
- Bucket Block Public Access 保持开启；对象 ACL 不作为业务权限来源。
- CORS 只允许本站精确 Origin、所需方法和请求头，不使用 `*` 搭配凭据。
- 推荐启用服务端加密、版本化和生命周期规则；合规场景按厂商能力使用 KMS。

## 2. 生命周期

```text
pending → processing → ready
   └───────────────→ quarantined
ready → expired（到期、立即禁止访问）→ purged（宽限期后物理删除）
ready → deleted（主动软删）→ purged（物理删除）
```

1. 创建 `pending` 元数据。
2. 上传到临时 key。
3. 校验大小、magic/MIME、扩展名、hash 和配额。
4. 图片隔离重解码、去除元数据并生成缩略图。
5. 原子/受控移动到最终 key，设 `ready`。
6. 只有 ready 且未到期的文件可以关联已发布内容。
7. 创建时必须写入 `expires_at`；到期判定以服务端时间为准，请求路径实时拒绝，清理任务不构成授权边界。

失败任务进入 quarantined 或删除临时文件，并向用户返回安全原因码。

## 3. 文件限制

初始默认值（均可降低）：

| 用途 | 大小 | 类型 |
|---|---:|---|
| 头像 | 2MB | JPEG/PNG/WebP/AVIF（按库支持） |
| 封面 | 8MB | JPEG/PNG/WebP/AVIF |
| 帖子图片 | 12MB | JPEG/PNG/WebP/AVIF/GIF（GIF 可限制帧数） |
| 文档附件 | 20MB | 管理员明确白名单 |

- 同时限制图片像素数、帧数和解码内存。
- 默认拒绝 SVG、HTML、脚本、可执行文件和压缩包。
- 若未来允许压缩包，只作为下载附件，不解压到 Web 路径。
- 扩展名与 magic 不一致则拒绝或隔离。

## 4. 存储 key

推荐：

```text
attachments/<yyyy>/<mm>/<uuid>/<variant>.<safe-ext>
```

- key 由服务端生成。
- 原始文件名仅保存为清洗后的下载展示名。
- 不允许用户输入 `..`、绝对路径、控制字符或路径分隔符进入 key。
- 本地实现禁止符号链接逃逸，数据目录位于 Web 根目录之外。

## 5. 图片处理

- 在受限 worker 中解码和重新编码，移除 EXIF/GPS 等元数据。
- 限制并发，避免小机器 OOM。
- 生成固定规格缩略图，variant 记录处理参数版本。
- 动图可仅保留第一帧或严格限制尺寸/帧数。
- 处理库漏洞纳入依赖审计。
- 失败时原图不自动公开。

## 6. 下载与访问控制

### 公开附件

- Rust 校验附件状态和引用资源可见性。
- 可返回 Caddy 内部加速指示或短期 S3 签名 URL。
- 公开静态资产可长缓存，文件名必须内容哈希/不可变。

### 私有/受限附件

- 每次下载校验关联帖子/回复的可见性或 grant。
- `Cache-Control: private, no-store`。
- S3 签名 URL 有短有效期，并绑定响应头；不要记录完整 URL。
- 本地存储由 Rust 流式发送，支持 Range 时也要先鉴权。

文档类附件默认：

```text
Content-Disposition: attachment; filename*=UTF-8''...
X-Content-Type-Options: nosniff
```

## 7. 引用与清理

- `attachment_links` 记录附件与头像、帖子、回复、主题资产等关系。
- 发布/修改内容时在事务中同步链接元数据。
- `ref_count` 只是缓存，可通过链接表重建。
- 未绑定 pending 上传在 24 小时后清理。
- 软删资源的附件在保留期后再清理。
- 清理采用 mark-and-sweep：先标记，再延迟物理删除，降低竞态误删。
- 备份期间避免与 purge 任务冲突，或使用对象版本化。

## 8. 有效期与等级配额

### 8.1 所有附件均有有效期

- `expires_at` 为必填字段，不允许 `null`、无限期或通过公开 URL 绕过。
- 创建附件时，客户端可在允许范围内选择更短期限；最终期限不得超过“站点最长有效期、等级最长有效期、用途/板块最长有效期”的最小值。
- 到达 `expires_at` 后，元数据原子变为 `expired` 或由读取时实时视为过期，公开与私有下载、缩略图、Range 和新签名 URL 都必须拒绝。
- 已签发 URL 的 TTL 不得超过附件剩余有效期；临近过期时缩短 TTL 或拒绝签发。
- 到期附件进入宽限期，供审计和失败恢复，但用户不可访问；`purge_after` 后异步删除原对象和所有 variant。
- 正文中的到期附件保留不可点击占位，显示“附件已过期”，不得自动变成损坏图片或泄漏物理 key。
- 管理员延期属于受审计写操作，仍受站点最长有效期约束；不能把附件改成永久。

### 8.2 等级配额

每个等级至少配置：

- `attachment_max_bytes`：单个原始附件最大字节数。
- `attachment_total_bytes`：该用户所有计费附件总容量。
- `attachment_max_ttl_seconds`：附件最长有效期。
- 可选 `attachment_count` 和每日上传字节数。

实际单文件大小、总容量和期限分别取站点、用途、板块、等级及处罚规则中的最严格值。上传授权和 `complete` 两个阶段都必须重新读取当前等级与已用容量，不能只信任预签名时的快照。

容量口径包括 `pending/processing/ready/expired` 但尚未物理清理的原文件及计费 variant；`quarantined` 是否计费由站点策略明确，系统故障产生的对象不应永久占用用户额度。只有对象物理删除并更新计数后才释放容量，防止删除任务失败导致超卖。

用户升级后新上传立即使用新额度。用户降级不会删除或缩短已有附件，但若当前用量超过新总容量，则禁止继续上传；已有附件仍按原 `expires_at` 到期。处罚可进一步降低额度。达到站点存储高水位时停止新上传，不影响未到期附件阅读。

## 9. S3 上传

可选直传流程：

1. 客户端请求预签名上传，Rust 检查预期大小/类型/配额。
2. Rust 创建 pending attachment 和限定 key。
3. 客户端上传。
4. 客户端调用 complete。
5. Rust `HEAD` 校验大小，并由 worker读取 magic/hash/处理图片。
6. 通过后设 ready。

不能仅相信客户端 complete 参数或对象 Content-Type。Bucket 默认私有，CORS 精确限制站点 Origin。

## 10. 数据型主题/插件包

- 与普通用户附件分开目录和权限。
- 解压限制：条目数、总大小、单文件大小、压缩比、嵌套深度。
- 拒绝绝对路径、`..`、符号链接、硬链接和设备文件。
- 只允许 manifest 中声明的静态类型。
- 代码型主题不通过在线附件安装路径部署。

## 11. 备份与恢复

备份必须覆盖：

- 数据库附件元数据。
- 本地附件目录或 S3 bucket/version。
- 主题数据包。
- hash 与对象清单。

恢复后执行一致性检查：

- 数据库 ready 记录是否有对象。
- 对象 size/hash 是否匹配。
- 是否有无引用孤儿对象。
- 私有对象 ACL/bucket policy 是否正确。

数据库与附件备份应处于可解释的一致时间点；详见 `OPERATIONS.md`。

## 12. 测试

- 路径穿越、符号链接和文件名控制字符。
- MIME 欺骗、polyglot、SVG、图片炸弹和超大像素。
- 上传中断、complete 重放和重复 hash。
- 未授权附件、解锁后附件、grant 撤销和缓存。
- 本地/S3 两种适配器契约测试。
- 孤儿清理与备份恢复演练。
