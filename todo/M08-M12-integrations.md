# M8-M12：公开访问、AI、视频、OIDC 与 Marketplace

> 总索引：[`../TODO.md`](../TODO.md)
> 这些能力都不是核心论坛的单点依赖。未配置或专项门槛未通过时，必须保持核心浏览、发帖、回复和人工审核可用。

---

<a id="m8"></a>

# M8：搜索、SEO、RSS/Atom 与反爬

**完成定义：** 仅公开且被允许的内容进入公开投影；搜索/Feed/SEO/缓存权限隔离；爬虫按行为分级处理而非只依赖 robots。

## M08-INDEX：索引与公开投影

**元数据：** `P1` · `owner=unassigned/backend-search` · `risk=high` · `depends=M03-SEARCH-STORE,M04-VISIBILITY,M05-RISK` · `blocked=none`
**目标文件：** `backend/src/search/`、`backend/src/publication/`、`backend/tests/search/`、`docs/CRAWLER-POLICY.md`
**验收：** 公开投影与内容权限 canary 在三数据库、缓存和重建流程中一致。

- [ ] `M08-INDEX-01` `[45m]` 定义公开索引文档：标题、slug、已清洗摘要、标签、作者公开投影、revision 和 index policy。
- [ ] `M08-INDEX-02` `[30m]` 实现公开/登录/回复/等级/付费/审核/删除内容的统一排除规则。
- [ ] `M08-INDEX-03` `[45m]` 实现作者逐帖 `search_index_opt_out` 与 `ai_summary_opt_out`，管理员全站/板块策略优先。
- [ ] `M08-INDEX-04` `[45m]` 内容发布、编辑、隐藏、恢复、删除和策略变更触发幂等索引 Job。
- [ ] `M08-INDEX-05` `[30m]` 重建索引时按当前权限和策略重新生成，旧 revision 不能覆盖新 revision。
- [ ] `M08-INDEX-06` `[45m]` 搜索限制查询长度、语法、结果数、分页深度、匿名频率和高亮长度。
- [ ] `M08-INDEX-07` `[45m]` 搜索结果返回前重新执行实时可见性、处罚和索引退出判断。
- [ ] `M08-INDEX-08` `[45m]` 用隐藏正文 canary 验证索引、excerpt、highlight、相关内容和错误均不泄漏。
- [ ] `M08-INDEX-09` `[30m]` 更新 Search operation coverage、索引版本和失败/堆积指标。

## M08-FEEDS：RSS/Atom、sitemap 和 SEO

**元数据：** `P1` · `owner=unassigned/backend-public-web` · `risk=high` · `depends=M08-INDEX` · `blocked=none`
**目标文件：** `backend/src/feeds/`、`frontend/src/routes/`、`backend/tests/feeds/`、`docs/FRONTEND.md`
**验收：** RSS/Atom、sitemap、OpenGraph、JSON-LD 和 canonical 只包含安全公开文章。

- [ ] `M08-FEEDS-01` `[45m]` 实现 RSS feed，使用稳定 cursor/发布时间排序和明确的缓存/ETag 策略。
- [ ] `M08-FEEDS-02` `[45m]` 实现 Atom feed，字段、链接、更新时间和 XML escaping 通过 Fixture。
- [ ] `M08-FEEDS-03` `[30m]` sitemap 只列入允许索引的公开 canonical URL，限制数量并支持分页/分片。
- [ ] `M08-FEEDS-04` `[30m]` 动态生成 robots.txt、`X-Robots-Tag` 和 meta noindex；声明不替代服务端边界。
- [ ] `M08-FEEDS-05` `[45m]` OpenGraph、JSON-LD、canonical、摘要和图片投影重新执行可见性/退出索引策略。
- [ ] `M08-FEEDS-06` `[30m]` Feed/SEO 缓存按策略 revision、内容 revision 和公开投影维度隔离。
- [ ] `M08-FEEDS-07` `[45m]` 测试隐藏、回复、等级、付费、审核、删除和封禁内容不会进入任何投影。
- [ ] `M08-FEEDS-08` `[30m]` 无 JavaScript 访问公开文章、Feed 链接和 canonical 仍合理可用。
- [ ] `M08-FEEDS-09` `[30m]` 更新 Feeds/Search operation coverage 和响应头 Fixture。

## M08-CRAWL：行为检测与分级响应

