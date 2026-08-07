# BBLBB — 搜索索引、AI 爬虫与批量访问策略

> 版本：v0.5 需求冻结候选
> 适用范围：公开页面、搜索、RSS/Atom、sitemap、AI Provider 和疑似批量访问。
> 事实来源：[`PRODUCT-DECISIONS.md`](PRODUCT-DECISIONS.md)；实现必须同时满足 [`SECURITY.md`](SECURITY.md) 与 [`TESTING.md`](TESTING.md)。

## 1. 策略原则

BBLBB 区分“允许索引的公开内容”和“受保护的访问控制”：

- robots.txt、页面 meta 和 HTTP 头是爬虫声明层，不是安全边界。
- Rust 后端的 Session、对象级授权、内容可见性过滤、速率限制和行为检测才是安全边界。
- 默认拒绝 AI 训练爬虫；普通搜索引擎可以按站点和内容策略索引允许公开的内容。
- 管理员关闭策略优先于作者允许，隐藏或受限内容永远不得通过任何衍生渠道泄漏。

## 2. 爬虫分类与默认策略

机器人名称采用配置化名单，不把 User-Agent 当作唯一可信依据。初始配置至少覆盖：

| 类别 | 示例 | 默认策略 |
|---|---|---|
| 普通搜索引擎 | Googlebot、Bingbot | 仅允许索引明确允许的公开页面 |
| AI 训练/抓取 | GPTBot、CCBot、Google-Extended、ClaudeBot | 默认拒绝或返回受限响应 |
| 社交预览 | 受支持的分享抓取器 | 仅允许公开 OpenGraph 投影，禁止受限正文 |
| 未知/伪装机器人 | 任意 UA | 按普通访问处理，并参与行为风控 |

机器人名单、策略、例外和变更原因必须可在后台配置并写审计。robots 生成器应根据当前配置输出 `Disallow`，但不能据此跳过服务端鉴权。

## 3. 页面和衍生数据过滤

服务端在生成每一种公开投影前重新执行授权和索引策略：

- 未明确允许索引的页面输出 `noindex, nofollow, noarchive`，并可附加 `X-Robots-Tag`。
- 公开 sitemap 只包含当前可公开、未删除、未封禁且作者/管理员允许索引的 URL。
- RSS/Atom、搜索结果、OpenGraph、JSON-LD、摘要、推荐和缓存不得包含隐藏正文、审核中内容或受限资源。
- 作者可以按帖子/文章退出搜索索引和 AI 摘要；管理员可以按全站或板块强制关闭。
- 附件不因出现在公开页面就变成永久公开对象；下载仍需经过授权并使用短效 URL。
- 公开页面缓存键必须包含影响可见性的策略维度，不能把登录后或付费内容缓存给匿名用户。

## 4. 批量访问分级处置

风控信号至少包括账号、可信代理解析后的 IP、IP 段、UA 一致性、请求频率、并发、顺序扫描、分页深度、搜索窗口、失败率、Cookie/Session 行为和资源类型。不能只依赖单 IP 或 User-Agent。

默认处置阶梯：

1. 记录并观察；
2. 降低响应速率或提高分页间隔；
3. 返回 `429` 并带合理 `Retry-After`；
4. 要求挑战或重新验证；
5. 临时封禁并撤销相关 Session/Token；
6. 高风险、误报争议和升级案件进入人工复核。

降级与恢复必须可审计、有限时、可解释；健康检查、已认证正常用户和受信任内部任务不得被误判为公开爬虫。挑战不能绕过内容授权。

## 5. 网络和代理边界

- 只有 Caddy/CDN/WAF 明确配置的可信代理可以提供客户端 IP；Rust 不信任任意请求头中的 `X-Forwarded-For`。
- CDN/WAF 是预留增强层，不是单机部署的必要依赖；启用后必须隔离匿名公开缓存与个性化响应。
- 搜索、RSS、sitemap 和公开文章使用独立限流桶，避免一个接口耗尽全站预算。
- 管理员关闭 AI 爬虫策略后，robots、meta、响应头、搜索和 AI 摘要投影必须在配置传播窗口内一致更新。

## 6. 验收要求

正式启用搜索、RSS、AI 或公开市场前，至少通过：

- 匿名、登录、回复、等级和付费可见性的投影泄漏测试；
- robots、meta、`X-Robots-Tag`、sitemap、RSS/Atom、OpenGraph、JSON-LD 一致性测试；
- 伪造 User-Agent、代理头、轮换 IP、并发分页和失败重试的行为检测测试；
- `降速 → 429 → 挑战 → 临时封禁 → 人工复核` 状态机及恢复测试；
- 管理员全站/板块关闭与作者逐帖退出的优先级测试；
- 多节点或 CDN 缓存启用时的缓存隔离测试。

## 7. Feed/SEO 投影与缓存（M08-FEEDS）

实现位置：`backend/src/feeds/`（projection/render/robots/cache）+ 
`backend/src/routes/feeds.rs`。

### 7.1 RSS/Atom/sitemap

