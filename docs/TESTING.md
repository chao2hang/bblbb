# BBLBB — 测试策略与验收矩阵

> 版本：v0.4
> 本文定义编码后必须持续执行的测试；“支持 SQLite/MySQL/MariaDB、安全 OIDC、隐藏内容”必须由自动化和恢复演练证明。

## 1. 测试层级

| 层 | 目标 |
|---|---|
| 单元测试 | Domain policy、状态机、解析和纯函数 |
| 仓储契约测试 | 同一 repository 行为在三数据库一致 |
| 集成测试 | HTTP、事务、Session、任务和存储适配器 |
| 端到端 | 浏览器真实流程和无 JS 退化 |
| 安全测试 | 身份、权限、CSRF、泄漏、上传和 OIDC |
| 性能测试 | 小机器预算、SQLite 写竞争、SSR 和 worker |
| 运维演练 | 迁移、备份、恢复、回滚和密钥轮换 |

## 2. CI 矩阵

每个 PR：

```text
Rust: fmt, clippy -D warnings, test, cargo audit/deny
Frontend: format, lint, check, vitest, build
Database:
  SQLite 3.40+
  MySQL 8.0+
  MariaDB 10.11+
Integration: migrations up + repository contract + API smoke
Docs: markdown links, referenced files, terminology checks
```

定期/发布前：Playwright、axe、安全扫描、性能测试、OIDC conformance 和恢复演练。

## 3. 数据库迁移测试

- 空库应用全部迁移。
- 上一发布版本数据库升级到当前版本。
- migration checksum 修改会失败。
- 外键、唯一约束、枚举/应用校验在三数据库效果一致。
- MySQL 与 MariaDB 分开执行，不以一个通过代表另一个。
- SQLite 每连接启用 foreign keys/WAL/busy timeout。
- 迁移失败不会留下被错误标记为成功的版本。

## 4. 仓储契约

同一测试套件运行于每个引擎：

- 用户规范化唯一性。
- 角色与板块 assignment。
- 列表/详情可见性一致。
- 发帖和文章的 `visibility_level` 只能处于 `1..作者当前等级`；前端篡改、更高草稿值、并发降级、定时发布前降级均返回 `visibility_level_exceeds_author`，不创建内容。
- 当前等级边界值可以发布，低等级和公开值可以发布；作者创建成功后始终能查看自己的正文。
- 主题内楼层唯一和并发分配。
- 帖子版本冲突。
- 软删除/恢复。
- Cursor 分页不重不漏。
- Outbox 与业务事务原子性。
- 授权码一次消费。
- Refresh Token family 重用检测。

## 5. 权限测试

组合角色：

- 匿名。
- pending/active/restricted/banned 用户。
- member。
- 当前板块 moderator。
- 其他板块 moderator。
- 全局 moderator。
- administrator。

对象条件：

- 自己/他人内容。
- public/members/restricted/hidden 板块。
- draft/pending/published/hidden/deleted 内容。
- 锁定帖子。
- 有效/过期 assignment。
- mute/board mute/ban。
- 已/未解锁限制正文。

必须直接调用 API 验证拒绝，不能只看 UI 按钮。

## 6. 隐藏内容防泄漏

测试未授权用户无法从以下渠道取得 `restricted_html` 原始标记字符串：

- 帖子/回复详情 API。
- 列表、excerpt 和相关内容推荐。
- SSR HTML 与 hydration payload。
- RSS/Atom。
- sitemap、OpenGraph、JSON-LD。
- 搜索索引和高亮。
- 通知和邮件。
- 日志、tracing、错误响应和审计 metadata。
- 公共缓存/304。
- 附件下载。

付费解锁测试并发重复请求只扣一次并只创建一个 grant。

## 7. 市场交易测试