**元数据：** `P0` · `owner=unassigned/security-edge` · `risk=critical` · `depends=M08-INDEX,M00-BACKEND` · `blocked=none`
**目标文件：** `backend/src/antibot/`、`backend/src/middleware/rate_limit*`、`backend/tests/antibot/`、`docs/CRAWLER-POLICY.md`
**验收：** 观察/降速→429→挑战→临时封禁→人工复核状态机和误伤回退通过。

- [ ] `M08-CRAWL-01` `[45m]` 定义账号、可信代理 IP、IP 段、UA、顺序扫描、并发、分页深度和失败率的行为信号。
- [ ] `M08-CRAWL-02` `[30m]` 正确解析可信代理链；不信任客户端伪造的 `X-Forwarded-For`。
- [ ] `M08-CRAWL-03` `[45m]` 按匿名/登录/搜索/RSS/sitemap/公开文章/管理分别建立限流桶。
- [ ] `M08-CRAWL-04` `[45m]` 实现 observe/throttle 状态，增加延迟但不改变安全授权和内容结果。
- [ ] `M08-CRAWL-05` `[30m]` 实现 429、Retry-After、响应头和稳定 Problem code。
- [ ] `M08-CRAWL-06` `[45m]` 实现挑战状态、一次性 token、过期、失败次数和无障碍替代路径。
- [ ] `M08-CRAWL-07` `[30m]` 实现临时封禁、到期和人工复核状态；封禁写审计但不泄漏检测规则。
- [ ] `M08-CRAWL-08` `[30m]` 默认拒绝 GPTBot、CCBot、Google-Extended、ClaudeBot 等 AI 训练爬虫。
- [ ] `M08-CRAWL-09` `[30m]` 普通搜索引擎只索引明确允许的公开内容，robots 与 HTTP 授权同时执行。
- [ ] `M08-CRAWL-10` `[45m]` 测试伪造 UA、代理头、未知机器人、慢速爬虫、并发扫描、失败重试和误伤恢复。
- [ ] `M08-CRAWL-11` `[45m]` 测试缓存、304、Feed、sitemap、OpenGraph、JSON-LD 和公开文章的权限维度隔离。
- [ ] `M08-CRAWL-12` `[30m]` 建立反爬告警、人工复核查询和隐私最小化日志。

## M08-UI：搜索、公开 SEO 与隐私设置

**元数据：** `P1` · `owner=unassigned/frontend-public` · `risk=medium` · `depends=M08-FEEDS,M08-CRAWL` · `blocked=none`
**目标文件：** `frontend/src/routes/search/`、`frontend/src/routes/settings/privacy/`、`frontend/tests/`
**验收：** 公开搜索、搜索引擎预览、退出索引设置和挑战流程 E2E/a11y 通过。

- [ ] `M08-UI-01` `[45m]` 实现公开搜索 SSR、查询校验、cursor、空状态和 429/挑战恢复。
- [ ] `M08-UI-02` `[30m]` 搜索结果只渲染后端安全摘要和高亮，不在客户端重新拼接隐藏正文。
- [ ] `M08-UI-03` `[30m]` 实现作者逐帖退出搜索/AI 摘要设置，展示管理员策略优先级。
- [ ] `M08-UI-04` `[30m]` 实现 robots/索引状态说明，不承诺 robots 能阻止恶意抓取。
- [ ] `M08-UI-05` `[45m]` 测试 Feed、canonical、OG/JSON-LD 的页面源和无 JS 浏览。
- [ ] `M08-UI-06` `[30m]` 测试挑战键盘、屏幕阅读器、移动端和失败后安全回退。

---

<a id="m9"></a>

# M9：AI Gateway、同意、建议与任务

**完成定义：** AI 默认关闭；正文每次外发前明确同意；Gateway 防 SSRF/泄漏；AI 只产生版本化建议，不能自动处罚或改变权限。

## M09-SCHEMA：Consent、Provider、Task 与 Suggestion

**元数据：** `P0` · `owner=unassigned/backend-ai-db` · `risk=critical` · `depends=M01-JOBS,M04-VISIBILITY` · `blocked=none`
**目标文件：** `migrations/*/`、`backend/src/ai/model*`、`docs/AI.md`
**验收：** 三数据库状态、同意版本、任务幂等和旧 revision 隔离通过。

