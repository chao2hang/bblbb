# BBLBB — 附件与对象存储

> 版本：v0.4
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
ready → deleted（用户/管理员主动删除）→ purged（保留期后物理删除）
```

1. 创建 `pending` 元数据。
2. 上传到临时 key。
3. 校验大小、magic/MIME、扩展名、hash 和配额。
4. 图片隔离重解码、去除元数据并生成缩略图。
5. 原子/受控移动到最终 key，设 `ready`。
6. 只有 `ready` 文件可以关联已发布内容。S3 临时访问链接过期不改变附件状态，也不删除对象。

失败任务进入 quarantined 或删除临时文件，并向用户返回安全原因码。

## 3. 文件限制

初始默认值（均可降低）：

| 用途 | 大小 | 类型 |
|---|---:|---|
| 头像 | 2MB | JPEG/PNG/WebP/AVIF（按库支持） |
| 资料 Cover | 5MB | JPEG/PNG/WebP（原型限制；正式默认可配置） |
| 封面 | 8MB | JPEG/PNG/WebP/AVIF |

个人资料 Cover 属于用户资料附件：正式环境必须先走 Rust 附件创建/完成流程和内容安全处理，再保存稳定的 `attachment_id`，页面只通过后端鉴权后的稳定内容端点或短期签名 URL 展示。S3 签名 URL 到期不删除 Cover 对象；移除 Cover 只解除用户资料引用。
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

## 8. S3 链接有效期与等级配额

### 8.1 S3 公开链接有效期

- Bucket 默认私有；“公开链接”指后端鉴权后生成的临时预签名 URL，不代表对象设置为 Public ACL。
- 管理员在 `/admin/storage` 配置 `BBLBB_S3_SIGNED_URL_TTL_SECONDS`，建议 60–3600 秒，允许范围由部署配置限定。
- URL 到期后只有该链接失效，附件元数据仍为 `ready`，S3 对象及其 variant 不删除、不占用清理队列。
- 用户再次访问时，Rust 重新检查附件状态、引用内容可见性和 grant，再签发新 URL；不能无条件刷新旧链接。
- 签名 URL 不进入数据库正文、搜索索引、通知、日志或长期缓存。公开页面也应通过稳定 attachment URL 获取临时跳转，避免把签名参数持久化。
- 上传预签名 URL 与下载/公开访问 URL 可以使用不同 TTL；两者都只控制临时凭证，不控制对象生命周期。

### 8.2 后台等级配额

管理员在 `/admin/levels` 为每个等级配置：

- `attachment_max_bytes`：单个原始附件最大字节数。
- `attachment_total_bytes`：该用户所有计费附件总容量。
- 可选 `attachment_count` 和每日上传字节数。

实际单文件大小和总容量分别取站点、用途、板块、等级及处罚规则中的最严格值。上传授权和 `complete` 两个阶段都必须重新读取当前等级与已用容量，不能只信任预签名时的快照。后台保存时要求总容量不小于单附件上限，并记录管理员审计。

容量口径包括尚未删除的 `pending/processing/ready` 原文件及计费 variant；头像、个人资料 Cover、帖子/文章封面、正文图片和普通附件使用同一个用户附件总容量，不提供独立免费 Cover 空间。Cover 的 `quota_bytes_charged` 按原图及策略规定的计费 variant 计算，更换 Cover 后旧对象在仍被引用或进入延迟清理期间继续占用额度，物理删除并结算计数后才释放。`quarantined` 是否计费由站点策略明确，系统故障产生的对象不应永久占用用户额度。只有对象物理删除并更新计数后才释放容量，防止删除任务失败导致超卖。链接过期不释放容量，因为对象仍然存在。

用户升级后新上传立即使用新额度。用户降级不会删除已有附件；若当前用量超过新总容量，则禁止继续上传，直到主动删除附件或管理员提高额度。处罚可进一步降低额度。达到站点存储高水位时停止新上传，不影响已有附件阅读。

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
