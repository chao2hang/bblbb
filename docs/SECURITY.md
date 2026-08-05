# BBLBB — 安全基线与威胁模型

> 版本：v0.4
> 本文定义跨模块安全要求；会话/OIDC 见 `AUTH-OIDC.md`，权限见 `AUTHORIZATION.md`，文件见 `STORAGE.md`。

## 1. 信任边界

```text
不可信：浏览器、用户内容、上传文件、主题数据包、插件配置包、OAuth Client
   │
Caddy：TLS、请求大小、安全响应头、代理边界
   │
SvelteKit：不可信请求的 SSR/UI 层，不是授权边界
   │
Rust：身份、权限、CSRF、内容可见性、业务事务和协议边界
   │
数据库/对象存储：只允许 Rust 服务凭据访问
```

- 后端监听 loopback 不等于可信请求；每个请求仍需认证和授权。
- SvelteKit 被攻破时不能凭内部网络直接取得管理员权限。
- 管理员上传的代码型主题视为受信任应用代码，而非沙箱代码。

## 2. 身份和密码

- 使用 Argon2id PHC 字符串，参数在目标小机器上基准测试后固定，并保存算法参数以便未来重哈希。
- 允许长 passphrase；最小长度建议 10，不强制“必须包含数字和特殊字符”。
- 检查常见/已泄漏密码，可采用本地 Bloom/filter 数据或隐私保护查询。
- 登录错误不区分账号不存在、未验证或密码错误。
- 忘记密码 token 为一次性高熵随机值，数据库只存哈希，默认 30 分钟过期。
- 密码重置后撤销其他 Session，并发送安全通知。
- 管理员账号建议强制 TOTP；恢复码只保存哈希。

## 3. Session Cookie

建议 Cookie：

```text
__Host-bblbb_session=<random>
Path=/
Secure
HttpOnly
SameSite=Lax
```

- 不设置 `Domain`，利用 `__Host-` 约束。
- OAuth 重定向不要求把主 Session Cookie 改为 `SameSite=None`；顶层 GET 回跳适用于 Lax。
- Session 同时具有 idle timeout 与 absolute timeout。
- 数据库只保存 token 哈希。
- Session ID 在登录、权限提升和密码修改后旋转。
- 用户可查看和撤销设备 Session；管理员强制撤销必须写审计。
- 不用 IP 或 User-Agent 作为硬绑定，避免移动网络误伤；只用于风险提示和限流信号。

## 4. CSRF 与请求来源

采用 Session 绑定的 synchronizer token：

1. Rust 为 Session 生成 CSRF secret，只在数据库中保存其哈希/受保护表示。
2. `GET /api/v1/auth/csrf` 返回与当前 Session 和上下文绑定的短期 token；响应 `private, no-store`。匿名登录/注册流程使用独立的预认证 CSRF Cookie/状态，不能借此获得用户身份。
3. 所有使用 Cookie 身份的 POST/PUT/PATCH/DELETE 要求 `X-CSRF-Token`。
4. Rust 验证 token、`Origin`；缺少 Origin 时按策略校验 `Referer`。
5. OAuth `/authorize` 的同意提交同样要求 CSRF。

- GET/HEAD/OPTIONS 必须无业务副作用。
- Bearer Token API 不依赖 Cookie 时不要求 CSRF，但必须防 Token 泄漏。
- CORS 默认关闭；若未来开放，使用精确 Origin 白名单，禁止凭据搭配 `*`。

## 5. 授权与对象级访问

- 每个 handler 同时检查动作权限和对象范围。
- 列表查询在仓储层就过滤不可见资源，不能先查全部再由前端隐藏。
- 更新/删除使用目标 ID 和版本条件，防止 IDOR 与丢失更新。
- 管理员越权查看隐藏内容必须写 `audit_logs`。
- 403 与 404 的选择按资源存在性泄漏风险统一规定。
- 主题、插件和前端控件隐藏不是安全控制。

## 6. 用户内容和 XSS

- 用户内容仅接受 Markdown，不接受原始 HTML 或 BBCode。
- Rust 后端使用 Rust Markdown 解析器生成 HTML，并执行标签、属性和 URL 协议白名单清洗。
- 保存 `body_markdown` 和 `body_html`；渲染器或清洗规则升级后可后台重建 HTML。
- 前端只有专用 `SanitizedHtml` 可进入 `{@html}`。
- 禁止 `script`、事件属性、`javascript:`、危险 `data:`、未经白名单的 iframe 和 SVG。
- 外链增加 `ugc nofollow noopener noreferrer`。
- 隐藏正文不包含在未授权 API 响应、DOM、日志、异常或遥测中。