- [ ] `M09-SCHEMA-01` `[45m]` 新增 ai_providers、provider policies、purpose/model budget 和 Feature Flag 迁移。
- [ ] `M09-SCHEMA-02` `[45m]` 新增 ai_consents，保存 purpose、provider、notice version、text hash、scope、granted/revoked_at 和审计引用。
- [ ] `M09-SCHEMA-03` `[45m]` 新增 ai_tasks，覆盖 queued/running/retry_wait/succeeded/cancelled/dead 与 content revision。
- [ ] `M09-SCHEMA-04` `[45m]` 新增 suggestions，覆盖 formatting/SEO/tagging/moderation、schema version、base revision 和 decision。
- [ ] `M09-SCHEMA-05` `[30m]` 建立旧 revision/旧 policy 结果不能覆盖新内容的唯一/版本约束。
- [ ] `M09-SCHEMA-06` `[30m]` 同步 AI 状态机、事件目录、错误码、OpenAPI 和隐私文档。

## M09-GATEWAY：Provider allowlist 与数据边界

**元数据：** `P0` · `owner=unassigned/security-ai` · `risk=critical` · `depends=M09-SCHEMA,M01-CONFIG` · `blocked=none`
**目标文件：** `backend/src/ai/gateway/`、`backend/src/net/egress/`、`backend/tests/ai/security*`
**验收：** SSRF/DNS rebinding、TLS、重定向、预算、Secret 和内容投影测试通过。

- [ ] `M09-GATEWAY-01` `[45m]` 浏览器不能直连 Provider；所有请求经过 Rust Gateway 和用途策略。
- [ ] `M09-GATEWAY-02` `[45m]` Provider endpoint 使用 HTTPS、host allowlist、端口限制和证书校验。
- [ ] `M09-GATEWAY-03` `[45m]` 实现解析前后 IP 校验、DNS rebinding 防护、私网/回环/链路本地阻断。
- [ ] `M09-GATEWAY-04` `[30m]` 限制重定向次数、目标 allowlist、连接/读取/总超时和响应大小。
- [ ] `M09-GATEWAY-05` `[30m]` Secret 仅由 Gateway 读取，API/SSR/日志/错误/审计 metadata 全部脱敏。
- [ ] `M09-GATEWAY-06` `[45m]` 实现用途、模型、并发、token/金额预算、速率和熔断策略。
- [ ] `M09-GATEWAY-07` `[30m]` 默认脱敏；隐藏正文、私密审核备注、邮箱、Session 和 Secret 永不外发。
- [ ] `M09-GATEWAY-08` `[45m]` 用户正文外发前展示 Provider、用途、留存、训练、区域和数据模式并获取逐次确认。
- [ ] `M09-GATEWAY-09` `[30m]` 同意撤回后取消未发出任务，已返回的迟到结果不得写入或被自动采纳。
- [ ] `M09-GATEWAY-10` `[45m]` 测试 Prompt injection、模型输出注入、URL/SQL/模板注入、越权上下文和大响应。

## M09-TASKS：AI 异步任务与故障

**元数据：** `P0` · `owner=unassigned/backend-ai` · `risk=critical` · `depends=M09-GATEWAY,M01-JOBS` · `blocked=none`
**目标文件：** `backend/src/ai/tasks/`、`backend/tests/ai/tasks*`、`docs/JOBS.md`
**验收：** 429/4xx/5xx、熔断、取消、重试、迟到输出和至少一次消费测试通过。

- [ ] `M09-TASKS-01` `[30m]` 为 formatting/SEO/tagging/moderation 建立明确任务命令和输入投影。
- [ ] `M09-TASKS-02` `[45m]` 实现任务入队幂等、consent snapshot、content revision snapshot 和 budget reservation。
- [ ] `M09-TASKS-03` `[30m]` 实现取消，取消后 Provider 迟到响应只能进入丢弃/诊断路径。
- [ ] `M09-TASKS-04` `[45m]` 分类 Provider 429/4xx/5xx、超时、网络错误和 schema 错误，按策略重试或 dead。
- [ ] `M09-TASKS-05` `[45m]` 实现至少一次 worker 消费去重，不重复扣预算、不重复生成建议。
- [ ] `M09-TASKS-06` `[30m]` 任务执行前重新确认当前内容 revision、policy、consent 和账号状态。
- [ ] `M09-TASKS-07` `[30m]` 任务失败不能阻塞普通发帖、人工审核和核心阅读。
- [ ] `M09-TASKS-08` `[30m]` 暴露任务延迟、预算、Provider 错误、熔断、取消和 dead 指标。

## M09-SUGGESTIONS：建议、预览与采纳

**元数据：** `P1` · `owner=unassigned/backend-ai` · `risk=high` · `depends=M09-TASKS,M04-MARKDOWN` · `blocked=none`
**目标文件：** `backend/src/ai/suggestions/`、`backend/src/routes/ai/`、`backend/tests/ai/suggestions*`
**验收：** Suggestion 版本、diff、If-Match、重新鉴权和人工采纳流程通过。

