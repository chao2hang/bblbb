# BBLBB — 视频嵌入插件与西瓜视频接入

> 版本：v0.4
> 本插件允许用户在发帖时手动插入视频 URL，支持受控的常见视频格式、HLS 和西瓜视频页面链接。插件只负责解析与安全渲染，不下载、转码或绕过平台访问控制。

## 1. 支持范围

### 直接媒体 URL

默认允许：

- `https://.../*.mp4`，`video/mp4`
- `https://.../*.webm`，`video/webm`
- `https://.../*.ogv`，`video/ogg`
- `https://.../*.mov`，仅在浏览器/服务端能力确认后允许
- `https://.../*.m3u8`，HLS，必须经过 HLS 解析和来源策略

扩展名不是可信依据。服务端可以对直接媒体 URL执行受限 `HEAD`/探测，但不得下载任意大文件；最终渲染使用结构化 `video_embed` 节点，不能把 URL 直接拼进 HTML。

### 西瓜视频

支持用户粘贴西瓜视频公开页面 URL，解析为规范化平台引用：

- 只允许精确的西瓜视频 HTTPS 主域名和官方允许的嵌入域名。
- 优先保存 `platform=xigua`、平台视频 ID、规范化页面 URL和标题/封面等公开元数据。
- 优先使用官方公开播放器/嵌入协议；官方未提供稳定嵌入协议时，降级为安全的外链卡片，不抓取播放地址、不绕过登录、签名、地域或 DRM 限制。
- 不保存或转发平台签名播放 URL；播放地址可能短期有效并且属于平台控制的访问凭证。
- 平台页面变化、限流、地区限制或删除时，帖子仍保留安全外链状态和错误提示。

## 2. 用户流程

1. 发帖编辑器点击“插入视频”。
2. 用户手动输入 URL；前端只做格式提示，后端重新解析。
3. Rust 校验 URL scheme、Host、端口、重定向、DNS 解析和插件白名单。
4. 判断 `direct/video/hls/xigua_page` 类型，返回标题、封面、时长等非敏感元数据或安全错误。
5. 用户确认插入，保存为结构化附件/媒体引用；发布时再次校验。
6. 阅读页面按权限渲染：公开视频可嵌入；受限帖子先经过帖子权限；私密/审核中内容不加载第三方播放器。

视频 URL 默认不因浏览器打开而自动抓取。后端探测和元数据刷新使用异步 job，并有单 URL、响应大小、重定向次数和总耗时限制。

## 3. 安全策略

- 只允许 HTTPS；拒绝 `javascript:`, `data:`, `blob:`（除浏览器内存预览）、`file:`, 用户信息、非标准端口和 localhost/loopback/私网/链路本地地址。
- URL 解析使用标准 URL parser，禁止字符串前缀判断、混淆 Host、userinfo、Unicode/IDN 绕过和开放重定向。
- 出站请求使用独立 egress 适配器：DNS 重绑定防护、每次连接重新校验 IP、禁止跟随到不允许 Host 的重定向，限制连接/读取超时、响应头/正文大小和并发。
- HLS master/media playlist 必须在白名单来源；递归 playlist 深度、分片数量、单片大小、总时长/总字节、密钥 URI 和重定向均受限。默认禁止外部 HLS `EXT-X-KEY`、`EXT-X-MAP` 或跨域分片，除非每个 URI 都通过同一来源策略；禁止把服务端当开放 HLS 代理。
- 不执行远程 JavaScript，不允许任意 iframe。西瓜视频 iframe 只能来自精确官方嵌入 Origin，并配合 `sandbox`、`allow` 最小权限、CSP `frame-src` 白名单和 `referrerpolicy=no-referrer`。
- 用户提交的视频标题、封面和字幕均是不可信内容，经过文本长度、URL、图片 MIME、XSS 和隐私规则校验。
- 默认不自动播放、不启用摄像头/麦克风；播放器显示来源、外链和失败状态。
- 视频抓取/解析失败不能阻塞普通发帖；返回安全外链卡片或允许用户移除引用。

## 4. 数据模型

### `video_embeds`