## 7. 安全响应头

Caddy/SvelteKit 统一设置：

```text
Strict-Transport-Security: max-age=31536000; includeSubDomains
X-Content-Type-Options: nosniff
Referrer-Policy: strict-origin-when-cross-origin
Permissions-Policy: camera=(), microphone=(), geolocation=()
Cross-Origin-Opener-Policy: same-origin
Content-Security-Policy: default-src 'self'; ...
```

- HSTS `includeSubDomains` 只有在确认所有子域均支持 HTTPS 后启用；预加载另行评估。
- 使用 frame-ancestors CSP 控制嵌入，优先于旧 `X-Frame-Options`。
- CSP 默认不允许任意远程脚本；SvelteKit 内联脚本使用 nonce/hash 或框架兼容方案。
- `style-src` 是否使用 `'unsafe-inline'` 需经过实际构建验证并记录风险；不因主题上传任意放宽。
- 头像/CDN 域名以精确允许列表进入 `img-src`。

## 8. OAuth/OIDC

最低要求：

- Authorization Code + PKCE S256；不支持 implicit/password grant。
- `redirect_uri` 规范化后精确匹配已登记值。
- `state` 由 Client 负责并在文档中强烈要求；Provider 使用 `nonce` 防 ID Token 重放并按 OIDC 校验。
- 授权码一次、短时、原子消费。
- ID Token 检查 `iss/aud/exp/iat/nonce/auth_time`。
- opaque Access Token 和 Refresh Token 都只存哈希。
- Refresh Token 每次轮换；旧 token 重用撤销整个 family。
- RS256 私钥加密存储，`kid` 轮换时旧公钥保留至所有 Token 过期。
- 授权页面显示 Client、redirect 域名和 scope；不得进行开放重定向。
- 使用 Pairwise Subject，默认不向第三方暴露内部用户 ID 和角色。

详细协议见 [`AUTH-OIDC.md`](AUTH-OIDC.md)。

## 9. SQL、SSRF 和反序列化

- 所有值参数化；动态排序、列名和表名只能来自枚举白名单。
- 数据库账号使用最小权限；应用账号不能创建用户或修改服务器配置。
- v1 插件不允许通用 HTTP 调用。
- 服务端读取外部 URL 时使用协议/域名白名单、DNS 重绑定防护、私有/链路本地地址阻断、响应大小和超时限制。
- JSON/TOML/压缩包均限制深度、大小、条目数和解压比。
- 错误响应不返回 SQL、路径、密钥、栈信息或用户隐藏正文。

## 10. 文件上传

- 以内容 magic/MIME 和允许扩展名共同判断，不信任浏览器 Content-Type。
- 原文件名仅作展示，存储 key 为随机不可猜值。
- 图片在隔离流程中重新解码；限制像素数以防解压炸弹。
- 默认拒绝 SVG；若开放则进行严格清洗并以附件方式下载。
- 文档附件设置 `Content-Disposition: attachment` 和安全 Content-Type。
- 上传先进入 `pending/quarantined`，处理完成后变为 `ready`。
- S3 公开/预签名链接必须有有限有效期；链接到期只使该 URL 失效，附件对象不因此删除。每次重新签发都要重新鉴权。
- 等级单文件限制和总容量由 Rust 在创建与完成阶段双重校验；浏览器显示的剩余额度不是授权依据。
- 本地存储位于 Web 根目录之外，不使用 `.htaccess` 等与 Caddy 无关的假设。
- 私有附件由 Rust 鉴权后流式传输；S3 可发短期签名 URL。

## 11. 积分与付费解锁

- 余额与不可变流水在同一事务中完成。
- 所有外部重试路径使用幂等键。
- SQLite 使用 `BEGIN IMMEDIATE`；MySQL/MariaDB 使用行锁。
- 乐观更新必须检查 `rows_affected == 1`。
- 支付解锁在同一事务内完成扣费和 grant 创建，重复请求返回已有 grant。
- 管理员调整需要原因、二次确认和审计；大额调整可配置双人复核。
- 不修改或删除历史流水，撤销使用补偿交易。