- **RSS 2.0**（`GET /api/v1/rss`）：`published_at DESC, id DESC` 稳定排序 +
  keyset cursor（`base64url("published_at|id")`）；文本/属性统一 XML 转义
  （`& < > " '`）；每条含 guid/link/pubDate/description/author。
- **Atom 1.0**（`GET /api/v1/atom`）：feed/entry 的 id/link/updated/published/
  summary/author 全字段 + 更新时间（RFC 3339）。
- **sitemap**（`GET /api/v1/sitemap.xml`）：只列入**允许索引**的公开 canonical
  URL；总量超过单页上限（默认 500，钳制 100..=5000）返回 `<sitemapindex>`
  分片导航；越界分片返回空 urlset（不枚举总量）。
- 三个通道加载时**重新执行可见性/退出索引策略**：`status='published'` 且未
  删除、非审核中、有效访问策略 public、板块启用且 public、作者 active、作者
  逐帖未退出、管理员全站/板块未 deny。隐藏/回复/等级/付费/审核/删除/封禁/
  退出内容绝不进入任何 Feed 投影。

### 7.2 robots / X-Robots-Tag / meta noindex

- 动态 `robots.txt`（`GET /robots.txt`）：默认允许公开路径，`/api/`、`/admin/`、
  `/search`、revisions 一律 Disallow；**AI 训练爬虫（GPTBot/CCBot/
  Google-Extended/ClaudeBot/PerplexityBot）默认拒绝**。
- Feed/sitemap 响应携带 `X-Robots-Tag: noindex, nofollow, noarchive`。
- `meta name="robots"` 决策与 `X-Robots-Tag` 同源（`feeds::robots`），由
  公开投影的 `index_allowed` 决定；robots 只是爬虫声明层，**不替代服务端
  鉴权/授权/限流/行为风控**。

### 7.3 OpenGraph/JSON-LD/canonical/摘要/图片

`load_seo_post` 对单帖重跑可见性/退出索引策略（不可见 → 无投影）；`seo_meta_for`
组装 canonical、`og:*`、`Article`/`DiscussionForumPosting` JSON-LD、摘要
（安全 excerpt）与封面图片（`/api/v1/attachments/{id}/content` 稳定内容端点，
只投影 attachment id，不投影签名 URL）。`index_allowed = !作者逐帖退出 && !
管理员 deny` → 供前端输出 `noindex` meta / `X-Robots-Tag`。

### 7.4 Feed/SEO 缓存隔离

`feeds::cache`：进程内有界缓存（≤128 项，TTL 60s），键 =
`(endpoint, 查询参数, policy_revision, content_revision, 投影维度)`。策略
revision（逐帖退出/管理员策略/状态/可见性变更 bump）与内容 revision（编辑
bump）任一变化都使键失效——**登录后/付费/审核中内容永远不会以陈旧键被缓存
给匿名用户**；多节点下各节点独立，正确性以 ETag/`Cache-Control`
（`public, max-age=300`）与数据库实时查询兜底。

## 8. 行为检测与分级响应实现（M08-CRAWL）

实现位置：`backend/src/antibot/`（引擎 + 中间件）+ `backend/tests/antibot.rs`
+ `backend/src/error.rs`（`with_code` 专有错误码）。

### 8.1 处置阶梯

`observe → throttle → 429 → challenge → temp ban → review`：

- **observe/throttle**：进程内固定窗口计数；剩余额度 ≤ `limit × 15%` 时对
  疑似批量请求增加 `throttle_delay_ms` 延迟——**只加延迟，不改内容与授权**。
- **429**：`rate_limited` + `Retry-After` + `Ratelimit-*` 头。
- **challenge**：403 `challenge_required`，响应头 `X-BBLBB-Challenge` 携带
  HMAC 一次性 token（含 IP/桶/过期/随机 nonce）；重试带该头验证通过后放行，
  token 一次性且过期失效；验证失败累计达阈值触发临时封禁。无路由/无 JS 依赖，
  是无障碍替代路径。
- **temp ban**：403 `temporarily_banned`（不泄漏检测规则）+ `Retry-After`；
  封禁写审计（`AuditEntry::system_action`，仅 IP/类别/原因，不存完整 UA/路径）
  并记录告警（隐私最小化：只保留 IP 段），人工复核后可 `unban`。

### 8.2 分桶与代理边界

- 桶：`anonymous / authenticated / login / search / rss / sitemap /
  public_article / admin`，独立限流，接口之间不互相耗尽预算。
- 客户端 IP 只信任可信代理链：`X-Forwarded-For` 最右跳必须是配置的可信代理
  （默认回环），否则整条头视为伪造并回退 `"unknown"`（共享桶天然限流）；
  `X-Real-IP` 仅在链一致时信任。
- `/healthz`、`/readyz`、`/api/v1/openapi.json` 豁免风控。

### 8.3 AI 训练爬虫

`GPTBot / CCBot / Google-Extended / ClaudeBot / anthropic-ai / Bytespider /
PerplexityBot / Amazonbot` 默认拒绝（403 `crawler_denied`）；名单配置化且
写审计。普通搜索引擎名单允许访问但参与行为风控；robots 与 HTTP 授权同时执行
——中间件**不改变**内容可见性与对象级授权。
