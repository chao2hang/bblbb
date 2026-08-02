# BBLBB — 安全基线与威胁模型

> 版本：v0.3
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

## 12. 限流与反滥用

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

## 13. 秘密与供应链

- `.env` 不进入版本库；生产使用 systemd credentials、Docker secrets 或权限受限的秘密文件。
- OIDC 私钥、SMTP 密码、S3 secret 必须支持轮换。
- Rust 使用 `cargo audit`、`cargo deny`；前端使用 pnpm audit 与依赖更新机器人。
- 锁文件提交；CI 构建使用冻结锁文件。
- 发布产物生成 SBOM 和校验和；容器使用非 root、只读根文件系统和固定基础镜像 digest。
- 数据型主题/插件配置包按不可信压缩包处理；代码型扩展按完整供应链代码处理。

## 14. 隐私、日志与审计

- 普通日志不记录密码、Cookie、Authorization、OAuth code/token、完整邮箱、隐藏正文和附件签名 URL。
- 日志使用结构化字段和 request ID；安全审计与应用调试日志分开保留。
- IP 最小化保存，可保存前缀哈希并配置保留期限。
- 用户可导出数据并发起注销；延迟期后进行匿名化/删除。
- 审计日志不可由普通管理员 API 修改；清理需专用策略并记录清理事件。
- OIDC Client 所得 claim 受 scope 和同意控制。

## 15. 部署加固

- Caddy、SvelteKit、Rust 均以独立非 root 用户/容器运行。
- Rust 和数据库只监听 loopback/内部网络。
- systemd 使用 `NoNewPrivileges`、`PrivateTmp`、`ProtectSystem=strict`，仅开放数据目录写权限。
- 优雅停机时停止接新请求、等待短事务、释放任务锁。
- 数据库、附件、配置和 OIDC 私钥都进入加密备份和恢复演练。
- `/healthz` 不泄漏内部信息；`/readyz` 只对受控网络或内部探针开放详细状态。

## 16. 安全验收

发布前至少覆盖：

- OWASP 常见风险测试、依赖扫描和秘密扫描。
- IDOR、越权、CSRF、Session fixation、开放重定向。
- 隐藏正文通过 API、缓存、SEO、日志和搜索泄漏的测试。
- OAuth 授权码重放、PKCE、redirect、Refresh Token 重用和密钥轮换。
- 压缩炸弹、路径穿越、MIME 欺骗和图片炸弹。
- SQLite/MySQL/MariaDB 下积分并发和幂等。

完整矩阵见 [`TESTING.md`](TESTING.md)。