- [ ] `M09-SUGGESTIONS-01` `[45m]` 解析模型输出 schema，拒绝不符合结构、超限和混入 HTML/脚本的建议。
- [ ] `M09-SUGGESTIONS-02` `[45m]` formatting 建议生成 Markdown diff 预览，不直接改写正文。
- [ ] `M09-SUGGESTIONS-03` `[30m]` SEO 建议只修改允许的标题/摘要/slug 字段，不改变公开状态或权限。
- [ ] `M09-SUGGESTIONS-04` `[30m]` moderation 建议只显示给授权审核人员，不对作者暴露内部信号。
- [ ] `M09-SUGGESTIONS-05` `[45m]` 采纳时重新鉴权、校验 base_version/If-Match、Markdown 安全和内容策略。
- [ ] `M09-SUGGESTIONS-06` `[30m]` 采纳写 revision、actor、suggestion version、consent 和审计；重复采纳幂等。
- [ ] `M09-SUGGESTIONS-07` `[45m]` 测试旧 revision、撤回同意、迟到建议、越权采纳和模型输出事实篡改。
- [ ] `M09-SUGGESTIONS-08` `[30m]` 更新 AI operation coverage、前端类型、故障 Runbook 和 Feature Flag 门槛。

## M09-UI：AI 设置与建议界面

**元数据：** `P1` · `owner=unassigned/frontend-ai` · `risk=high` · `depends=M09-GATEWAY,M09-SUGGESTIONS` · `blocked=none`
**目标文件：** `frontend/src/routes/ai/`、`frontend/src/routes/editor/`、`frontend/src/routes/admin/ai/`、`frontend/tests/`
**验收：** 每次同意、撤回、diff 采纳、失败恢复和无 JS 普通发帖流程通过。

- [ ] `M09-UI-01` `[30m]` 实现 AI 能力/默认关闭/Provider 脱敏状态页面。
- [ ] `M09-UI-02` `[45m]` 在每次正文外发前展示完整同意信息和明确确认控件。
- [ ] `M09-UI-03` `[30m]` 实现同意版本、撤回、处理中和取消状态。
- [ ] `M09-UI-04` `[45m]` 实现格式化/SEO diff 预览、字段级采纳和版本冲突恢复。
- [ ] `M09-UI-05` `[30m]` 审核员查看 moderation suggestion 时隐藏内部 Prompt/举报信息边界。
- [ ] `M09-UI-06` `[45m]` 管理后台实现 Provider、预算、任务重试/取消和 Flag 配置，要求审计。
- [ ] `M09-UI-07` `[30m]` AI 故障、关闭或撤回时普通发帖和人工审核仍能无 JS 完成。

---

<a id="m10"></a>

# M10：Direct/HLS/Xigua 视频插件

**完成定义：** Provider Adapter 受限；URL resolve 不信任客户端；HLS/SSRF/CSP/版权边界通过后逐 Provider 开启。

## M10-VIDEO：核心服务与安全解析

**元数据：** `P0` · `owner=unassigned/backend-video-security` · `risk=critical` · `depends=M04-MARKDOWN,M01-CONFIG` · `blocked=none`
**目标文件：** `backend/src/video/`、`backend/src/routes/video/`、`backend/tests/video/`、`docs/VIDEO-PLUGIN.md`
**验收：** SSRF Corpus、HLS Corpus、CSP、外链降级和版权阻断通过。

- [ ] `M10-VIDEO-01` `[45m]` 定义 Video Service 与 Direct/HLS/Xigua Adapter trait，领域层不依赖具体 Provider SDK。
- [ ] `M10-VIDEO-02` `[30m]` 支持 MP4/WebM/OGV/MOV、HLS `.m3u8` 和西瓜公开页面 URL 的解析分类。
- [ ] `M10-VIDEO-03` `[30m]` resolve 只返回短效 resolution_id；创建不接受可信 MIME、iframe HTML、Key 或签名 URL。
- [ ] `M10-VIDEO-04` `[45m]` 精确限制 source scheme/host/port、重定向、DNS、私网 IPv4/IPv6、userinfo、Unicode/IDN。
- [ ] `M10-VIDEO-05` `[45m]` HLS 解析限制 playlist 深度、分片数量、总字节/时长、Key/Map、跨域和签名泄漏。
- [ ] `M10-VIDEO-06` `[30m]` Xigua 只允许官方公开页面/嵌入 Host，拒绝抓取、转存、破解和绕过鉴权。
- [ ] `M10-VIDEO-07` `[30m]` 生成动态 CSP frame-src/media-src，限制 sandbox、referrerpolicy、autoplay、camera/mic。
- [ ] `M10-VIDEO-08` `[45m]` 实现 pending/ready/blocked/error/removed 和异步 refresh 状态机。
- [ ] `M10-VIDEO-09` `[45m]` Provider 故障、下架、限流和无嵌入权限时降级官方外链卡片，不阻塞发帖。
- [ ] `M10-VIDEO-10` `[45m]` 测试 MIME 欺骗、Range、超时、超大响应、开放重定向、DNS rebinding 和 HLS 爆量。
- [ ] `M10-VIDEO-11` `[30m]` 测试帖子权限、审核状态、历史引用重新检查、重复 resolve 和无 JS 降级。
- [ ] `M10-VIDEO-12` `[30m]` 更新 Video operation coverage、Provider policy、指标和专项开启 Runbook。