## 12. 下载抵扣积分

- 下载扣费必须由 Rust 在后端重新鉴权和计算价格，浏览器提交的金额、货币、用户和附件所有权字段全部不可信。
- 账户扣减、不可变流水、下载授权、审计和 Outbox 同一事务；生成 S3 临时 URL 不重复扣费。
- 同一用户和附件的有效授权可复用；Idempotency-Key 防止重复点击和网络重试造成重复扣款。
- URL 失效只影响 URL；下载授权有效期、附件对象生命周期和 S3 链接 TTL 是三个独立概念。
- 后台策略变更只作用于新授权，不能修改历史流水或撤销既有授权；退款使用补偿交易。
- URL 签发失败不得再次扣费，客户端通过原幂等键查询已提交授权并重试签发。
- 详见 [`DOWNLOAD-BILLING.md`](DOWNLOAD-BILLING.md)。

## 13. 公开市场交易

- 普通 `openid/profile/email` scope 永远不能扣款；市场交易使用独立高风险 scope、管理员批准和用户单独同意。
- 仅 Confidential Client 可获得购买/退款能力；Secret 只存安全 hash，业务 API 使用短期 opaque Access Token。
- 金额、货币、收款方和物品版本来自服务端登记的 Offer/Checkout Intent，不信任市场提交的价格字段。
- 购买、账户扣款、不可变流水、意图消费、审计与 Outbox 同事务；成功响应必须代表数据库已经提交。
- 创建意图、购买和退款强制幂等；短效意图一次消费，并绑定用户、Client、Offer 版本与金额，防止重放和换价。
- Webhook 在提交后通过 Outbox 投递，使用每 Client 独立可轮换密钥签名，并执行 SSRF 防护；它不是账务事实来源。
- 禁止直接修改历史交易；退款使用受限的补偿交易。完整协议见 [`MARKETPLACE.md`](MARKETPLACE.md)。

## 14. 大模型与外部 Provider

- 浏览器、插件和用户配置不能直连模型 Provider；所有调用经过 Rust AI Gateway 和已批准适配器。
- API Base URL 必须 HTTPS、精确域名白名单，阻断私网/loopback/链路本地地址、DNS 重绑定、任意重定向和超大响应，防止 SSRF。
- Provider Secret 只存 Secret Store 或加密受保护配置；GET 只返回脱敏状态，不进入前端、localStorage、日志、审计 metadata 或错误响应。
- 用户内容默认脱敏，隐藏正文和私密审核备注不外发；完整内容发送需单独同意、展示 Provider 留存/训练/区域信息并支持撤回。
- 模型输入和输出均不可信：禁止输出直接作为 HTML、SQL、模板、权限、审核处罚、价格或积分操作执行；必须经过 schema、长度、XSS/Markdown 和业务规则校验。
- 内容审计只产生风险建议，不能单独永久封禁、删除或放行高风险内容；人工审核和核心规则仍是最终裁决。
- AI 任务异步、幂等、可取消、有限重试并带预算/并发/熔断；Provider 故障不能绕过安全规则，也不应阻塞普通发帖。
- 详见 [`AI.md`](AI.md)。

## 15. 视频嵌入与第三方媒体

- 视频 URL 只能经 Rust Video Service 解析为结构化引用；禁止用户提交任意 iframe/HTML，禁止 `javascript:`, `data:`, userinfo、非 HTTPS、私网和 loopback 地址。
- 出站探测使用精确 Host 白名单、TLS、DNS 重绑定防护、重定向限制、超时、响应大小和并发限制，防止 SSRF。
- HLS master/media playlist、分片、Key、Map 和重定向必须逐个经过来源策略；默认不代理、不保存密钥、不转存第三方流。
- 西瓜视频仅支持公开页面/官方嵌入白名单；不抓取签名播放地址，不绕过登录、地域、DRM 或平台限制；失败降级为外链卡片。
- iframe 必须 CSP `frame-src` 精确白名单，`sandbox`、`referrerpolicy` 和 `allow` 最小化；默认禁止自动播放、摄像头和麦克风。
- 受限、审核中或不可见帖子不加载第三方播放器，避免内容存在性和用户阅读权限泄漏。
- 完整协议见 [`VIDEO-PLUGIN.md`](VIDEO-PLUGIN.md)。

