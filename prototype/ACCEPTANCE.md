# BBLBB 原型验收矩阵（单入口 Hash SPA）

> **重要声明：原型通过不等于生产上线。** 本文件只验收 prototype/index.html 单入口原型及 hash 路由、交互、视觉和无障碍。原型中的数据、登录、权限、通知、积分、支付、邮件、上传和 OAuth 可能是 Mock 或本地状态；任何通过仅表示指定原型环境中的可重复证据。未执行或无证据项目必须标记为未验证，不得声称数字全绿。

## 0. 历史 Mock 运行时证据（2026-08-29）

> **状态注记（2026-09-06 复验）**：自单入口 Hash SPA 替换（commit abb4bda）起，`prototype/index.html` 不再加载 `assets/mock-runtime.js` / `assets/mock-runtime.css`（git 历史中该入口从未引用此层；当前仅引擎的重置逻辑清理其 localStorage key）。`verify-mock-runtime.mjs` 对当前入口不可复现（干净树同样卡在 `#messages` 的 `[data-message-fail]` 超时，与 2026-09-06 转场加载条改动无关，A/B 已验证）。本节 29/29 结论为当时入口仍加载 mock 层的历史证据，不应视为当前入口证据。

- 旧版入口曾追加加载 `assets/mock-runtime.js` 与 `assets/mock-runtime.css`；该层使用 `localStorage` key `bblbb:mock-runtime:v1` 保存演示状态，并同步既有原型守卫状态。
- 旧层覆盖登录、内容发布/草稿/附件、评论解锁、付费解锁、通知已读、消息失败、MFA、下载账单、商城、收藏、账户设置、申诉、结算/API 密钥，以及 AI/视频/主题/插件/存储/市场/举报/积分/审计后台流程。
- 2026-08-29 执行 `node prototype/verify-mock-runtime.mjs`：29/29 通过、0 浏览器错误；对应生成报告已清理。当前入口不加载该层，脚本不属于当前验收命令。
- **边界**：该层不发起 API 请求、不处理真实支付/上传/MFA/邮件/OAuth；所有“成功”均为可演示的本地 Mock 状态。

## 1. 验收边界与证据