## M10-UI：视频编辑与播放投影

**元数据：** `P1` · `owner=unassigned/frontend-video` · `risk=high` · `depends=M10-VIDEO,M04-VISIBILITY` · `blocked=none`
**目标文件：** `frontend/src/lib/video/`、`frontend/src/routes/editor/`、`frontend/src/routes/admin/video/`、`frontend/tests/`
**验收：** 直链/HLS/Xigua 公开/受限/阻断/降级状态及 a11y 通过。

- [ ] `M10-UI-01` `[45m]` 实现手动 URL 输入、resolve 预览、Provider 状态和错误说明。
- [ ] `M10-UI-02` `[30m]` 只提交 resolution_id 和允许字段，不向浏览器暴露 Provider Secret/Key。
- [ ] `M10-UI-03` `[45m]` 实现安全 video/iframe 投影、CSP、sandbox 和 fallback 外链。
- [ ] `M10-UI-04` `[30m]` 隐藏/审核/删除/封禁内容不渲染视频 URL 或播放器配置。
- [ ] `M10-UI-05` `[30m]` 无 JS 显示安全外链；减少动效、键盘控制和移动端比例通过测试。
- [ ] `M10-UI-06` `[30m]` 管理后台逐 Provider 配置、测试、停用和审计展示。

---

<a id="m11"></a>

# M11：OIDC Provider

**完成定义：** 默认关闭；Authorization Code + PKCE S256、精确 redirect、RS256 ID Token、opaque Access Token、Refresh rotation、consent 和密钥恢复全部通过专项门槛。

## M11-OIDC-SCHEMA：客户端、同意与 Token 数据模型

**元数据：** `P0` · `owner=unassigned/identity-oidc` · `risk=critical` · `depends=M02-MFA,M01-CONFIG` · `blocked=none`
**目标文件：** `migrations/*/`、`backend/src/oidc/model*`、`docs/AUTH-OIDC.md`
**验收：** Public/Confidential、scope、code、token family、interaction 和 key metadata 三数据库迁移通过。

- [ ] `M11-OIDC-SCHEMA-01` `[45m]` 新增 oauth_clients、redirect URIs、post logout URIs、scopes 和 client status。
- [ ] `M11-OIDC-SCHEMA-02` `[45m]` 新增 consents、authorization requests/interactions、one-time codes 和 request hash。
- [ ] `M11-OIDC-SCHEMA-03` `[45m]` 新增 opaque access tokens、refresh families、revocation reason 和 usage timestamps。
- [ ] `M11-OIDC-SCHEMA-04` `[45m]` 新增 encrypted signing keys、JWKS revision、active/retiring 状态和 key audit。
- [ ] `M11-OIDC-SCHEMA-05` `[30m]` 所有高熵 code/token 只存 hash；scope、redirect 和 client type 使用封闭枚举/约束。
- [ ] `M11-OIDC-SCHEMA-06` `[30m]` 同步 OIDC 文档、状态机、错误码、事件目录和恢复 Runbook。

## M11-PROTOCOL：OIDC 协议端点

**元数据：** `P0` · `owner=unassigned/identity-oidc` · `risk=critical` · `depends=M11-OIDC-SCHEMA,M02-SESSION` · `blocked=none`
**目标文件：** `backend/src/oidc/`、`backend/src/routes/oauth/`、`backend/tests/oidc/`
**验收：** discovery/authorize/token/userinfo/JWKS/revoke/logout 的协议错误和安全语义通过。