- 仅 Confidential Client、批准 scope、有效用户同意可创建/确认购买；Public Client、普通 OIDC scope 和撤销授权全部拒绝。
- Offer 金额、货币、版本、收款方不可被请求体覆盖；旧版本、禁用 Offer、过期或已消费 Intent 拒绝。
- 同一幂等键重复成功、重复失败和并发确认只产生一个 Purchase、一个扣款和一条有效账本流水；不同摘要返回 409。
- 双花、余额不足、单笔/日限额、库存竞争、用户封禁和 Client 紧急禁用均无余额或库存泄漏。
- SQLite `BEGIN IMMEDIATE`、MySQL/MariaDB 行锁竞争以及固定锁顺序通过；注入每一步失败验证全回滚。
- 成功响应只在数据库提交后返回；数据库提交前断连时查询结果只能是已提交或明确不存在，不得伪造成功。
- 退款并发、累计超额、幂等重试和补偿流水通过；原 Purchase/point transaction 不被更新或删除。
- Checkout Intent、Access Token、Webhook 签名重放、时间窗、CSRF、IDOR、越权和价格篡改测试通过。
- Webhook HMAC 校验、重复/乱序/延迟、轮换、SSR​​F 阻断、重试和 dead-letter 通过；对账可恢复。
- 账本恒等式、Purchase 与 point operation 一致性、Outbox 原子性和告警指标通过属性/故障注入测试。

## 8. 下载抵扣积分测试

- 全局/板块/附件价格优先级、免费等级/角色、停用策略、策略版本变更和限额。
- 重复下载、有效授权复用、并发下载、Idempotency-Key 冲突只产生一次扣费。
- 余额不足、对象未 ready、无附件权限、封禁和 URL 签发失败不会泄漏授权或重复扣款。
- SQLite/MySQL/MariaDB 锁竞争与每一步故障注入；账本、授权和账户保持一致。
- S3 URL 到期重新签发不删除对象、不释放容量、不新增扣费；Range 请求不重复收费。

## 9. 内部商城、装扮与活跃测试

- 商品价格/货币/库存/版本均以后端为准；并发购买只产生一次扣费和一次 entitlement。
- 装备槽位互斥、徽章最多 3 个、限时过期自动卸下、撤销和补偿不修改历史账本。
- Token-only 展示值拒绝任意 CSS/HTML/JS/远程 URL/SVG；XSS、CSS 逃逸、远程资源和冒充官方徽章测试。
- 登录用户每日首次有效页面访问自动签到，按用户时区（缺失时站点时区）每日幂等；并发开页、刷新、预取、静态资源、健康检查、匿名访问、失败请求和爬虫不得重复或错误发奖。
- 活跃任务去重、冷却、每日上限、自我互动、批量刷反应和奖励反查测试。
- 用户关闭他人装饰、减少动效和隐私设置后，帖子、回复、通知、榜单展示降级一致。
- Reaction 创建/删除、数量包消耗、通知偏好、目标权限和限流必须覆盖。

## 10. 积分测试

- 奖励、消费、冻结、解冻、转账、管理员调整和补偿。
- 不允许负余额时拒绝透支。
- 账本 `balance_after` 与账户一致。
- 历史流水不可修改。
- 相同幂等键相同请求返回原结果。
- 相同 key 不同请求返回冲突。
- SQLite 并发写、MySQL/MariaDB 行锁竞争。
- 模拟事务各步骤失败，保证无“余额变但流水没写”。
- 经验等级缓存可从账户重建。

可用属性测试验证所有账户：

```text
初始余额 + Σ(delta_balance) = 当前余额
初始冻结 + Σ(delta_frozen) = 当前冻结余额
```

## 11. Session 与 CSRF

- 登录成功/失败、账号锁定和统一错误。
- Session fixation：登录后 Token 变化。
- idle/absolute 过期。
- 登出、改密码、封禁、设备撤销。
- Cookie 属性：`__Host-`、Secure、HttpOnly、Path、SameSite。
- 无/错/其他 Session 的 CSRF token 被拒绝。
- 跨 Origin POST 被拒绝。
- GET 无副作用。
- 登录 CSRF 和 form action 代理的 Set-Cookie 传播。

## 12. OIDC

自动化：

- Discovery 与 JWKS。
- Public/Confidential Client。
- redirect 精确匹配和危险变体。
- PKCE 缺失、plain、错误 verifier。
- state Client 集成流程与 nonce claim。
- code 过期、重放、绑定错误 redirect/Client。
- ID Token `iss/sub/aud/exp/iat/auth_time/nonce/kid`。
- scope 和 consent。
- opaque Access Token 的 userinfo。
- Refresh Rotation、旧 token reuse 撤销 family。
- Client 禁用、用户封禁和 consent 撤销。
- key rotation 期间新旧 Token 校验。
- logout 与 post logout redirect。

启用 v1.0 OIDC 前运行适用的 OpenID Foundation conformance profile，并保存报告。

## 13. 审核测试