- 入口是 prototype/index.html；通过 location.hash 在同一 HTML 文档切换页面，不是 /topics/201 等真实多页 URL。
- **页面拆分（2026-09-02）**：index.html 现在只是薄壳（约 2KB：skip-link + page-chrome.js + 空的宿主 section + page-loader.js）。每个路由的页面标记都拆分在 prototype/pages/<name>.html（57 个文件，完整文档：可直接用浏览器打开预览，与全站共用 assets/*.css 与共享骨架 assets/page-chrome.js）。assets/page-loader.js 启动时拉取 home/discover/design 三个静态页并抽取 section.page 内容注入宿主，随后动态加载引擎；其余路由由引擎的 TPL()/mountTpl() 在首次访问时惰性 fetch 对应 pages/<name>.html、DOMParser 抽取 section.page 内容、缓存到 window.__PAGES 后挂载。TPL_TOKEN 保证快速切换路由时过期渲染被丢弃。
- 新运行时 prototype/assets/prototype-app.js 是唯一活动路由（挂载后负责状态同步与事件绑定）；旧版内联逻辑仅作为 template 参考，不参与执行。
- 当前实际 hash：#home、#loading、#login、#discover、#messages、#me、#thread[:key]、#search、#notifications、#shop、#achievements、#drafts、#design，以及补全后的 #articles、#boards、#board:slug、#tags、#tag:name、#topic:id、#publish、#user:name、#favorites、#settings:tab、#billing、#appeals、#mfa、#market、#checkout、#purchases、#apikeys、#admin*、#403、#404、#429、#error。
- 未匹配 hash 按当前代码进入 #404；这与 IA 的真实路径仍有差异，正式前端需使用 SvelteKit 路由。
- 未登录访问 messages、me、drafts、admin、notifications、shop、achievements 会转到 #login；演示账号只能证明本地演示状态。

| 标记 | 含义 | 规则 |
|---|---|---|
| 通过（原型） | 当前入口、视口、主题和前置状态下有可重复证据 | 不代表生产可用 |
| 修复后待复验 | 已修复但未完成完整回归 | 不得关闭 |
| 部分覆盖 | 只覆盖 IA/API 一部分 | 不得写全量通过 |
| 未验证 | 没有截图、录屏、自动化或接口证据 | 默认状态 |
| 不适用/未实现 | 当前原型没有此能力 | 记录缺口 |
| 生产阻塞 | 后端、基础设施或安全尚未满足上线条件 | 必须保留 |

每条断言记录 commit、hash、登录态、1440/1024/768/390 视口、亮暗主题、步骤、预期、实际、证据路径和时间。仅 Toast、DOM 变化或 Mock 数字不算 API 证据。

## 1. 后端领域 / operationId → 原型 hash 映射

依据 openapi/openapi.yaml operationId 与 backend/src/routes/mod.rs 领域模块。代表 operationId 非全集；全集以 OpenAPI 为准。映射表示应验收的用户入口，不表示原型已调用 API。

| 后端领域 | 代表 operationId | 原型入口/交互 | 覆盖 |
|---|---|---|---|
| health / ready | getHealth | #admin 仪表盘生产健康提示；实际 /healthz /readyz 另行验证 | 原型不覆盖；环境级 API 待验 |
| auth / mfa | login、loginMfa、logout、reAuth、getCsrfToken | #login；本地成功/失败/退出；无真实 MFA/CSRF 网络闭环 | 部分 |
| users | getMe、updateMe、getPublicUser、listSessions、revokeSession | #user:name、#me、#settings:devices | 原型已补充；API 待验 |
| boards | listBoards、getBoard、listBoardPosts | #boards、#board:slug、首页入口 | 原型已补充；API 待验 |
| posts / comments | listPosts、createPost、getPost、updatePost、listComments、createComment | #articles、#topic:id、#publish、回复编辑器 | 原型已补充；API 待验 |
| reactions / favorites | post_posts_id_reactions、post_comments_id_reactions、createFavorite、deleteFavorite | #topic:id、#favorites、楼层操作 | 原型已补充；API 待验 |
| tags / search | listTags、searchPublicContent | #tags、#tag:name、#search 本地筛选/空态 | 原型已补充；API 待验 |
| notifications | get_notifications、markAllNotificationsRead、post_notifications_id_read、getNotificationPreferences | #notifications、#settings:notifications | 原型已补充；API 待验 |
| drafts / AI | listDrafts、createDraft、updateDraft、previewDraft、post_ai_drafts_draft_id_format | #drafts、#publish、#admin-ai | 原型已补充；Mock 持久化/任务；API 待验 |
| attachments / storage / download | listAttachments、createAttachment、get_admin_storage_config、post_attachments_id_download | #publish、#billing、#admin-attachments、#admin-storage | 原型已补充；API 待验 |
| economy / shop | get_shop_products、post_shop_orders、get_me_entitlements、get_me_download_transactions | #shop、#billing、#topic:202 解锁 | 原型已补充；API 待验 |
| achievements | getAchievements、getMyAchievements | #achievements；卡片、隐藏态、装备槽 | 原型已补充；服务端事件流待验 |
| moderation | listOwnReports、post_reports、listModerationCases、updateModerationCase、decideModerationAppeal | #topic:id 举报、#admin-reports、#admin-report:R-1024、#appeals | 原型已补充；API 待验 |
| admin / admin_* | listAdminUsers、listAdminBoards、listAdminTags、listAdminRoles、listAdminSettings、listAdminAuditLogs | #admin 与 #admin-* 全量管理入口 | 原型已补充；逐 operationId API 待验 |
| admin_points / admin_levels | getAdminUserBalance、adjustAdminPoints、listAdminLevels、updateAdminLevel | #admin-points、#admin-levels | 原型已补充；API 待验 |
| admin_achievements / activity / bi | listAdminAchievements、grantAdminAchievement、listAdminActivityTasks、getAdminBiOverview | #admin-achievements、#admin-audit、#admin-bi | 原型已补充；API 待验 |
| themes | get_themes_active、get_me_preferences_theme、put_me_preferences_theme、get_admin_themes | #design、#settings、#admin-themes | 原型已补充；API/四视图待验 |
| OAuth / oidc | get_oauth_authorize、post_oauth_token、post_oauth_revoke、listAdminOAuthClients、createAdminOAuthClient | #apikeys、#settings:oauth、#admin-oauth；回调仅状态说明 | 部分；真实 OAuth/API 待验 |
| marketplace | post_marketplace_checkout_intents、post_marketplace_checkout_intents_id_confirm、get_marketplace_purchases | #market、#checkout、#purchases、#admin-marketplace | 原型已补充；事务/Outbox/Webhook 待验 |
| video | post_video_embeds_resolve、post_video_embeds、get_video_embeds_id_ | #publish 视频插入说明、#admin-video 白名单/CSP 状态 | 原型部分补充；真实解析/API 待验 |
| feeds / metrics | getPublicRssFeed、getPublicAtomFeed | 无可视入口 | 原型不覆盖 |

## 2. IA 路由覆盖矩阵

| IA 规格路径 | 实际单入口 hash | 状态 | 必须记录 |
|---|---|---|---|
| / | #home | 已自动验收 | 首页和响应式仍需四视图人工签核 |
| /articles | #articles | 原型已补充 | 静态 mock 列表；正式实现需接 listPosts |
| /boards、/boards/[slug] | #boards、#board:slug | 原型已补充 | 静态 mock 列表；正式实现需接 listBoards/listBoardPosts |
| /tags、/tags/[name] | #tags、#tag:name | 原型已补充 | 静态 mock 聚合；正式实现需接 listTags/searchPublicContent |
| /topics/[id] | #topic:id（兼容 #thread:key） | 原型已补充 | 101/201/202 分流；正式实现需接 getPost/listComments |
| /publish | #publish:article/topic | 原型已补充 | 校验、预览、草稿、附件配额、确认和失败提示；正式实现需接 createPost/drafts/attachments |
| /users/[name] | #user:name、#me | 原型已补充 | 多用户资料/内容/积分 tab；正式实现需接 getPublicUser/getMe |
| /login | #login | 部分自动验收 | 成功/失败/锁定实现；真实会话、MFA、CSRF 待生产验收 |
| /register | #register | 原型已补充 | 同意规则、邮箱格式、密码校验与验证邮件说明；真实 sender 待生产验收 |
| /forgot-password | #forgot-password | 原型已补充 | 邮箱校验与成功态；真实邮件投递待生产验收 |
| /search?q= | #search | 原型已补充 | 本地搜索/空态；query 非真实 URL 契约 |
| /notifications | #notifications | 原型已补充 | 筛选、全部已读、来源链接；真实分页/投递待验 |
| /favorites | #favorites | 原型已补充 | 收藏列表、移除与空态；正式实现需接 favorites API |
| /achievements | #achievements | 原型已补充 | 隐藏成就占位和装备槽；服务端事件判定待验 |
| /settings?tab= | #settings:profile/security/devices/notifications/oauth/privacy | 原型已补充 | 设备下线、通知/OAuth/隐私开关与确认；正式实现需接 users/notifications/auth sessions |
| /admin | #admin | 原型已补充 | 管理员仪表盘与 RBAC 演示；正式实现需接 admin API |
| /admin/* | #admin-* | 原型已补充 | 高保真页与最小占位页；正式实现需逐 operationId 验证 |
| /403 | #403 | 原型已补充 | 返回首页/重试状态页；管理员越权仍需切换普通成员实测 |
| 未匹配/资源 404 | #404 | 原型已补充 | 未知 hash 进入 404；正式前端由 catch-all 路由处理 |

## 3. 九条旅程验收矩阵

| # | hash 步骤 | 原型断言 | 后端补证 | 状态 |
|---|---|---|---|---|
| 1 | #home → 分类/发现 → #thread:key → 回复 | 路由可达、空回复阻止、合法回复插入、计数/受限块一致 | listBoards、listBoardPosts、getPost、createComment、鉴权、429、幂等 | 深度复测通过：本地交互可操作；完整 API 待验 |
| 2 | #home → #topic:id → 收藏 → #favorites | 收藏态、移除、空态与持久化 Mock | getPost、createFavorite/deleteFavorite、持久化 | 深度复测通过：收藏可持久化；点赞/收藏 API 待验 |
| 3 | #topic:201 受限卡 → 回复 → 解锁 | 未解锁正文不进 DOM；去回复聚焦；发表后一次解锁 | 服务端授权、事件、审计、缓存隔离 | 深度复测通过：本地交互可操作；服务端授权待验 |
| 4 | #topic:202 → 支付解锁 | 确认显示余额/扣款；取消无副作用；成功余额一致 | checkout intent/confirm、原子余额、Outbox、退款 | 深度复测通过：本地交互可操作；真实资金链路待验 |
| 5 | 头像菜单 → #me → settings | 登录门槛、资料/设备/通知/OAuth 分组和退出入口 | sessions、revoke、cookie、CSRF、审计 | 深度复测通过：设备/偏好本地状态可操作；真实安全 API 待验 |
| 6 | #admin → #admin-reports → R-1024 | 举报列表、详情、原因校验、处罚确认、审计时间线与申诉入口 | moderation cases、处罚权限、审计、申诉 | 深度复测通过：Mock 案件状态可操作；真实审核 API 待验 |
| 7 | #admin → 积分/等级 → 调整确认 | 必填、不可逆提示、取消/确认、数字更新 | balance、adjust、不可变账本、step-up | 深度复测通过：Mock 调整可操作；真实账本/step-up 待验 |
| 8 | #admin → OAuth | Secret 一次显示/复制/关闭不可查；创建与撤销确认 | client CRUD、redirect 白名单、哈希/轮换、撤销 | 深度复测通过：表单/确认可操作；真实 OAuth API 待验 |
| 9 | #admin/#design → 暗色预览/切换 → 切回 | Token、focus、对比度、刷新后策略按证据验收 | theme preference、CSP、发布/回滚 | 自动响应式回归通过；人工设计/无障碍签核待完成 |

## 4. 状态矩阵

| 状态 | 入口/触发 | 原型验收断言 | 当前口径 |
|---|---|---|---|
| 正常登录 | #home 及登录成功 | hash 切换、active、回顶、本地状态可复现 | 深度复测通过；真实会话/MFA/CSRF 待验 |
| 未登录保护 | messages/me/drafts/admin/notifications/shop/achievements 等 | 转 #login、保存 returnTo，不泄漏内容 | 深度复测通过；仅为本地 Mock 守卫 |
| 加载 | #loading、路由/提交（若存在） | 进度、按钮禁用、live 文本、不重复提交 | 加载页可达；异步接口态待验 |
| 空态 | #search、#board:slug、#favorites 等 | 固定文案、清除/返回入口 | 原型已补充并可交互；真实列表空态待 API 验收 |
| 表单错误 | #login、#publish、#topic:id、#register | 字段红字、alert/live、首错聚焦、输入保留 | 原型已补充；完整状态组合待复验 |
| 接口失败 | #publish 失败模拟、消息/上传/下载/后台操作入口 | 错误状态、保留表单、可重试；Toast 不等于网络失败 | 发布/消息/存储部分已补充；真实网络失败待 API 验收 |
| 403 | 非管理员进入 admin | #403 状态页；后台提供切换普通成员演示 | 已补充；需执行非管理员旅程 |
| 404 | 未知 hash/不存在资源 | 进入 #404，提供返回首页/重试 | 自动验收通过；真实 catch-all 路由待验 |
| 429 | 登录五次失败、回复限频 | 锁定/倒计时须实际复现 | 登录锁定逻辑有实现；倒计时/回复限流尚未完成深度复测 |
| 受限内容 | #topic:101/201/202 | 锁定时正文不在 DOM；回复/支付后渲染 Mock 投影 | 深度复测通过本地条件；服务端授权/缓存隔离待验 |
| 破坏性操作 | 退出、删除/封禁、积分、Secret、退款 | Confirm、后果说明、取消无副作用 | 深度复测通过部分 Mock 操作；真实后台 API/step-up 待验 |
| 亮暗主题 | #design/#settings | 只切变量，focus/对比度不回退 | 已有主题切换；四视图对比度待签核 |

## 5. 响应式、UI 与无障碍验收

- [x] 1440：自动路由回归无横向溢出、重叠；信息密度仍需人工视觉签核。
- [x] 1024、768：自动路由回归无横向溢出；侧栏/筛选视觉仍需人工签核。
- [x] 390：自动路由回归无横向溢出；聊天、回复、发布、Modal 热区仍需人工/无障碍签核。
- [x] 自动覆盖 1440/1024/768/390 的亮/暗路由回归且无 JS 错误；最近一次 182/182 结果由 `cd prototype && node verify.mjs` 生成，报告为本地生成物，已清理且不入库；人工视觉/无障碍签核仍未完成。
- [x] 页面转场加载条（RouteProgress，2026-09-06 新增）：顶栏下缘 fixed 2px 细线，纯黑白灰 Token（--route-progress-*：亮 #1A1A1A / 暗 #E8E8E8，轨道近透明），亮/暗主题自动适配；hash 路由开始 0→72%（500ms）、最短显示 450ms、完成补 100%（240ms）淡出（340ms）、运行中重复 start 不重置、守卫拦截 cancel 直接淡出、prefers-reduced-motion 无横向位移；触发点覆盖 SPA route() 起止、启动拉取页面片段、独立页站内跳转（跳转前 start）。证据：专项脚本 16/16 通过、0 console/page 错误（2026-09-06，1440/390 视口、亮暗双主题）；截图为本地生成物，已清理；全量回归 182/182 见上一条命令。
- [ ] 颜色、边框、阴影、圆角、字号、间距使用 UI 语义 Token；组件适用状态覆盖 normal/hover/focus/disabled/loading/error。
- [ ] 键盘完成九条关键路径，无陷阱；focus 顺序正确；图标按钮有 aria-label/可见文本。
- [ ] 页面标题、label、role=alert/aria-live、aria-checked、aria-selected 正确；动态状态可读。
- [ ] Modal/抽屉焦点进入内部、Esc 关闭、关闭后还原；触屏不依赖 hover。
- [ ] 对比度按 UI 规格复测：正文至少 4.5:1；品牌蓝不用于 14px 以下正文；focus 亮暗均可见。
- [ ] 屏幕阅读器抽查登录、回复、通知、主题切换和危险操作。

## 6. 安全验收矩阵

| 类别 | 原型检查 | 生产必须补齐 |
|---|---|---|
| XSS/注入 | 标题、正文、回复、搜索、URL 输入后检查 DOM；禁止未转义 HTML | 服务端校验/编码、Markdown sanitizer、CSP、日志脱敏 |
| 受限内容 | 锁定时正文不进 DOM/预加载数据 | 服务端对象授权、缓存隔离、审计 |
| 认证会话 | 未登录重定向、退出清本地演示状态 | Secure/HttpOnly/SameSite、CSRF、MFA、rotation、撤销 |
| 权限 | 不能仅凭 hash 获得管理权限 | 每个 operationId 后端 RBAC/ABAC、拒绝默认、step-up、审计 |
| 限流 | 登录/回复限频不可复现则未验证 | 服务端限流、429 Retry-After、告警 |
| 上传下载 | 不把 Toast 当安全证据 | MIME、魔数/扩展名、隔离存储、签名 URL、权限 |
| OAuth | 第三方按钮不得宣称接入 | PKCE、redirect 白名单、Secret 轮换/撤销、TTL、审计 |
| 支付积分 | 余额变化只算视觉演示 | 原子事务、幂等、不可变账本、Outbox、对账、退款 |
| 管理操作 | Confirm 和原因字段 | 二次授权、不可抵赖审计、审批/申诉、回滚 |
| 隐私邮件 | 不收集真实凭证，不声称已发邮件 | PII 最小化、加密/保留、退订、发送可观测 |

## 7. 本次深度复查结果

- [x] 活动脚本确认：prototype/assets/prototype-app.js 已由 prototype/index.html 在 body 结束前加载；旧版逻辑仅保留在 template 参考区。
- [x] 入口重建记录（2026-08-28 晚）：此前的 SPA 入口 index.html 被行号前缀污染并在会话间丢失；已按 prototype-app.js 的挂载契约重建（壳层 + 静态 #page-home/#page-discover/#page-loading/#page-design，sprite 补齐 admin 图标），且 #design 页含设计规范速览。旧多页残留备份于 /tmp/stale-index-multidoc-backup.html。
- [x] 页面拆分记录（2026-09-02）：index.html 缩为约 2KB 薄壳；57 个 pages/*.html 承载全部路由标记（完整文档、可独立打开、CSS 共用）；共享骨架（SVG sprite/顶栏/底部导航/Toast）抽取到 assets/page-chrome.js；assets/page-loader.js 只负责首屏三页 + 动态加载引擎；引擎新增 TPL()/mountTpl() 惰性取模板（TPL_TOKEN 防过期渲染），全部渲染器改为“模板挂载 + 区域状态同步 + 事件绑定”。回归证据：`cd prototype && node verify.mjs` 182/182；报告为本地生成物，已清理；后台 23 路由 + 13 项交互冒烟全过（封禁/审计/帖子审核/BI 周期/系统设置/建板块/标签合并/市场上架/全员广播/角色权限/附件扫描均持久化）；pages/*.html 直开（独立模式）抽样全过、无控制台报错。
- [x] 真实交互复测：顶部搜索、首页分类/点赞、登录回跳、回复解锁、付费确认、消息失败重试、设备下线、OAuth 撤销、MFA 验证码校验、管理员 RBAC 均已执行。
- [x] 持久化修复：发布草稿/高级字段、回复、点赞、通知已读、消息、关注、设备和 OAuth 状态写入本地原型状态并可重绘恢复。
- [x] 自动回归：`cd prototype && node verify.mjs` 通过 182/182 项；console/pageerror/requestfailed 均为 0；报告为本地生成物，已清理。
- [x] 视觉主题：chinese-elegance.css 已修复并新增组件对齐层（列表金线分隔、宋体行标题、空心方标签、印章式空态、搜索焦点环、表头金线、代码块/引用/下拉/焦点环对齐）；板块页、首页、亮暗四视口截图复核通过。
- [ ] 真实后端未接入：当前仍是 Mock/localStorage；以下生产阻塞不能由原型结果替代。

## 8. 已知后端生产阻塞

1. **邮件发送器/通知投递**：注册验证、找回密码、通知/摘要需要真实 sender、队列、重试、幂等键、死信、告警和投递追踪；原型“已发送” Toast 不是证据。
2. **原型与后端边界**：当前 prototype/assets/prototype-app.js 没有 fetch/XMLHttpRequest/WebSocket，也没有 /api/v1 调用；因此本轮所有“可操作”均是本地 Mock 状态，不得写成后端已接入。
3. **真实认证会话**：登录/MFA/CSRF、设备撤销、密码策略、锁定和 429 必须后端集成与浏览器安全测试。
4. **存储附件**：本地/S3、签名 URL、私有 bucket、配额、魔数/扩展名校验、迁移校验、回滚、过期链接语义待证。
5. **受限内容**：回复/等级/支付解锁必须服务端授权；前端隐藏 DOM 不构成访问控制。
6. **积分商城市场**：扣款、权益、订单、Webhook、Outbox、对账、退款、并发幂等必须故障测试；市场不能直接改余额。
7. **举报处罚申诉**：状态机、权限、处罚、审计时间线、申诉和通知必须可追溯；当前 hash 未形成完整后台 IA。
8. **OAuth/OIDC**：授权、回调、PKCE、scope、Secret、撤销和 post-logout redirect 未端到端关闭。
9. **AI/视频/插件**：Secret、数据同意、任务取消/重试、视频白名单/CSP、禁止执行任意代码需服务端策略。
10. **数据库兼容迁移**：OpenAPI 要求 SQLite、MySQL 8、MariaDB 10.11 契约一致；需迁移、并发、回滚、完整性测试。
11. **可观测性运维**：request ID、日志、指标、告警、备份恢复、健康/就绪、灰度回滚不会因原型通过自动成立。

## 9. 正式环境准入门槛（全部满足前不得上线）

### 契约与后端
- [ ] OpenAPI lint/契约测试通过；operationId、handler、权限、错误码、CSRF、幂等一致。
- [ ] 每个声称已接入的入口有 network trace、成功/失败/超时/429 证据；Mock 明确隔离。
- [ ] 认证、授权、受限内容、积分/支付、上传下载、邮件通知、OAuth、审核有集成/E2E 测试。
- [ ] 迁移、备份恢复、兼容、并发、回滚演练完成。

### 安全与隐私
- [ ] SAST、依赖扫描、DAST、XSS/CSRF/SSRF/越权/限流/上传测试完成，高危关闭。
- [ ] Secret 不进仓库、浏览器持久化或日志；cookie、CSP、CORS、TLS、PII 经审查。
- [ ] 危险管理操作有 step-up/审计/告警；OAuth redirect/scope 白名单复核。

### 可靠性与运营
- [ ] 邮件 sender、队列、Outbox、重试/死信、Webhook、任务、存储、AI/video provider 有生产配置、健康检查、告警。
- [ ] 压测、故障注入、限流、RTO、备份恢复和容量阈值有记录。
- [ ] 发布清单含迁移顺序、feature flag、灰度、回滚、数据修复和 on-call。

### 产品与体验
- [ ] IA 要求的真实路径已实现，或产品正式批准 hash 原型与 IA 的差异：/articles、/boards/*、/topics/*、/register、/forgot-password、/settings/*、/admin/*、403/404。
- [ ] 1440/1024/768/390 亮暗回归、键盘和屏幕阅读器抽查完成且证据可追溯。
- [ ] 九条旅程的原型断言与真实 API 旅程分别签字；不得凭 Toast/Mock 数字关闭生产风险。

## 10. 签核

| 角色 | 内容 | 签核 |
|---|---|---|
| 产品 | IA/hash 差异、九条旅程 | 待签 |
| 设计/无障碍 | Token、响应式、WCAG 2.2 AA | 待签 |
| 前端 | 入口、状态、交互、回归证据 | 待签 |
| 后端 | operationId、权限、错误、数据一致性 | 待签 |
| 安全 | 会话、受限内容、上传、OAuth、管理操作 | 待签 |
| SRE/运营 | 邮件、队列、监控、备份、回滚、值班 | 待签 |
| 发布负责人 | 准入门槛全部满足 | 门槛未满足不得签发上线 |

**结论：原型通过不等于生产上线；除明确附证据的单项外，不预设任何数字全绿。**