- [ ] `M11-PROTOCOL-01` `[30m]` 实现 discovery、issuer、端点能力和精确 JWKS cache headers。
- [ ] `M11-PROTOCOL-02` `[45m]` 实现 Authorization Code + PKCE S256；拒绝 implicit、plain、password 和 device flow。
- [ ] `M11-PROTOCOL-03` `[45m]` 精确匹配 redirect/post-logout URI，除明确 localhost 开发例外不做通配。
- [ ] `M11-PROTOCOL-04` `[45m]` 实现 state、nonce、授权码一次消费、过期、client/redirect/request hash 绑定。
- [ ] `M11-PROTOCOL-05` `[45m]` 实现 `openid/profile/email` scope、Pairwise Subject 和用户投影。
- [ ] `M11-PROTOCOL-06` `[45m]` 签发 RS256 ID Token、opaque Access Token，校验 iss/sub/aud/exp/iat/auth_time/nonce/kid。
- [ ] `M11-PROTOCOL-07` `[45m]` 实现 userinfo scope 过滤、revoke 和 logout，协议错误使用标准 OIDC 格式。
- [ ] `M11-PROTOCOL-08` `[45m]` 实现 Refresh Token Rotation；reuse 撤销整个 family 并通知用户。
- [ ] `M11-PROTOCOL-09` `[45m]` 测试 PKCE、redirect 变体、code 重放、nonce/state、scope、Token claim 和 logout。
- [ ] `M11-PROTOCOL-10` `[30m]` 默认关闭端点的正确故障行为不影响本地登录和核心论坛。

## M11-CONSENT：同意、交互、密钥和管理

**元数据：** `P0` · `owner=unassigned/security-oidc` · `risk=critical` · `depends=M11-PROTOCOL,M02-MFA` · `blocked=none`
**目标文件：** `backend/src/oidc/consent*`、`backend/src/oidc/keys*`、`backend/src/routes/admin/oauth*`、`backend/tests/oidc/security*`
**验收：** 逐 Client/逐 Scope 同意、key rotation、密钥恢复、管理员 Client 管理通过。

- [ ] `M11-CONSENT-01` `[45m]` 实现逐 Client/逐 Scope consent、重新同意、撤销和安全通知。
- [ ] `M11-CONSENT-02` `[30m]` interaction 查询与 decision 使用 Session + CSRF，并绑定原始请求摘要。
- [ ] `M11-CONSENT-03` `[45m]` 私钥加密保存；生产 ready 在 key 无法恢复时失败。
- [ ] `M11-CONSENT-04` `[45m]` 先发布新 JWKS key、再切换 active；旧 key 保留至 Token 过期加安全余量。
- [ ] `M11-CONSENT-05` `[30m]` 客户端创建/更新/停用要求管理员权限、reason、recent-auth、审计和精确 URI 校验。
- [ ] `M11-CONSENT-06` `[30m]` 普通 openid/profile/email scope 永远不具备扣款能力。
- [ ] `M11-CONSENT-07` `[45m]` 测试 Client 禁用、用户封禁、consent 撤销、key rotation 期间旧/新 Token 和 family reuse。
- [ ] `M11-CONSENT-08` `[45m]` 运行 OpenID Foundation 适用 conformance profile 并保存报告。
- [ ] `M11-CONSENT-09` `[45m]` 与至少两个独立 RP 完成登录、userinfo、refresh、logout 和错误流程集成。
- [ ] `M11-CONSENT-10` `[45m]` 执行 OIDC 私钥备份、恢复、丢失和回滚演练。
- [ ] `M11-CONSENT-11` `[30m]` 更新 OAuth/OAuth Clients operation coverage 和 Feature Flag 审批记录。

---

<a id="m12"></a>

# M12：第三方 Marketplace 与原子账务

**完成定义：** 只服务第三方应用额度；Confidential Client、逐应用/逐 Scope 审核；Checkout 原子扣款/入账；退款、Webhook、对账和紧急冻结可恢复。

## M12-SCHEMA：Client、Offer、Intent、Purchase 与双边余额

**元数据：** `P0` · `owner=unassigned/marketplace-db` · `risk=critical` · `depends=M07-LEDGER,M11-CONSENT` · `blocked=none`
**目标文件：** `migrations/*/`、`backend/src/marketplace/model*`、`docs/MARKETPLACE-ACCOUNTING.md`
**验收：** 账务恒等式、唯一约束和三数据库迁移/锁语义通过。