- 举报去重和状态机。
- 板块版主范围。
- 自己案件的利益冲突阻断。
- 内容 hide/restore revision。
- mute、board mute、rate limit、ban 的实时生效与到期。
- 封禁撤销 Session/Refresh Token。
- 申诉接受创建撤销记录而不删历史。
- 举报者与内部备注不泄漏。

## 14. 文件与存储

- 本地/S3 adapter contract；发布前至少覆盖 AWS S3、MinIO 与 Cloudflare R2，其他厂商通过同一契约测试后才声明兼容。
- Virtual-hosted-style 与 Path-style、Region/`auto`、签名版本、Multipart 和预签名上传/下载。
- 路径穿越、绝对路径、符号链接和服务端生成对象 key。
- MIME 欺骗、SVG、polyglot、图片炸弹和超尺寸。
- 中断上传、过期签名、对象被替换、`HEAD` 大小不符、重复 complete 和 pending 清理。
- S3 临时公开链接在 TTL 边界前后按服务端时间生效；旧 URL 到期失效，但附件对象仍为 `ready`，重新鉴权可获得新 URL。
- 后台修改链接 TTL 只影响新签发链接，不删除对象；上传与下载预签名 TTL 可分别验证。
- 等级单附件上限和总容量的后台读取、修改、审计，以及升级、降级、处罚覆盖和并发上传竞争。
- 预签名后用户降级、并发占满容量或对象实际大小变大时，`complete` 必须拒绝且不超卖配额。
- 私有/受限附件权限、Range、缓存和签名 URL 过期；未授权请求不能获得或刷新签名 URL。
- Secret 不出现在 API、SSR/hydration、浏览器持久化、日志、错误、审计 metadata 和配置导出。
- S3 403/404/429/5xx、超时、DNS/TLS 错误和部分上传的重试/隔离行为。
- 图片元数据移除和缩略图。
- 资料 Cover 上传、预览、替换、移除、JPG/PNG/WebP 白名单、5MB/像素上限和移动端裁切展示。
- Cover 只能引用本人已完成且通过安全处理的附件；任意远程 URL、SVG、脚本和跨用户附件引用必须拒绝。
- Cover 的 S3 临时 URL 过期后重新鉴权签发，不能因 URL 过期删除对象；移除 Cover 只解除资料引用。
- Cover 与头像、帖子图片和普通附件共享用户总容量；上传、并发更换、延迟清理、降级超额和物理删除释放容量的数值必须一致。
- 帖子、文章和回复的作者头像/昵称支持鼠标 Hover 与键盘 Focus 展示统一资料卡；卡片同步安全 Cover、可进入主页，移动触屏不阻挡原导航。
- Hover Card 不泄漏私有资料和签名 URL；Cover 加载失败、用户不存在、减少动态效果及窄屏时安全降级。
- 孤儿 mark-and-sweep 不误删在用文件。
- 本地→S3 与 S3→本地迁移的对象数量、size/hash、权限、断点续传、切换和回滚。
- 数据库与附件恢复后一致性。

## 15. 前端与可访问性

Playwright 流程：

- 匿名浏览文章/论坛。
- 注册、验证、登录、退出和 Session 管理。
- 发文章、发讨论、草稿、冲突提示、回复。
- 举报、审核、处罚和申诉。
- 积分/等级/解锁。
- 主题切换和默认 fallback。
- 管理后台高风险确认。

可访问性：

- axe 无严重/关键错误。
- 全流程键盘操作。
- 焦点管理、表单错误关联、对比度和减少动画。
- 无 JavaScript 时公开浏览和关键表单仍合理退化。

## 16. API 契约

- OpenAPI 生成与提交文件一致。
- problem+json 格式和稳定 code。
- 401/403/404 策略。
- Cursor 分页、未知参数、最大 limit。
- ETag/If-Match 版本冲突。
- 429 与 Retry-After。
- 隐私 DTO 与管理员 DTO 分离。
- 向后兼容测试使用上一版本生成客户端。

## 17. AI 与任务故障注入

- Provider Base URL 白名单、TLS、SSRF、DNS 重绑定、重定向、超时和超大响应。
- Secret 不出现在 API、浏览器、SSR、日志、错误或审计 metadata。
- Prompt injection、隐藏正文/审核备注泄漏、用户撤回同意和数据脱敏边界。
- 模型输出 schema、XSS/Markdown/SEO 注入、URL/SQL/模板注入和事实篡改。
- 格式化建议必须 diff 预览并由用户采纳；SEO 建议不能绕过公开状态和版本校验。
- Provider 429/4xx/5xx、熔断、重试、预算、并发和任务取消；失败不能绕过核心审核。
- 重复任务、旧 revision、旧策略结果不能覆盖新内容；至少一次 worker 不产生重复采纳。