## 16. 限流与反滥用

限流按多信号组合：IP/网段、账号、Session、动作和设备风险；User-Agent 只作为弱信号。

建议初始值：

| 动作 | 默认限制 |
|---|---|
| 登录 | 每 IP 10/分钟；每账号连续失败 5 次短时锁定 |
| 注册 | 每 IP 3/小时，并支持 Turnstile |
| 找回密码 | 每账号和每 IP 分别限流，但始终返回相同响应 |
| 发帖/回复 | 按账号年龄、等级和处罚动态限制 |
| OAuth token | 每 Client、IP、grant 分别限流 |
| 上传 | 并发、每日字节数和用户配额 |

- 单机使用进程内限流 + 数据库账号锁定；多实例再引入 Redis。
- 代理 IP 只信任 Caddy 注入且来自 loopback/配置的可信代理。
- 429 返回 `Retry-After`。

## 17. 秘密与供应链

- `.env` 不进入版本库；生产使用 systemd credentials、Docker secrets 或权限受限的秘密文件。
- OIDC 私钥、SMTP 密码、S3 secret 必须支持轮换。
- S3 生产环境优先使用实例角色、Workload Identity 或短期凭据；静态密钥仅授予指定 Bucket/前缀的最小对象权限，不授予 Bucket Policy、公开 ACL 或跨 Bucket 管理权限。
- `/admin/storage` 不返回 Secret 明文；浏览器、本地存储、SSR payload、普通配置导出、错误和日志中均不得出现 S3 Secret 或完整预签名 URL。
- Bucket 默认私有并启用 Block Public Access；CORS 使用本站精确 Origin。凭证轮换应允许新旧凭证短时重叠验证，成功后撤销旧凭证并写安全审计。
- Rust 使用 `cargo audit`、`cargo deny`；前端使用 pnpm audit 与依赖更新机器人。
- 锁文件提交；CI 构建使用冻结锁文件。
- 发布产物生成 SBOM 和校验和；容器使用非 root、只读根文件系统和固定基础镜像 digest。
- 数据型主题/插件配置包按不可信压缩包处理；代码型扩展按完整供应链代码处理。

## 18. 隐私、日志与审计

- 普通日志不记录密码、Cookie、Authorization、OAuth code/token、完整邮箱、隐藏正文和附件签名 URL。
- 日志使用结构化字段和 request ID；安全审计与应用调试日志分开保留。
- IP 最小化保存，可保存前缀哈希并配置保留期限。
- 用户可导出数据并发起注销；延迟期后进行匿名化/删除。
- 审计日志不可由普通管理员 API 修改；清理需专用策略并记录清理事件。
- 审计 before/after 使用字段 allowlist（`AUDIT_FIELD_ALLOWLIST`，M01-AUDIT-02）：
  非白名单字段（密码、Token、Secret、隐藏正文、完整签名 URL）一律丢弃；
  白名单字段的字符串若含密码/Secret/Bearer/签名 URL/token 形态则脱敏为
  `[REDACTED]`。
- OIDC Client 所得 claim 受 scope 和同意控制。

## 19. 部署加固

- Caddy、SvelteKit、Rust 均以独立非 root 用户/容器运行。
- Rust 和数据库只监听 loopback/内部网络。
- systemd 使用 `NoNewPrivileges`、`PrivateTmp`、`ProtectSystem=strict`，仅开放数据目录写权限。
- 优雅停机时停止接新请求、等待短事务、释放任务锁。
- 数据库、附件、配置和 OIDC 私钥都进入加密备份和恢复演练。
- `/healthz` 不泄漏内部信息；`/readyz` 只对受控网络或内部探针开放详细状态。

## 20. 安全验收

发布前至少覆盖：

- OWASP 常见风险测试、依赖扫描和秘密扫描。
- IDOR、越权、CSRF、Session fixation、开放重定向。
- 隐藏正文通过 API、缓存、SEO、日志和搜索泄漏的测试。
- OAuth 授权码重放、PKCE、redirect、Refresh Token 重用和密钥轮换。
- 压缩炸弹、路径穿越、MIME 欺骗和图片炸弹。
- SQLite/MySQL/MariaDB 下积分并发和幂等。

完整矩阵见 [`TESTING.md`](TESTING.md)。