- `id`、`owner_id`、`post_id`/`comment_id`（规范化目标二选一）、`provider`（`direct/hls/xigua`）、`source_url`（规范化后，敏感 query 按策略剥离）。
- `canonical_url`、`external_id`、`media_type`、`poster_url`（受来源白名单）、`title`、`duration_seconds`、`status`（`pending/ready/blocked/error/removed`）。
- `metadata_json`、`policy_version`、`last_checked_at`、`created_at`、`updated_at`、`deleted_at`。
- 不保存平台签名播放 URL、Cookie、授权 Header、HLS 密钥或任意抓取响应正文。
- 索引 `(post_id, status)`、`(provider, external_id)`，必要时对用户/目标建立唯一引用。

### `video_provider_policies`

- `provider`、`enabled`、`allowed_hosts_json`、`embed_hosts_json`、`allowed_media_types_json`。
- `max_duration_seconds`、`max_bytes`、`max_redirects`、`hls_max_depth`、`hls_max_segments`、`hls_max_bytes`、`timeout_ms`、`policy_version`。
- 管理员修改立即影响新解析和新渲染；历史引用重新检查后决定继续嵌入或降级外链。

## 5. API

Video Service 是不可绕过的核心安全模块；`direct`、`hls`、`xigua` 是随应用编译和签名发布、由管理员启停的 Provider Adapter。它们可复用插件 manifest/capability 做注册和 UI 展示，但不属于运行时上传代码，也不能直接访问网络。所有网络访问必须经过核心受控 egress。

播放边界：Direct/HLS 默认由浏览器直连已验证 HTTPS 来源，服务端不代理媒体；HTML 页面按当前启用 Provider 生成精确 `media-src`、`connect-src`、`img-src`、`frame-src`。如果来源不能在不泄露凭证、跨域 Key 或宽泛 CSP 的前提下直连，则只显示外链，不提供服务端代理。西瓜视频仅使用确认过的官方 iframe；元素级 `referrerpolicy=no-referrer` 优先于站点全局策略。

```text
POST /api/v1/video-embeds/resolve       解析 URL，返回类型和安全元数据
POST /api/v1/video-embeds                创建结构化视频引用
PATCH /api/v1/video-embeds/{id}           修改用户可编辑的标题/展示字段
POST /api/v1/video-embeds/{id}/refresh     按当前策略异步重新解析元数据
DELETE /api/v1/video-embeds/{id}          删除未引用视频引用
GET /api/v1/video-embeds/{id}             获取当前请求方可见投影

GET   /api/v1/admin/video/policies
GET   /api/v1/admin/video/policies/{provider}
PATCH /api/v1/admin/video/policies/{provider}
POST  /api/v1/admin/video/policies/test
```

`resolve`、创建、修改、刷新和删除接口要求登录、CSRF、幂等键、限流和 `Cache-Control: no-store`。`refresh` 返回 202 `{ task_id, status, poll_url }`；任务完成后更新 Embed，失败时保留安全外链。解析接口返回的 Provider 元数据不是权限依据，发布和阅读时必须重新校验。

## 6. 插件边界

- 插件 manifest 声明 `video.resolve`、`video.render`、`video.metadata.refresh` capability。
- 插件不能获得数据库、Session、OAuth Token、S3 Secret、用户私密资料或通用网络请求能力。
- 插件只能调用核心 Video Service 的受控接口；Provider policy、CSP、权限和审核不能被插件关闭。
- 西瓜视频适配器只处理公开页面引用和官方嵌入；适配器失效时核心服务降级为外链，不影响帖子正文事务。

## 7. 法律与运营

- 用户必须拥有或获准分享视频 URL；发布页显示来源与版权提醒。
- BBLBB 不复制第三方视频，不移除水印，不绕过 DRM/登录/签名/地域限制，不将第三方受限流媒体转存为本站附件。
- 侵权投诉、来源下架和平台通知应能使引用进入 `blocked`，保留审计和申诉记录。

## 8. 验收

- URL 解析、Unicode/IDN、userinfo、端口、重定向、DNS 重绑定、IPv4/IPv6 私网和 SSRF。
- mp4/webm/mov/m3u8 类型欺骗、超时、超大响应、Range、恶意 MIME 和恶意元数据。
- HLS 跨域 playlist、递归 playlist、分片爆量、跨域 Key/Map、签名 URL 和密钥泄漏。
- 西瓜页面合法/非法 Host、平台 URL 变体、无嵌入权限、删除、限流和降级外链。
- 帖子权限、审核状态、受限内容、CSP、iframe sandbox、referrer 和无 JS 可用性。
- 插件 capability 越权、重复 resolve、旧策略、Provider 故障和历史引用安全降级。