## 18. 视频插件测试

- 常见 MP4/WebM/OGV/MOV URL、MIME/扩展名欺骗、Range、超时和超大响应。
- HLS playlist 递归、跨域分片、Key/Map、分片爆量、总字节/时长/深度限制和签名 URL 泄漏。
- 西瓜视频页面 URL 精确 Host/官方嵌入域名、URL 变体、下架、限流、无嵌入权限和安全外链降级。
- SSRF、userinfo、Unicode/IDN、DNS 重绑定、私网 IPv4/IPv6、开放重定向和非标准端口。
- 帖子权限、审核状态、CSP frame-src、iframe sandbox、referrerpolicy、自动播放/麦克风/摄像头限制。
- 插件 capability 越权、重复 resolve、旧策略、Provider 故障、历史引用重新检查和无 JavaScript 降级。

## 19. 任务与故障注入

- Outbox 与业务提交/回滚。
- Worker 崩溃、lease 到期和重复执行。
- SMTP/S3 超时、永久错误和 dead-letter。
- 幂等 handler。
- 优雅停机不领取新任务。
- SQLite busy 时退避而非高频自旋。
- 邮件任务不会把 token 写日志。

## 20. 性能预算

在明确环境记录：CPU、RAM、数据库、数据量、并发、命令和 commit。

SQLite 512MB 参考场景：

- 10 万用户、100 万帖子/回复级的合成数据（可分阶段建立）。
- 首页/文章/板块 SSR。
- 登录、发帖、回复。
- 积分并发。
- worker 处理邮件/缩略图时 HTTP 延迟。

验收使用 SLO，而非无依据 QPS，例如：

- 公开文章 p95 服务端响应目标。
- 登录和发帖 p95。
- 峰值 RSS 不超过部署预算并留系统余量。
- 无持续 SQLite busy 错误。

数值在第一次基准测试后写入版本化性能基线。

## 21. 备份恢复与升级演练

每次发布候选至少：

- 从上一版本数据执行迁移。
- 验证兼容回滚路径或明确不可回滚。
- 恢复最近 SQLite 备份。
- 定期恢复 MySQL 和 MariaDB 备份。
- 恢复附件和 OIDC key 后验证旧 ID Token/JWKS。
- 校验账户与账本、附件 hash、授权 grant、迁移版本。

“备份命令成功”不等于恢复测试成功。

## 22. 发布门槛

统一功能发布矩阵见 [`DOCUMENT-STATUS.md`](DOCUMENT-STATUS.md)。任何可选领域只有满足本节和其专项门槛后才能打开 Feature Flag：

- Marketplace：双边账本恒等式、user-bound Checkout 时序、Client service refund、库存/退款并发、Webhook 对账和紧急冻结通过。
- Download Billing：完整策略优先级、免费授权、URL 签发失败不重扣、跨引用上下文和三数据库竞争通过。
- AI：同意粒度、撤回取消/丢弃迟到输出、202/200 union、Suggestion version 和 Provider 故障降级通过。
- Video：核心 Video Service/Adapter 边界、SSRF/HLS Corpus、动态 CSP、浏览器直连/外链降级和版权阻断通过。
- 稳定错误码、状态机、权限矩阵和领域事件均有契约测试；文档与 OpenAPI 不允许存在未批准差异。

v1.0：

- 三数据库迁移/契约绿。
- 核心权限、审核、Session、CSRF、内容泄漏测试绿。
- 默认主题 Playwright + axe 绿。
- SQLite 恢复演练成功。

v1.0 OIDC 专项门槛：

- conformance 适用 profile 通过。
- key rotation 和 Refresh reuse 测试通过。
- 与至少两个独立 RP 集成。
- OIDC 密钥恢复演练通过。

## 23. M14：Playwright、axe 与无 JS 验收（2026-08 追加）

M14 交付 Playwright E2E + axe 可访问性基线：

