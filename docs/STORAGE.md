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

## 2. 生命周期

```text
pending → processing → ready
   └───────────────→ quarantined
ready → deleted（软删）→ purged（物理删除）
```

1. 创建 `pending` 元数据。
2. 上传到临时 key。
3. 校验大小、magic/MIME、扩展名、hash 和配额。
4. 图片隔离重解码、去除元数据并生成缩略图。
5. 原子/受控移动到最终 key，设 `ready`。
6. 只有 ready 文件可以关联已发布内容。

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

## 8. 配额

配额可按：

- 用户总字节、每日上传字节、文件数量。
- 等级 benefit。
- 板块单文件上限。
- 站点全局磁盘水位。

实际安全限制取所有适用限制的最小值。处罚可进一步降低额度。达到高水位时先停止新上传，不影响阅读现有文件。

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