- [ ] `M12-SCHEMA-01` `[45m]` 新增 marketplace_clients、client redirect/terms/privacy/webhook 配置和 approval history。
- [ ] `M12-SCHEMA-02` `[45m]` 新增 client_scopes、scope approvals、limits、versions 和 emergency status。
- [ ] `M12-SCHEMA-03` `[45m]` 新增 offers、offer versions、currency、amount、inventory 和 merchant order reference。
- [ ] `M12-SCHEMA-04` `[45m]` 新增 checkout_intents、purchases、refunds、webhook deliveries 和 reconciliation records。
- [ ] `M12-SCHEMA-05` `[45m]` 新增 merchant available/pending/frozen balance，与 point operations 建立不可变关联。
- [ ] `M12-SCHEMA-06` `[30m]` 建立 client+merchant_order、intent、purchase、refund 累计和 webhook event 唯一约束。
- [ ] `M12-SCHEMA-07` `[45m]` 测试余额不得为负、历史流水不可变、库存/退款并发和三数据库锁顺序。
- [ ] `M12-SCHEMA-08` `[30m]` 同步 Marketplace、Accounting、Schema、Events、错误码和 API coverage。

## M12-CLIENTS：Client、Scope 与 Offer 管理

**元数据：** `P0` · `owner=unassigned/marketplace-security` · `risk=critical` · `depends=M12-SCHEMA,M03-AUTHZ` · `blocked=none`
**目标文件：** `backend/src/marketplace/clients/`、`backend/src/routes/admin/marketplace/`、`backend/tests/marketplace/clients*`
**验收：** 只有管理员批准的 Confidential Client 和精确 scope 可进入 checkout。

- [ ] `M12-CLIENTS-01` `[30m]` 拒绝 Public Client 接入 Marketplace，secret 只存 hash/外部 Secret ref。
- [ ] `M12-CLIENTS-02` `[45m]` 精确校验 HTTPS redirect、terms、privacy、webhook URL，阻断 SSRF/开放重定向。
- [ ] `M12-CLIENTS-03` `[45m]` 实现逐应用、逐 Scope 审核、版本、限额和生效时间。
- [ ] `M12-CLIENTS-04` `[30m]` 确保普通 OIDC scope 永远不能调用扣款接口。
- [ ] `M12-CLIENTS-05` `[45m]` Offer 金额、货币、库存、版本、平台费和收款 Client 只能由服务端登记。
- [ ] `M12-CLIENTS-06` `[30m]` merchant balance 只能站内消费，不提现、不现金兑换、不转普通用户。
- [ ] `M12-CLIENTS-07` `[30m]` 实现 Client/Scope 紧急停用，立即阻止新 Intent/confirm，历史交易可查询。
- [ ] `M12-CLIENTS-08` `[45m]` 测试 Client 禁用、scope 撤销、Offer 旧版本、超限和 URL 变体。
- [ ] `M12-CLIENTS-09` `[30m]` 更新管理端 operation coverage、审计和审批证据。

## M12-CHECKOUT：user-bound Intent 与原子购买

**元数据：** `P0` · `owner=unassigned/marketplace-accounting` · `risk=critical` · `depends=M12-CLIENTS,M07-LEDGER` · `blocked=none`
**目标文件：** `backend/src/marketplace/checkout/`、`backend/src/routes/marketplace/`、`backend/tests/marketplace/checkout*`
**验收：** user-bound、准确金额、锁序、幂等、提交后响应和故障注入全部通过。

- [ ] `M12-CHECKOUT-01` `[45m]` 创建短 TTL Checkout Intent，绑定 client、user、offer version、amount、currency、order ref 和 request hash。
- [ ] `M12-CHECKOUT-02` `[30m]` 只接受 user-bound Access Token；拒绝请求体 user_id、amount、currency、merchant 或 balance。
- [ ] `M12-CHECKOUT-03` `[45m]` 托管确认页展示市场、商品、数量、准确金额、余额变化和授权期限。
- [ ] `M12-CHECKOUT-04` `[45m]` confirm 重新读取 Client、Scope、Offer、库存、用户状态、限额和 Intent expiry。
- [ ] `M12-CHECKOUT-05` `[45m]` 固定锁序，原子锁库存、扣买方、入 merchant pending、写 Purchase/流水/审计/Outbox。
- [ ] `M12-CHECKOUT-06` `[30m]` 成功响应只在数据库提交后返回；提交前断连只能得到已提交或明确不存在。
- [ ] `M12-CHECKOUT-07` `[30m]` 同一 Idempotency-Key/摘要返回原结果，不同摘要返回 409。
- [ ] `M12-CHECKOUT-08` `[45m]` SQLite `BEGIN IMMEDIATE`、MySQL/MariaDB 行锁竞争和固定锁顺序通过。
- [ ] `M12-CHECKOUT-09` `[45m]` 注入余额、库存、流水、Outbox、审计和提交各步骤失败，验证完全回滚。
- [ ] `M12-CHECKOUT-10` `[30m]` 测试 IDOR、CSRF、过期/已消费 Intent、用户封禁、价格篡改和限额。