- **环境编排**：`cd frontend && npm run test:e2e`。`playwright.config.ts` 的
  webServer 运行 `tests/playwright/fixtures/serve.mjs`：重建 `data/e2e.sqlite`
  → 真实 Rust 后端 `--migrate` 启动 → `seed-personas.mjs` 铸成 DB persona
  （anonymous/unverified/cooldown/member/moderator/admin/mute/banned，会话按
  `user_sessions` 真实 schema 铸造）→ `vite dev --port 4173`（`/api` 代理到后端）。
- **项目矩阵**：`desktop-chromium`（1280×720）+ `mobile-chromium`（Galaxy S9 触屏
  语义，viewport 360×740）；`workers=1` 串行（共享后端 persona 数据）。
- **流程覆盖**：`tests/playwright/flows-{public,member,economy,admin}.spec.ts`
  覆盖匿名浏览/搜索/注册/登录/验证、发帖/回复/举报/申诉、附件/Cover/下载/积分/
  商城/衣柜/视频/AI 同意、管理后台高风险设置（reason 必填/409/recent-auth）。
- **axe 基线（P0）**：`a11y-axe.spec.ts` 扫描公开+认证+管理页面，serious/critical
  违规 = 测试失败；报告 artifact `tests/a11y/axe-report.json`（含缺陷 target/html，
  修复证据）。已知修复：认证页脚/正文文本链接默认下划线（WCAG 1.4.1）、移动端
  「发布」按钮文字 sr-only（link-name）、表单链接加 `.text-link`。
- **无 JS**：`nojs.spec.ts` 用 `javaScriptEnabled:false` 上下文跑公开阅读、注册
  （原生表单 action）、登录（303 跳转）与搜索 GET 表单。
- **键盘/焦点/减少动效**：`keyboard-focus.spec.ts`（skip link/Tab 遍历/读屏名称/
  `prefers-reduced-motion`）；Dialog 焦点陷阱/Escape/焦点回收由
  `ui/base-components.test.ts`（vitest）覆盖。
- **响应式**：`responsive.spec.ts` 覆盖 200% 文本放大、360px 窄屏、触屏 tap、
  横竖屏、慢网络（CDP 限速）与图片失败降级（alt 可读）。
- **记录**：`tests/a11y/records.json` 保存浏览器版本/viewport/locale/commit/报告/
  人工验收；`tests/a11y/seo-perf.json` 记录公开首屏 p95/HTML 大小/JS 预算
  （构建 immutable JS）/图片 lazy/峰值 RSS（M14-SEO-05）。
- **已知诚实记录**：真实注册消费后端 IP 注册配额（3 次/小时），注册提交类用例只在
  desktop 项目执行，重复运行小时内命中 429 时断言接受限流降级；搜索 FTS 由后端
  维护，API 创建的种子帖子未被索引，帖子定位改用 `/boards/general` 板块列表。

## 24. M15：生产运维演练（2026-08 追加）

| 演练 | 命令 | 结果/频率 |
|---|---|---|
| 备份/恢复（SQLite） | `ops/backup/drill-sqlite.sh` | 实测 RPO=0、RTO=0.18s；每周（oncall.md §5） |
| 迁移升级（上一版本→当前） | `deploy/scripts/drill-migration-upgrade.sh` | apply_ms=68、lock_events=0；每次发布 |
| 优雅停机（HTTP/worker） | `ops/test-graceful-shutdown.sh` | HTTP 0.30s / worker 0.04s 干净退出；每次发布 |
| 发布后冒烟 | `ops/smoke/smoke.sh` | db/登录/发帖/回复/附件/账本/管理 API；每次发布 |
| 告警表推 | `deploy/monitoring/alerts-drill.sh` | PASS=71；每月 |
| 日志脱敏扫描 | `ops/scan-log-corpus.sh --test` + 实测 | CLEAN；每次发布/日志变更 |
| 恢复内容校验 | `ops/restore/verify.sh` + `verify-attachments.sh` + `verify-oidc-keys.sh` | 每次恢复 |
| release bundle / Caddy / 权限 | `deploy/tests/test-release-bundle.sh` | PASS=26；每次构建 |
| 非作者执行 Runbook | `ops/runbooks/execution-sqlite-restore-2026-08-07.txt` | PASSED；每次 Runbook 变更 |

MySQL/MariaDB 实机备份恢复、S3 版本化演练、SMTP 故障演练、生产主机部署
执行为外部基础设施阻塞项（M15-BACKUP-02/03、M15-RUNBOOK-03、M15-PACKAGE-08
`[!]`），脚本与文档已就绪。