## M12-REFUND：退款、Webhook、对账与补偿

**元数据：** `P0` · `owner=unassigned/marketplace-accounting` · `risk=critical` · `depends=M12-CHECKOUT,M01-JOBS` · `blocked=none`
**目标文件：** `backend/src/marketplace/refunds/`、`backend/src/marketplace/webhooks/`、`backend/src/marketplace/reconcile/`、`backend/tests/marketplace/`
**验收：** reversal、重复/乱序 Webhook、对账、紧急冻结和恢复演练通过。

- [ ] `M12-REFUND-01` `[30m]` 退款只引用原 Purchase 写 reversal operation，不修改/删除原订单和流水。
- [ ] `M12-REFUND-02` `[45m]` 锁定原 Purchase，累计退款不得超过原金额，并发退款幂等。
- [ ] `M12-REFUND-03` `[30m]` Client 只能退自己的交易；管理员退款要求 recent-auth、reason、限额和审计。
- [ ] `M12-REFUND-04` `[45m]` merchant pending/available 余额结算、冻结和退款补偿保持双边恒等式。
- [ ] `M12-REFUND-05` `[45m]` Webhook 只由提交后 Outbox 投递，使用签名版本、时间窗、重放保护和最小 payload。
- [ ] `M12-REFUND-06` `[30m]` 处理 Webhook 延迟、重复、乱序、4xx/5xx、轮换和 dead-letter，不改变已提交购买结果。
- [ ] `M12-REFUND-07` `[45m]` 实现增量对账、差异分类、人工复核、重放和修复补偿。
- [ ] `M12-REFUND-08` `[45m]` 测试账本恒等式、余额/库存泄漏、Webhook HMAC、SSR​​F、时间窗和紧急冻结。
- [ ] `M12-REFUND-09` `[30m]` 更新 Marketplace operation coverage、对账告警和停用 Runbook。

## M12-UI：Marketplace 授权、交易与管理

**元数据：** `P1` · `owner=unassigned/frontend-marketplace` · `risk=high` · `depends=M12-CHECKOUT,M12-REFUND` · `blocked=none`
**目标文件：** `frontend/src/routes/marketplace/`、`frontend/src/routes/admin/marketplace/`、`frontend/tests/`
**验收：** 确认页、结果、退款、Client 审批和紧急停用 UI 的 E2E/a11y 通过。

- [ ] `M12-UI-01` `[45m]` 实现托管 checkout 确认页，不把价格、用户或余额作为隐藏可篡改字段提交。
- [ ] `M12-UI-02` `[30m]` 显示准确金额、货币、商户、商品版本、余额变化、Scope 和有效期。
- [ ] `M12-UI-03` `[45m]` 实现成功/失败/处理中/重复请求/过期 Intent 和 request ID 状态。
- [ ] `M12-UI-04` `[30m]` 实现用户 Purchase 查询和可用退款入口，隐藏其他 Client 交易。
- [ ] `M12-UI-05` `[45m]` 管理员实现 Client/Scope/Offer、限额、余额、Webhook、对账和紧急停用页面。
- [ ] `M12-UI-06` `[30m]` 高风险设置和退款强制 reason、recent-auth、确认和审计结果展示。
- [ ] `M12-UI-07` `[45m]` 测试键盘、无 JS 托管表单、移动端、权限越界和敏感账务字段脱敏。

---

## M8-M12 出口门槛

- 公开搜索、RSS/Atom、sitemap、OG/JSON-LD 和缓存均不泄漏受限内容；robots 不是唯一防线。
- 反爬分级状态机和人工复核可追踪，AI 训练爬虫默认拒绝。
- AI 每次正文外发有同意证据；Provider 安全、任务故障、迟到结果和 Suggestion 采纳全部通过。
- Video Direct/HLS/Xigua 各自通过 SSRF、HLS、CSP、版权和降级门槛后才可开 Flag。
- OIDC conformance、两个独立 RP、Refresh reuse、key rotation 和密钥恢复全部完成后才开启。
- Marketplace 仅允许审核通过的 Confidential Client；原子双边账务、退款、Webhook 对账和紧急冻结全部通过后逐应用开启。
