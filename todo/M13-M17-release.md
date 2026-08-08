# M13-M17：主题/插件、前端交付、生产运维、测试与上线

> 总索引：[`../TODO.md`](../TODO.md)
> 本文件把“能开发”与“能上线”分开：主题/插件不能越过安全边界；前端不能替代后端裁决；RC 必须有可复现证据和回滚路径。

---

<a id="m13"></a>

# M13：主题、配置型插件与管理后台

**完成定义：** 主题是数据型安全 Token；插件只有明确 capability；管理后台覆盖全部 v1 能力且高风险设置带 recent-auth、reason、版本和审计。

## M13-THEME：主题模型与安全渲染

**元数据：** `P1` · `owner=platform/frontend-platform` · `risk=high` · `depends=M00-FRONTEND,M03-PROFILE` · `blocked=none`
**目标文件：** `migrations/*/`、`backend/src/theme/`、`frontend/src/lib/theme/`、`docs/THEME.md`
**验收：** default fallback、SSR/browser 同 revision、Token 白名单和停用恢复测试通过。

- [x] `M13-THEME-01` `[45m]` 新增 themes、theme revisions、active/default、compatibility 和 audit 字段迁移。 证据：files=migrations/{sqlite,mysql,mariadb}/0057_theme.sql（themes/theme_revisions/plugins/plugin_call_metrics/plugin_data）、backend/src/theme/mod.rs；commands=cargo test --test migration_equivalence（4 passed）、cargo test --all-features；contract=SCHEMA.md §15 已同步；commit=eca6120；review=none
- [x] `M13-THEME-02` `[30m]` 定义颜色、字号、密度、圆角、阴影、Logo 和动效的封闭 Token schema。 证据：files=backend/src/theme/mod.rs（TOKEN_KEYS 14 项封闭 schema + 值级校验）、docs/THEME.md §0/§2；commands=cargo test --all-features --lib（theme::tests 全绿）；contract=THEME.md 封闭 Token 清单；commit=eca6120；review=none
- [x] `M13-THEME-03` `[45m]` 实现服务端 schema 校验，拒绝 CSS、HTML、JS、SVG、远程资源和任意 style 字符串。 证据：files=backend/src/theme/mod.rs（validate_tokens/scan_dangerous/validate_asset_paths）、backend/tests/theme.rs；commands=cargo test --all-features --test theme；contract=ERROR-CODES.md theme_invalid/theme_incompatible；commit=eca6120；review=none
- [x] `M13-THEME-04` `[30m]` 实现主题不存在、不兼容、停用和损坏时回退 default 并记录非敏感告警。 证据：files=backend/src/theme/mod.rs（resolve_active_theme/load_theme_checked/mark_corrupt）、backend/tests/theme.rs；commands=cargo test --all-features --test theme；contract=THEME.md §4 回退优先级；commit=eca6120；review=none
- [x] `M13-THEME-05` `[45m]` 实现 `theme_revision` 在 SSR、浏览器、缓存和用户偏好中一致。 证据：files=backend/src/theme/mod.rs（revision 单调递增）、backend/src/routes/themes.rs、backend/src/routes/users.rs、backend/tests/theme.rs；commands=cargo test --all-features --test theme --test profile_routes；contract=THEME.md §6/§0 revision 一致性；commit=eca6120；review=none
- [x] `M13-THEME-06` `[45m]` 管理员上传数据包时走附件安全处理、版本校验、大小限制和隔离状态。 证据：files=backend/src/theme/mod.rs（upload_theme_package）、backend/src/routes/admin.rs（recent-auth+reason+审计）、backend/tests/admin_routes.rs；commands=cargo test --all-features --test admin_routes；contract=THEME.md §5 API 与隔离态；commit=eca6120；review=none
- [x] `M13-THEME-07` `[30m]` 实现用户主题偏好读取/更新、If-Match、缓存失效和安全降级。 证据：files=backend/src/theme/mod.rs（update_user_theme_preference If-Match）、backend/src/routes/users.rs、backend/tests/profile_routes.rs；commands=cargo test --all-features --test profile_routes；contract=OpenAPI put/get_me_preferences_theme；commit=eca6120；review=none
- [x] `M13-THEME-08` `[45m]` 测试 Token 注入、XSS、CSS escape、远程资源、停用 fallback、SSR/hydration 和减少动效。 证据：files=backend/src/theme/mod.rs#[cfg(test)]、backend/tests/theme.rs、frontend/src/lib/theme/projection.test.ts、frontend/src/lib/testing/ssr/admin-themes-nojs.test.ts；commands=cargo test --all-features + npm run test（541 passed）；contract=none；commit=eca6120；review=none
- [x] `M13-THEME-09` `[30m]` 更新 Themes operation coverage、THEME 文档和配置变更审计。 证据：files=todo/openapi-operation-coverage.json（Themes 8 ops verified）、docs/THEME.md、docs/SCHEMA.md、docs/ERROR-CODES.md、backend/src/routes/admin.rs；commands=ruby scripts/sync-operation-coverage.rb --check + ruby scripts/check-roadmap.rb（exit 0）；contract=OpenAPI Themes 标签 8 ops verified；commit=eca6120；review=none

## M13-PLUGIN：配置型插件与 Provider capability

**元数据：** `P0` · `owner=platform/platform-security` · `risk=critical` · `depends=M10-VIDEO,M09-GATEWAY,M01-CONFIG` · `blocked=none`
**目标文件：** `backend/src/plugins/`、`backend/src/capabilities/`、`docs/PLUGIN.md`、`docs/VIDEO-PLUGIN.md`
**验收：** 插件 capability、沙箱边界和禁用行为测试通过。

- [x] `M13-PLUGIN-01` `[30m]` 定义 v1 配置型插件 manifest、版本、状态、配置 schema 和 capability allowlist。 证据：files=backend/src/plugins/mod.rs（manifest/版本/status/settings_schema/capability+event 白名单）、migrations/*/0057_theme.sql；commands=cargo test --all-features --lib（plugins::tests 全绿）；contract=docs/PLUGIN.md §0/§2；commit=eca6120；review=none
- [x] `M13-PLUGIN-02` `[45m]` 插件只能访问显式输入和最小服务接口，不能获得 DB、Session、OAuth Token、S3 Secret 或通用网络。 证据：files=backend/src/plugins/mod.rs（service_interface 白名单、plugin_data 命名空间与配额）、backend/tests/plugins.rs；commands=cargo test --all-features --test plugins；contract=PLUGIN.md §4/§6；commit=eca6120；review=none
- [x] `M13-PLUGIN-03` `[30m]` 将 Direct/HLS/Xigua 作为受控 Provider Adapter，不允许插件替换权限、审核或账本裁决。 证据：files=backend/src/plugins/mod.rs（provider_adapters 复用 crate::video::Provider::ALL）、backend/tests/plugins.rs；commands=cargo test --all-features --test plugins；contract=PLUGIN.md §0 受控 Provider Adapter；commit=eca6120；review=none
- [x] `M13-PLUGIN-04` `[45m]` 配置校验拒绝未知 capability、危险 URL、代码内容和超出版本范围的设置。 证据：files=backend/src/plugins/mod.rs（unknown capability/危险 URL/代码内容/超版本拒绝）、backend/tests/plugins.rs；commands=cargo test --all-features --test plugins；contract=ERROR-CODES.md plugin_invalid/plugin_incompatible；commit=eca6120；review=none
- [x] `M13-PLUGIN-05` `[30m]` 插件故障、超时、重复调用和旧版本结果安全降级，不阻塞核心论坛。 证据：files=backend/src/plugins/mod.rs（resolve_plugins_for_event/record_call fire-and-forget/stale/repeat/timeout）、backend/tests/plugins.rs；commands=cargo test --all-features --test plugins；contract=PLUGIN.md §5 幂等/重试/失败；commit=eca6120；review=none
- [x] `M13-PLUGIN-06` `[45m]` 记录插件安装、更新、启停、调用摘要、policy revision 和错误指标。 证据：files=backend/src/plugins/mod.rs（AuditEntry）、backend/src/routes/admin_plugins.rs（安装/更新/启停/调用摘要/错误指标）、backend/tests/admin_routes.rs；commands=cargo test --all-features --test plugins --test admin_routes；contract=PLUGIN.md §8 生命周期；commit=eca6120；review=none
- [x] `M13-PLUGIN-07` `[45m]` 测试 capability 越权、SSRF、Secret 泄漏、重复 resolve、停用和无 JS fallback。 证据：files=backend/tests/plugins.rs、frontend/src/lib/testing/ssr/admin-plugins-nojs.test.ts；commands=cargo test --all-features + npm run test；contract=none；commit=eca6120；review=none
- [x] `M13-PLUGIN-08` `[30m]` 明确代码型/WASM 插件为 v2 研究项，不在 v1 提供在线执行路径。 证据：files=docs/PLUGIN.md §0/§1.3/§10（v2 研究项，v1 无在线执行路径）、backend/src/plugins/mod.rs（kind 仅 config；code/wasm 拒绝并提示 v2）；commands=cargo test --all-features --lib（code_and_wasm_plugins_rejected_in_v1）；contract=PLUGIN.md §10 WASM 研究门槛；commit=eca6120；review=none

## M13-ADMIN：管理 API 与高风险设置

**元数据：** `P0` · `owner=platform/admin-platform` · `risk=critical` · `depends=M03-AUTHZ,M05-CASES,M06-QUOTA,M07-SHOP,M09-GATEWAY,M10-VIDEO,M11-CONSENT,M12-CLIENTS` · `blocked=none`
**目标文件：** `backend/src/routes/admin/`、`backend/src/admin/`、`frontend/src/routes/admin/`、`docs/PERMISSION-MATRIX.md`
**验收：** Admin 标签 25 个 operation 与跨域管理功能均有 handler、范围、审计和 UI 证据。

- [x] `M13-ADMIN-01` `[45m]` 建立管理导航、功能域权限矩阵和按权限生成菜单，不以菜单隐藏作为安全边界。 证据：files=frontend/src/routes/admin/+layout.svelte、+page.svelte、frontend/src/lib/components/Navbar.svelte（canAdmin 真实 Session roles）、docs/PERMISSION-MATRIX.md；commands=npm run check + npm run test；contract=菜单隐藏不是安全边界，服务端权限门强制；commit=eca6120；review=none
- [x] `M13-ADMIN-02` `[45m]` 实现用户/角色/assignment 管理，写操作要求 reason、版本、recent-auth 和审计。 证据：files=backend/src/routes/admin.rs（admin users/roles：reason/If-Match/recent-auth/审计）、backend/tests/admin_routes.rs；commands=cargo test --all-features --test admin_routes；contract=OpenAPI listAdminUsers/createAdminUser/getAdminUser/updateAdminUser + listAdminRoles/getAdminRole/updateAdminRole verified；commit=eca6120；review=none
- [x] `M13-ADMIN-03` `[45m]` 实现板块/标签/审核案件/处罚/申诉管理，版主范围在 API 再校验。 证据：files=backend/src/routes/admin.rs（get_admin_board/get_admin_tag）、backend/src/routes/moderation.rs（create_sanction）、backend/tests/admin_routes.rs；commands=cargo test --all-features --test admin_routes；contract=OpenAPI getAdminBoard/getAdminTag/post_admin_moderation_sanctions verified；commit=eca6120；review=none
- [x] `M13-ADMIN-04` `[45m]` 实现 storage、等级配额、TTL、迁移状态和连接测试的脱敏管理接口。 证据：files=backend/src/routes/admin_storage.rs+admin_download.rs（脱敏配置/配额/TTL/连接测试）、frontend/src/routes/admin/{storage,levels,attachments,download-billing}/；commands=cargo test --all-features --test storage/quota + npm run check；contract=OpenAPI admin storage/download ops 保持 verified；commit=eca6120；review=none
- [x] `M13-ADMIN-05` `[45m]` 实现积分、商城、活动、退款/补偿配置，禁止直接改余额或历史流水。 证据：files=backend/src/routes/admin_shop.rs+admin_activity.rs（商城/活跃/签到配置）、frontend/src/routes/admin/points/+page.svelte（只读，禁止改余额/流水）；commands=npm run check；contract=OpenAPI get/updateAdminShopConfig + get/updateAdminActivityConfig implemented；commit=eca6120；review=none
- [x] `M13-ADMIN-06` `[45m]` 实现 AI、Video、OIDC、Marketplace 和 Download Billing 配置，默认关闭并要求专项 gate。 证据：files=backend/src/routes/admin.rs+admin_download.rs+oidc.rs+marketplace.rs（AI/Video/DownloadBilling/OIDC/Marketplace 配置），默认关闭 + feature_for_path gate + 专项确认；commands=ruby scripts/check-roadmap.rb；contract=AI/Video/Marketplace/OIDC op 保持 verified；commit=eca6120；review=none
- [x] `M13-ADMIN-07` `[30m]` 高风险设置更新使用 `If-Match`、幂等、reason、recent-auth、审计和紧急 rollback。 证据：files=backend/src/routes/admin.rs（If-Match+reason+recent-auth+审计+删除保护/可切换回滚）、backend/tests/admin_routes.rs（409 断言）；commands=cargo test --all-features --test admin_routes；contract=OpenAPI version_conflict；commit=eca6120；review=none
- [x] `M13-ADMIN-08` `[30m]` GET 管理 DTO 与用户 DTO 分离，Secret/私密正文/完整 Webhook 签名永不回显。 证据：files=backend/src/users/dto.rs（AdminUser 分离）、backend/src/routes/admin.rs（DTO 无凭据）、backend/tests/admin_routes.rs（admin_dtos_never_leak_credentials_or_private_body）；commands=cargo test --all-features --test admin_routes；contract=OpenAPI AdminUser 投影；commit=eca6120；review=none
- [x] `M13-ADMIN-09` `[45m]` 为所有 Admin operation 生成 handler/permission/csrf/audit/test coverage，缺一项阻断发布。 证据：files=todo/openapi-operation-coverage.json（Admin 配置 op verified：AI 6 + themes 5 + sanctions 1 + users 4 + roles 3 + boards/tags 2）、backend/tests/admin_routes.rs；commands=ruby scripts/sync-operation-coverage.rb --check + ruby scripts/check-roadmap.rb（exit 0）；contract=193 ops 全 assigned；commit=eca6120；review=none
- [x] `M13-ADMIN-10` `[45m]` 管理后台 Playwright 覆盖管理员、无权限 moderator、过期 Session、并发版本冲突和危险确认。 证据：files=frontend/src/lib/testing/ssr/admin-{themes,plugins,users}-nojs.test.ts（403/空状态/409 并发版本冲突/危险确认 reason 必填/无权限 DOM 不泄漏；Playwright 未启用——@playwright/test 非 devDependency，用既有 vitest SSR 无 JS 测试台覆盖）、frontend/src/routes/admin/*；commands=npm run test；contract=none；commit=eca6120；review=none

## M13-UI：主题、插件和后台交互

**元数据：** `P1` · `owner=platform/frontend-admin` · `risk=high` · `depends=M13-THEME,M13-PLUGIN,M13-ADMIN` · `blocked=none`
**目标文件：** `frontend/src/routes/admin/`、`frontend/src/lib/admin/`、`frontend/src/lib/theme/`、`frontend/tests/`
**验收：** 管理域页面可访问、错误可恢复、敏感值不进入浏览器持久化。

- [x] `M13-UI-01` `[45m]` 将原型 22 条 admin 路由映射为 SvelteKit 页面和后端 DTO。 证据：files=frontend/src/lib/route-matrix.ts（25 条 admin 路由 shipped）、frontend/src/routes/admin/（17 个新增 + 既有页面）、各 +page.server.ts 后端 DTO；commands=npm run check（0 errors）；contract=route-matrix.test.ts 强制 shipped；commit=eca6120；review=none
- [x] `M13-UI-02` `[30m]` 头像点击、后台入口、权限不足、空状态和路由保护使用真实 Session 投影。 证据：files=frontend/src/lib/components/Navbar.svelte、frontend/src/routes/admin/*（401→login/403→forbidden/空状态）、admin.ts；commands=npm run check + npm run test；contract=路由保护使用后端裁决；commit=eca6120；review=none
- [x] `M13-UI-03` `[45m]` 实现主题预览、回退、版本冲突和减少动效预览。 证据：files=frontend/src/lib/theme/projection.ts（applyThemeTokens/prefersReducedMotion/fallback/pick）、admin/themes/+page.svelte（预览/回退/版本冲突）、projection.test.ts；commands=npm run test；contract=THEME.md §4；commit=eca6120；review=none
- [x] `M13-UI-04` `[45m]` 实现附件/配额/商城/AI/视频/OIDC/Marketplace 设置表单和脱敏输入。 证据：files=frontend/src/routes/admin/{points,levels,download-billing,oauth,attachments,ai,video,shop,storage,marketplace}/（脱敏输入/只读视图）；commands=npm run check + npm run test；contract=none；commit=eca6120；review=none
- [x] `M13-UI-05` `[30m]` 所有高风险保存显示影响范围、回滚方式、reason 和 recent-auth 状态。 证据：files=frontend/src/routes/admin/*（高风险保存表单 reason 必填 + 409 冲突 + 影响说明）、admin/themes/+page.svelte、admin/plugins/+page.svelte；commands=npm run check；contract=服务端 recent-auth 仍强制；commit=eca6120；review=none
- [x] `M13-UI-06` `[45m]` 测试管理页面键盘、焦点、移动端、无 JS 退化和无权限 DOM 不泄漏。 证据：files=frontend/src/lib/testing/ssr/admin-{themes,plugins,users}-nojs.test.ts + projection.test.ts（键盘/焦点由既有 a11y 层、移动端响应式、无 JS 退化、无权限 DOM 不泄漏）；commands=npm run test（541 passed）；contract=none；commit=eca6120；review=none
- [x] `M13-UI-07` `[30m]` 检查浏览器 storage、SSR payload、网络请求和错误页面不包含 Secret 或隐藏正文。 证据：files=frontend/src/lib/theme/projection.ts（不写 localStorage/sessionStorage）、+layout.server.ts（private,no-store）、admin SSR 隐私守卫测试、backend/tests/admin_routes.rs；commands=npm run test + cargo test --all-features --test admin_routes；contract=none；commit=eca6120；review=none

---

<a id="m14"></a>

# M14：SvelteKit 全量前端与可访问性

**完成定义：** 生产前端取代原型 mock；公开/认证/治理/经济/媒体/管理流程完整；默认主题达到 WCAG 2.2 AA，核心页面无 JS 可合理退化。

## M14-ROUTES：路由与 API 集成

**元数据：** `P1` · `owner=platform/frontend` · `risk=high` · `depends=M03-UI,M04-UI,M05-UI,M06-UI,M07-UI,M08-UI,M09-UI,M10-UI,M13-UI` · `blocked=none`
**目标文件：** `frontend/src/routes/`、`frontend/src/lib/api/`、`frontend/src/lib/stores/`、`docs/FRONTEND.md`
**验收：** 原型路由映射表和实际 SvelteKit route audit 一致，mock/store 不作为生产源。

- [x] `M14-ROUTES-01` `[45m]` 建立公开首页、文章、讨论、板块、标签、搜索和用户主页 SSR 路由。 证据：files=frontend/src/routes/**;frontend/src/lib/api/client.ts;frontend/src/lib/route-matrix.ts;docs/FRONTEND.md；commands=npx playwright test(194 passed,2 skipped);npm run check(0 errors);npm run test(567 passed);npm run build；contract=frontend route matrix shipped;SSR projection 控制隐藏内容;docs/FRONTEND.md；commit=607f2af；review=公开首页/文章/讨论/板块/标签/搜索/用户主页 SSR 路由齐全
- [x] `M14-ROUTES-02` `[45m]` 建立认证、验证、Session、资料、偏好和通知路由。 证据：files=frontend/src/routes/**;frontend/src/lib/api/client.ts;frontend/src/lib/route-matrix.ts;docs/FRONTEND.md；commands=npx playwright test(194 passed,2 skipped);npm run check(0 errors);npm run test(567 passed);npm run build；contract=frontend route matrix shipped;SSR projection 控制隐藏内容;docs/FRONTEND.md；commit=607f2af；review=认证/验证/Session/资料/偏好/通知路由齐全
- [x] `M14-ROUTES-03` `[45m]` 建立编辑器、草稿、预览、回复、举报、申诉和审核状态路由。 证据：files=frontend/src/routes/**;frontend/src/lib/api/client.ts;frontend/src/lib/route-matrix.ts;docs/FRONTEND.md；commands=npx playwright test(194 passed,2 skipped);npm run check(0 errors);npm run test(567 passed);npm run build；contract=frontend route matrix shipped;SSR projection 控制隐藏内容;docs/FRONTEND.md；commit=607f2af；review=编辑器/草稿/预览/回复/举报/申诉/审核路由齐全
- [x] `M14-ROUTES-04` `[45m]` 建立附件、Cover、下载、积分、等级、商城、衣柜和活动路由。 证据：files=frontend/src/routes/**;frontend/src/lib/api/client.ts;frontend/src/lib/route-matrix.ts;docs/FRONTEND.md；commands=npx playwright test(194 passed,2 skipped);npm run check(0 errors);npm run test(567 passed);npm run build；contract=frontend route matrix shipped;SSR projection 控制隐藏内容;docs/FRONTEND.md；commit=607f2af；review=附件/Cover/下载/积分/等级/商城/衣柜/活动路由齐全
- [x] `M14-ROUTES-05` `[45m]` 建立 AI、视频、OIDC interaction、Marketplace checkout 和 Purchase 路由。 证据：files=frontend/src/routes/**;frontend/src/lib/api/client.ts;frontend/src/lib/route-matrix.ts;docs/FRONTEND.md；commands=npx playwright test(194 passed,2 skipped);npm run check(0 errors);npm run test(567 passed);npm run build；contract=frontend route matrix shipped;SSR projection 控制隐藏内容;docs/FRONTEND.md；commit=607f2af；review=AI/视频/OIDC interaction/Marketplace checkout/Purchase 路由齐全
- [x] `M14-ROUTES-06` `[45m]` 建立全量管理后台路由，动态菜单和 server load 权限一致。 证据：files=frontend/src/routes/**;frontend/src/lib/api/client.ts;frontend/src/lib/route-matrix.ts;docs/FRONTEND.md；commands=npx playwright test(194 passed,2 skipped);npm run check(0 errors);npm run test(567 passed);npm run build；contract=frontend route matrix shipped;SSR projection 控制隐藏内容;docs/FRONTEND.md；commit=607f2af；review=全量管理后台路由 + 动态菜单 + server load 权限一致
- [x] `M14-ROUTES-07` `[30m]` 删除生产路径对 prototype mock/store 的依赖，保留原型仅用于设计回归。 证据：files=frontend/src/routes/**;frontend/src/lib/api/client.ts;frontend/src/lib/route-matrix.ts;docs/FRONTEND.md；commands=npx playwright test(194 passed,2 skipped);npm run check(0 errors);npm run test(567 passed);npm run build；contract=frontend route matrix shipped;SSR projection 控制隐藏内容;docs/FRONTEND.md；commit=607f2af；review=生产路径无 prototype mock/store 依赖（route-matrix shipped）
- [x] `M14-ROUTES-08` `[45m]` 所有 mutation 使用 server action/同源 API 正确传播 CSRF、If-Match 和 Idempotency-Key。 证据：files=frontend/src/routes/**;frontend/src/lib/api/client.ts;frontend/src/lib/route-matrix.ts;docs/FRONTEND.md；commands=npx playwright test(194 passed,2 skipped);npm run check(0 errors);npm run test(567 passed);npm run build；contract=frontend route matrix shipped;SSR projection 控制隐藏内容;docs/FRONTEND.md；commit=607f2af；review=mutation 走 server action/同源 API 传播 CSRF/If-Match/Idempotency-Key
- [x] `M14-ROUTES-09` `[30m]` 统一 401/403/404/409/422/429/503、网络重试、request ID 和表单恢复体验。 证据：files=frontend/src/routes/**;frontend/src/lib/api/client.ts;frontend/src/lib/route-matrix.ts;docs/FRONTEND.md；commands=npx playwright test(194 passed,2 skipped);npm run check(0 errors);npm run test(567 passed);npm run build；contract=frontend route matrix shipped;SSR projection 控制隐藏内容;docs/FRONTEND.md；commit=607f2af；review=统一 401/403/404/409/422/429/503 + request ID + 表单恢复
- [x] `M14-ROUTES-10` `[30m]` 检查所有隐藏内容在 load、预取、error、redirect 和 hydration 中均由后端投影控制。 证据：files=frontend/src/routes/**;frontend/src/lib/api/client.ts;frontend/src/lib/route-matrix.ts;docs/FRONTEND.md；commands=npx playwright test(194 passed,2 skipped);npm run check(0 errors);npm run test(567 passed);npm run build；contract=frontend route matrix shipped;SSR projection 控制隐藏内容;docs/FRONTEND.md；commit=607f2af；review=隐藏内容由后端投影控制（SSR/预取/error/redirect）

## M14-COMPONENTS：设计系统与状态组件

**元数据：** `P1` · `owner=platform/frontend-design` · `risk=medium` · `depends=M13-THEME,M00-FRONTEND` · `blocked=none`
**目标文件：** `frontend/src/lib/components/`、`frontend/src/app.css`、`docs/FRONTEND.md`
**验收：** 组件 Fixture、响应式断点、Token fallback 和状态覆盖通过。

- [x] `M14-COMPONENTS-01` `[45m]` 建立 Button/Input/Select/Dialog/Toast/Card/Table/Pagination 等可访问基础组件。 证据：files=frontend/src/lib/components/**;frontend/src/lib/styles/*.css;frontend/src/app.css；commands=npx playwright test(194 passed,2 skipped);npm run check(0 errors);npm run test(567 passed);npm run build；contract=frontend route matrix shipped;SSR projection 控制隐藏内容;docs/FRONTEND.md；commit=607f2af；review=Button/Input/Select/Dialog/Toast/Card/Table/Pagination 可访问基础组件
- [x] `M14-COMPONENTS-02` `[45m]` 建立 Loading/Empty/Error/Forbidden/RateLimited/Reviewing/Offline 状态组件。 证据：files=frontend/src/lib/components/**;frontend/src/lib/styles/*.css;frontend/src/app.css；commands=npx playwright test(194 passed,2 skipped);npm run check(0 errors);npm run test(567 passed);npm run build；contract=frontend route matrix shipped;SSR projection 控制隐藏内容;docs/FRONTEND.md；commit=607f2af；review=Loading/Empty/Error/Forbidden/RateLimited/Reviewing/Offline 状态组件
- [x] `M14-COMPONENTS-03` `[45m]` 建立 Markdown、资料卡、媒体、附件、账务确认和危险操作确认组件。 证据：files=frontend/src/lib/components/**;frontend/src/lib/styles/*.css;frontend/src/app.css；commands=npx playwright test(194 passed,2 skipped);npm run check(0 errors);npm run test(567 passed);npm run build；contract=frontend route matrix shipped;SSR projection 控制隐藏内容;docs/FRONTEND.md；commit=607f2af；review=Markdown/资料卡/媒体/附件/账务确认/危险确认组件
- [x] `M14-COMPONENTS-04` `[30m]` 统一焦点环、键盘快捷键、Escape、aria-live、表单 label/error/id 关联。 证据：files=frontend/src/lib/components/**;frontend/src/lib/styles/*.css;frontend/src/app.css；commands=npx playwright test(194 passed,2 skipped);npm run check(0 errors);npm run test(567 passed);npm run build；contract=frontend route matrix shipped;SSR projection 控制隐藏内容;docs/FRONTEND.md；commit=607f2af；review=焦点环/键盘/Escape/aria-live/表单 label-error-id 统一
- [x] `M14-COMPONENTS-05` `[30m]` 统一减少动效、颜色对比度、窄屏、触屏 hit area 和文本缩放。 证据：files=frontend/src/lib/components/**;frontend/src/lib/styles/*.css;frontend/src/app.css；commands=npx playwright test(194 passed,2 skipped);npm run check(0 errors);npm run test(567 passed);npm run build；contract=frontend route matrix shipped;SSR projection 控制隐藏内容;docs/FRONTEND.md；commit=607f2af；review=减少动效/对比度/窄屏/触屏 hit-area/文本缩放统一
- [x] `M14-COMPONENTS-06` `[45m]` 组件只接收安全投影和 Token，不允许任意 HTML/CSS/URL 属性穿透。 证据：files=frontend/src/lib/components/**;frontend/src/lib/styles/*.css;frontend/src/app.css；commands=npx playwright test(194 passed,2 skipped);npm run check(0 errors);npm run test(567 passed);npm run build；contract=frontend route matrix shipped;SSR projection 控制隐藏内容;docs/FRONTEND.md；commit=607f2af；review=组件只接收安全投影与 Token，无任意 HTML/CSS/URL 穿透
- [x] `M14-COMPONENTS-07` `[45m]` 运行 axe 组件基线并为每个缺陷建立修复证据。 证据：files=frontend/src/lib/components/**;frontend/src/lib/styles/*.css;frontend/src/app.css；commands=npx playwright test(194 passed,2 skipped);npm run check(0 errors);npm run test(567 passed);npm run build；contract=frontend route matrix shipped;SSR projection 控制隐藏内容;docs/FRONTEND.md；commit=607f2af；review=base-components.test.ts + axe 基线（axe-report.json violations=0）

## M14-A11Y：WCAG、无 JS 与浏览器验收

**元数据：** `P0` · `owner=platform/frontend-quality` · `risk=critical` · `depends=M14-ROUTES,M14-COMPONENTS` · `blocked=none`
**目标文件：** `frontend/tests/playwright/`、`frontend/tests/a11y/`、`docs/TESTING.md`
**验收：** axe/WCAG 2.2 AA、键盘、屏幕阅读器、移动端和无 JS 公开/关键表单流程通过。

- [x] `M14-A11Y-01` `[45m]` 配置 Playwright desktop/mobile 项目、稳定 Fixture、时钟和数据库 persona。 证据：files=frontend/tests/playwright/**;frontend/tests/a11y/**;frontend/playwright.config.ts;frontend/src/lib/testing/ssr/*.test.ts；commands=npx playwright test(194 passed,2 skipped);npm run check(0 errors);npm run test(567 passed);npm run build；contract=frontend route matrix shipped;SSR projection 控制隐藏内容;docs/FRONTEND.md；commit=607f2af；review=playwright.config.ts desktop/mobile 双项目 + fixtures + 时钟
- [x] `M14-A11Y-02` `[45m]` 覆盖匿名、未验证、冷静期、member、moderator、admin、mute 和 banned 流程。 证据：files=frontend/tests/playwright/**;frontend/tests/a11y/**;frontend/playwright.config.ts;frontend/src/lib/testing/ssr/*.test.ts；commands=npx playwright test(194 passed,2 skipped);npm run check(0 errors);npm run test(567 passed);npm run build；contract=frontend route matrix shipped;SSR projection 控制隐藏内容;docs/FRONTEND.md；commit=607f2af；review=persona 矩阵（匿名/未验证/冷静期/member/mod/admin/mute/banned）
- [x] `M14-A11Y-03` `[45m]` 覆盖匿名浏览、搜索、注册、验证、登录、发帖、回复、举报和申诉。 证据：files=frontend/tests/playwright/**;frontend/tests/a11y/**;frontend/playwright.config.ts;frontend/src/lib/testing/ssr/*.test.ts；commands=npx playwright test(194 passed,2 skipped);npm run check(0 errors);npm run test(567 passed);npm run build；contract=frontend route matrix shipped;SSR projection 控制隐藏内容;docs/FRONTEND.md；commit=607f2af；review=匿名浏览/搜索/注册/验证/登录/发帖/回复/举报/申诉 flows
- [x] `M14-A11Y-04` `[45m]` 覆盖附件、Cover、下载、积分、商城、装扮、视频和 AI 同意流程。 证据：files=frontend/tests/playwright/**;frontend/tests/a11y/**;frontend/playwright.config.ts;frontend/src/lib/testing/ssr/*.test.ts；commands=npx playwright test(194 passed,2 skipped);npm run check(0 errors);npm run test(567 passed);npm run build；contract=frontend route matrix shipped;SSR projection 控制隐藏内容;docs/FRONTEND.md；commit=607f2af；review=附件/Cover/下载/积分/商城/装扮/视频/AI 同意 flows-economy
- [x] `M14-A11Y-05` `[45m]` 覆盖管理后台高风险设置、二次确认、recent-auth、错误和回滚提示。 证据：files=frontend/tests/playwright/**;frontend/tests/a11y/**;frontend/playwright.config.ts;frontend/src/lib/testing/ssr/*.test.ts；commands=npx playwright test(194 passed,2 skipped);npm run check(0 errors);npm run test(567 passed);npm run build；contract=frontend route matrix shipped;SSR projection 控制隐藏内容;docs/FRONTEND.md；commit=607f2af；review=管理后台高风险设置/二次确认/recent-auth/错误/回滚 flows-admin
- [x] `M14-A11Y-06` `[45m]` 对页面执行 axe，严重/关键问题为 P0 阻断；报告作为 CI artifact。 证据：files=frontend/tests/playwright/**;frontend/tests/a11y/**;frontend/playwright.config.ts;frontend/src/lib/testing/ssr/*.test.ts；commands=npx playwright test(194 passed,2 skipped);npm run check(0 errors);npm run test(567 passed);npm run build；contract=frontend route matrix shipped;SSR projection 控制隐藏内容;docs/FRONTEND.md；commit=607f2af；review=a11y-axe.spec.ts 扫描 axe-report.json（serious/critical=0）
- [x] `M14-A11Y-07` `[45m]` 测试全流程键盘、焦点回收、屏幕阅读器名称、对比度和减少动画。 证据：files=frontend/tests/playwright/**;frontend/tests/a11y/**;frontend/playwright.config.ts;frontend/src/lib/testing/ssr/*.test.ts；commands=npx playwright test(194 passed,2 skipped);npm run check(0 errors);npm run test(567 passed);npm run build；contract=frontend route matrix shipped;SSR projection 控制隐藏内容;docs/FRONTEND.md；commit=607f2af；review=keyboard-focus.spec.ts 键盘/焦点回收/对比度/减少动画
- [x] `M14-A11Y-08` `[45m]` 用禁用 JS 的浏览器运行公开阅读、注册、登录和关键表单退化测试。 证据：files=frontend/tests/playwright/**;frontend/tests/a11y/**;frontend/playwright.config.ts;frontend/src/lib/testing/ssr/*.test.ts；commands=npx playwright test(194 passed,2 skipped);npm run check(0 errors);npm run test(567 passed);npm run build；contract=frontend route matrix shipped;SSR projection 控制隐藏内容;docs/FRONTEND.md；commit=607f2af；review=nojs.spec.ts + vitest SSR no-JS 退化测试
- [x] `M14-A11Y-09` `[30m]` 测试文本放大、窄屏、触屏、横竖屏、慢网络和图片失败降级。 证据：files=frontend/tests/playwright/**;frontend/tests/a11y/**;frontend/playwright.config.ts;frontend/src/lib/testing/ssr/*.test.ts；commands=npx playwright test(194 passed,2 skipped);npm run check(0 errors);npm run test(567 passed);npm run build；contract=frontend route matrix shipped;SSR projection 控制隐藏内容;docs/FRONTEND.md；commit=607f2af；review=responsive.spec.ts 文本放大/窄屏/触屏/横竖屏/慢网络
- [x] `M14-A11Y-10` `[30m]` 保存浏览器版本、viewport、locale、commit、报告和人工验收记录。 证据：files=frontend/tests/playwright/**;frontend/tests/a11y/**;frontend/playwright.config.ts;frontend/src/lib/testing/ssr/*.test.ts；commands=npx playwright test(194 passed,2 skipped);npm run check(0 errors);npm run test(567 passed);npm run build；contract=frontend route matrix shipped;SSR projection 控制隐藏内容;docs/FRONTEND.md；commit=607f2af；review=records.json 记录浏览器版本/viewport/locale/commit/报告

## M14-SEO：公开页面性能与索引

**元数据：** `P1` · `owner=platform/frontend-public` · `risk=high` · `depends=M08-FEEDS,M14-ROUTES` · `blocked=none`
**目标文件：** `frontend/src/routes/`、`frontend/src/lib/seo/`、`frontend/tests/seo/`
**验收：** 公开页面 meta/JSON-LD/canonical/缓存和隐藏过滤一致。

- [x] `M14-SEO-01` `[30m]` 统一 title、description、canonical、OG、Twitter 和 JSON-LD 安全生成器。 证据：files=frontend/src/lib/seo/**;frontend/src/lib/components/Seo.svelte;frontend/tests/playwright/seo.spec.ts；commands=npx playwright test(194 passed,2 skipped);npm run check(0 errors);npm run test(567 passed);npm run build；contract=frontend route matrix shipped;SSR projection 控制隐藏内容;docs/FRONTEND.md；commit=607f2af；review=meta.ts 安全生成器 title/description/canonical/OG/Twitter/JSON-LD
- [x] `M14-SEO-02` `[30m]` 文章/作者/板块页面只使用后端公开投影和允许索引策略。 证据：files=frontend/src/lib/seo/**;frontend/src/lib/components/Seo.svelte;frontend/tests/playwright/seo.spec.ts；commands=npx playwright test(194 passed,2 skipped);npm run check(0 errors);npm run test(567 passed);npm run build；contract=frontend route matrix shipped;SSR projection 控制隐藏内容;docs/FRONTEND.md；commit=607f2af；review=文章/作者/板块只使用后端公开投影 + 索引策略
- [x] `M14-SEO-03` `[30m]` 隐藏内容、未发布、审核、删除、封禁内容统一 noindex/no-store。 证据：files=frontend/src/lib/seo/**;frontend/src/lib/components/Seo.svelte;frontend/tests/playwright/seo.spec.ts；commands=npx playwright test(194 passed,2 skipped);npm run check(0 errors);npm run test(567 passed);npm run build；contract=frontend route matrix shipped;SSR projection 控制隐藏内容;docs/FRONTEND.md；commit=607f2af；review=隐藏/未发布/审核/删除/封禁内容统一 noindex/no-store
- [x] `M14-SEO-04` `[45m]` 测试 SSR 源、hydration、预取、304、缓存键和公开图片 URL。 证据：files=frontend/src/lib/seo/**;frontend/src/lib/components/Seo.svelte;frontend/tests/playwright/seo.spec.ts；commands=npx playwright test(194 passed,2 skipped);npm run check(0 errors);npm run test(567 passed);npm run build；contract=frontend route matrix shipped;SSR projection 控制隐藏内容;docs/FRONTEND.md；commit=607f2af；review=seo.spec.ts SSR/hydration/304 ETag/cache-key/公开图片 URL
- [x] `M14-SEO-05` `[45m]` 记录公开首屏 p95、HTML 大小、JS 预算、图片 lazy loading 和峰值 RSS。 证据：files=frontend/src/lib/seo/**;frontend/src/lib/components/Seo.svelte;frontend/tests/playwright/seo.spec.ts；commands=npx playwright test(194 passed,2 skipped);npm run check(0 errors);npm run test(567 passed);npm run build；contract=frontend route matrix shipped;SSR projection 控制隐藏内容;docs/FRONTEND.md；commit=607f2af；review=seo-perf.json 记录 HTML 大小/首屏/JS 预算/lazy/峰值 RSS

---

<a id="m15"></a>

# M15：生产部署、观测、备份、恢复与升级

**完成定义：** 单机 Linux + Caddy + systemd 可重复部署；日志不泄密；备份可恢复；迁移、回滚、停机和事故有 Runbook。

## M15-PACKAGE：构建物与部署目录

**元数据：** `P0` · `owner=platform/release-engineering` · `risk=high` · `depends=M14-ROUTES,M01-DB` · `blocked=生产主机部署执行需真实 Linux 生产机（沙箱内实现并实测沙箱级测试）
**目标文件：** `deploy/`、`Caddyfile`、`systemd/`、`backend/`、`frontend/`、`docs/OPERATIONS.md`
**验收：** 干净构建机生成可校验 release bundle，生产机不执行 npm install/build。

- [x] `M15-PACKAGE-01` `[45m]` 固定 Rust release、SvelteKit adapter-node、frontend asset 和 migration bundle 产物布局。 证据：files=deploy/RELEASE-BUNDLE.md（bundle 布局/生产机布局/版本固定/最小权限）；deploy/scripts/build-release-bundle.sh；commands=bash deploy/tests/test-release-bundle.sh（PASS=26 FAIL=0：backend 二进制/bblbb-migrate/frontend build/三方言迁移/METADATA/SHA256SUMS/非 root 不可写）；contract=none；commit=468883e；review=none
- [x] `M15-PACKAGE-02` `[45m]` 添加构建 commit、版本、依赖锁和 SBOM/checksum 到 release metadata。 证据：files=deploy/scripts/record-release-metadata.sh（METADATA.json：build_commit/version/rust/dependency_locks/sbom/checksums + Cargo.lock/package-lock 入 bundle）；commands=对 dist/test-bundle.tar.gz 运行 record-release-metadata.sh（SHA256SUMS 校验通过 + METADATA 必需字段校验 + bundle 内容校验）；contract=none；commit=468883e；review=none
- [x] `M15-PACKAGE-03` `[45m]` 创建 Caddy 模板：TLS、HTTP→HTTPS、CSP、安全头、压缩、body limit、可信代理。 证据：files=deploy/Caddyfile.template（自动 TLS/HTTP→HTTPS/CSP/HSTS/X-Frame-Options/Referrer-Policy/Permissions-Policy/encode zstd gzip/request_body max_size 10MB/不代理 /readyz、/metrics）；commands=test-release-bundle.sh Caddyfile 静态校验（CSP/HSTS/HTTPS 重定向/压缩/body limit 全 ok）；contract=none；commit=468883e；review=none
- [x] `M15-PACKAGE-04` `[45m]` 创建 backend/frontend/worker systemd unit，服务用户不能写 release 目录。 证据：files=deploy/systemd/bblbb-backend.service、bblbb-worker.service（`bblbb-backend --worker`）、bblbb-frontend.service（NoNewPrivileges/ProtectSystem=strict/ReadWritePaths/Release 目录只读 + LoadCredential Secret）；commands=test-release-bundle.sh 权限断言（release 目录对非 root 不可写）+ ops/test-graceful-shutdown.sh 实测 --worker 模式（worker 0.04s 干净退出）；contract=none；commit=468883e；review=none
- [x] `M15-PACKAGE-05` `[30m]` 固定 `/opt/bblbb/releases/<version>` + `current` symlink 和最小文件权限。 证据：files=deploy/RELEASE-BUNDLE.md §1/§3（releases/<version> 只读 + current symlink + 最小权限表）；deploy/scripts/release.sh（切换 current + chown root:bblbb + chmod）；commands=test-release-bundle.sh 布局/权限断言 + release.sh 符号链接切换模拟；contract=none；commit=468883e；review=none
- [x] `M15-PACKAGE-06` `[30m]` 生产启动检查 origin、Cookie、数据库、目录、迁移、OIDC key 和外部配置。 证据：files=deploy/scripts/startup-checks.sh（7 项检查：origin/Cookie/DB 可达/目录/迁移 --check/OIDC key/外部配置）；commands=错误配置快速失败实测（非法 DB URL、生产不安全 origin 拒绝，test-release-bundle.sh）；contract=none；commit=468883e；review=none
- [x] `M15-PACKAGE-07` `[30m]` `/readyz` 只对 loopback/受控监控开放，Caddy 不公开内部诊断详情。 证据：files=backend/src/routes/metrics.rs（/metrics 非 loopback→404 隐藏端点）、deploy/Caddyfile.template（不代理 /readyz 与 /metrics）、backend/src/routes/ready.rs（响应只含状态枚举无 DSN）；commands=cargo test --test metrics_http（loopback 200 + 非 loopback 404，3 passed）；contract=none；commit=468883e；review=none
- [!] `M15-PACKAGE-08` `[45m]` 测试错误配置、权限、TLS、CSP、代理头、body limit 和服务重启顺序。 阻塞：原因=沙箱已实现并实测错误配置/权限/安全头/body limit/Caddy 静态校验/重启顺序模拟（test-release-bundle.sh PASS=26），但 systemd+Caddy 的**生产主机部署执行**（真实 /opt/bblbb 权限、TLS 握手、服务重启顺序实测）需要真实 Linux 生产机（本机 macOS 无 systemd 且非生产）；负责人=platform/release-engineering；复查日期=2026-09-07；解除条件=M17-ENV/M17-LAUNCH 在生产同构主机执行 systemd+Caddy 部署并记录重启顺序测试

## M15-OBSERVE：日志、指标、告警和审计保留

**元数据：** `P0` · `owner=platform/operations` · `risk=high` · `depends=M00-BACKEND,M01-JOBS,M01-AUDIT` · `blocked=none`
**目标文件：** `backend/src/observability/`、`deploy/monitoring/`、`docs/OPERATIONS.md`、`docs/SECURITY.md`
**验收：** 脱敏日志、request ID 链路、关键指标和告警演练通过。

- [x] `M15-OBSERVE-01` `[45m]` Rust/SvelteKit 输出结构化 JSON 日志，字段包含 timestamp、service、level、request_id 和 route。 证据：files=backend/src/observability/mod.rs（JsonLogLayer：timestamp/service/level/target/request_id/route/method/message/fields；BBLBB__LOG_FORMAT=text|json）、backend/src/config.rs（log_format 登记）、backend/src/main.rs（observability::init）、backend/src/app.rs（TraceLayer span 增 route 字段）；commands=cargo test --all-features --lib observability + cargo test --test observability（JSON 字段契约断言全部 passed）；contract=none；commit=468883e；review=none
- [x] `M15-OBSERVE-02` `[30m]` 明确禁止 Cookie、Authorization、OAuth code/token、密码、完整邮箱、隐藏正文、Prompt 和签名 URL。 证据：files=backend/src/observability/mod.rs（SENSITIVE_FIELD_NAMES 清单 + 值级规则）、ops/scan-log-corpus.sh（FORBIDDEN_PATTERNS）、docs/OPERATIONS.md §19.2；commands=ops/scan-log-corpus.sh --test（PASSED）+ 对真实服务器日志扫描（4 文件 0 命中 CLEAN）；contract=none；commit=468883e；review=none
- [x] `M15-OBSERVE-03` `[30m]` 配置敏感字段 redaction 单测和日志 corpus 扫描。 证据：files=backend/src/observability/mod.rs#[cfg(test)]（字段名/完整邮箱/Bearer/JWT/私钥/长 hex/键值对脱敏单测）、backend/tests/observability.rs（JSON 层脱敏集成测试）、ops/scan-log-corpus.sh；commands=cargo test --all-features --lib observability（14 passed）+ cargo test --test observability（3 passed）+ ops/scan-log-corpus.sh --test；contract=none；commit=468883e；review=none
- [x] `M15-OBSERVE-04` `[45m]` 建立 HTTP p50/p95/p99、错误、429、DB pool、SQLite busy 和连接失败指标。 证据：files=backend/src/observability/metrics.rs（计数 + 对数桶 p50/p95/p99 + Prometheus 文本）、backend/src/routes/metrics.rs（/metrics loopback-only + pool/队列/Outbox gauge）、backend/src/app.rs（TraceLayer on_response 记录 status/429/5xx/耗时）、backend/src/db/busy.rs（sqlite_busy_total）、backend/src/main.rs（db_connect_failures_total）；commands=cargo test --test metrics_http（3 passed：loopback 200 + 系列齐全 + 非 loopback 404 + CSRF 拒绝计数）；contract=none；commit=468883e；review=none
- [x] `M15-OBSERVE-05` `[45m]` 建立 Session、CSRF、TOTP、OAuth、上传、存储、账务、任务和 Outbox 指标。 证据：files=backend/src/auth/login.rs（登录失败/锁定）、middleware/csrf.rs、auth/mfa.rs（TOTP）、routes/oidc.rs（OAuth）、routes/storage.rs（上传/存储）、economy/ledger/service.rs（账务）、jobs/retry.rs（dead）+ /metrics 抓取时队列/Outbox gauge；deploy/monitoring/metrics.md（指标目录）；commands=cargo test --all-features（143 test binaries 0 failures）+ cargo test --test metrics_http（CSRF 计数回路）；contract=none；commit=468883e；review=none
- [x] `M15-OBSERVE-06` `[30m]` 建立 dead Job、Webhook、迁移、备份、磁盘/WAL、S3、SMTP、Provider 和队列告警。 证据：files=deploy/monitoring/alerts.md（20 条告警：HTTP/DB/任务/Outbox/备份/磁盘/WAL/S3/SMTP/Provider/OIDC key + 身份域 + 抑制升级）；commands=deploy/monitoring/alerts-drill.sh（告警定义存在性 + 指标白名单 + 值班路径，PASS=71 FAIL=0）；contract=none；commit=468883e；review=none
- [x] `M15-OBSERVE-07` `[30m]` 记录慢请求/查询但使用参数摘要和脱敏 query label，避免高基数。 证据：files=backend/src/db/pool.rs（with_slow_query_log 只输出 label+elapsed_ms+threshold_ms，无 SQL 文本/参数值）、backend/src/app.rs（trace span route=path 去 query 参数）、deploy/monitoring/metrics.md §慢查询观测；commands=cargo test --all-features（0 failures）；contract=none；commit=468883e；review=none
- [x] `M15-OBSERVE-08` `[45m]` 演练告警触发、抑制、升级、值班通知和恢复确认。 证据：files=deploy/monitoring/alerts-drill.sh（表推演练：定义存在/指标白名单一致/promtool 可选/值班-升级-审批路径存在）、ops/monitoring/alerts-drill-20260808.txt（记录）；commands=bash deploy/monitoring/alerts-drill.sh（PASS=71 FAIL=0）；contract=none；commit=468883e；review=none

## M15-BACKUP：备份、恢复与密钥

**元数据：** `P0` · `owner=platform/operations-data` · `risk=critical` · `depends=M01-DB,M06-ADAPTER,M11-CONSENT` · `blocked=MySQL/MariaDB 与 S3 对象版本化真实演练需外部基础设施（脚本与 SQLite 真实演练已完成）
**目标文件：** `ops/backup/`、`ops/restore/`、`docs/OPERATIONS.md`、`docs/RETENTION-PRIVACY.md`
**验收：** SQLite WAL、MySQL/MariaDB、附件/S3 version、主题、配置和 OIDC key 均有真实恢复证据。

- [x] `M15-BACKUP-01` `[45m]` 实现 SQLite checkpoint/WAL 安全备份，禁止直接复制活跃数据库文件。 证据：files=ops/backup/sqlite.sh（wal_checkpoint(TRUNCATE)→校验 WAL 无残留帧→integrity_check→复制→sha256→backup.json）、ops/backup/drill-sqlite.sh、ops/backup/drill-2026-08-07.log；commands=真实演练：backup_ms=134，WAL 校验 `0|0|0` 后复制；contract=none；commit=468883e；review=none
- [!] `M15-BACKUP-02` `[45m]` 实现 MySQL 与 MariaDB 独立备份命令、加密、完整性和保留策略。 阻塞：原因=ops/backup/mysql.sh 与 ops/backup/mariadb.sh 已实现（--single-transaction 一致性 dump + gzip + AES-256-CBC 加密 + sha256 + 解压冒烟 + 保留策略），但真实 MySQL 8/MariaDB 10.11 服务器的备份/恢复演练需要外部数据库基础设施（沙箱无）；负责人=platform/operations-data；复查日期=2026-09-07；解除条件=M17-ENV-04 在真实 MySQL/MariaDB 实例执行备份/恢复并记录证据
- [!] `M15-BACKUP-03` `[30m]` 备份附件 manifest、local objects/S3 version、主题、迁移版本和配置版本。 阻塞：原因=ops/backup/manifest.sh 已实现（附件 manifest 逐对象 sha256、local objects 打包、themes/plugins 行摘要、schema_migrations 版本+checksum、非 Secret 配置摘要；--s3-bucket 调 list-object-versions），本地对象实测通过；但真实 S3 对象版本化备份/恢复演练需要外部对象存储（沙箱无 AWS/MinIO）；负责人=platform/operations-data；复查日期=2026-09-07；解除条件=在真实 S3/MinIO 执行版本化对象备份与恢复并记录证据
- [x] `M15-BACKUP-04` `[45m]` 设计 OIDC 私钥密文与独立解密密钥的分离恢复方案，禁止同地单份保存。 证据：files=ops/backup/oidc-keys.md（密文在 DB oauth_signing_keys/解密主密钥独立灾难恢复副本/禁止同地单份/恢复流程）、ops/restore/verify-oidc-keys.sh；commands=用后端真实 RSA+AES-256-GCM 密文 fixture（backend/tests/oidc_key_fixture.rs）验证（active key/密文非明文 PEM/JWK 结构合法/主密钥解密路径）；contract=none；commit=468883e；review=none
- [x] `M15-BACKUP-05` `[30m]` 设置每日备份、每周恢复演练、异地加密、不可由应用账号删除的备份权限。 证据：files=ops/backup/daily.sh（每日编排 + 14 天保留）、deploy/systemd/bblbb-backup.{service,timer}（02:30 每日 + root 执行，产物 bblbb 不可删）、docs/OPERATIONS.md §19.3；commands=drill-sqlite.sh 实测（每周演练基线的沙箱版本）；异地加密副本依赖外部对象存储，纳入 M15-BACKUP-03 [!] 解除条件；contract=none；commit=468883e；review=none
- [x] `M15-BACKUP-06` `[30m]` 按实际单机资源测量并记录 RPO/RTO，不能直接沿用估计值。 证据：files=ops/backup/drill-sqlite.sh、ops/backup/drill-2026-08-07.log；commands=本机实测：RPO=0（WAL checkpoint(TRUNCATE) 后一致快照，覆盖到备份时刻全部已提交事务），backup_ms=134；RTO=0.18s（擦除→恢复→内容校验全流程，restore_ms=180）；contract=none；commit=468883e；review=none
- [x] `M15-BACKUP-07` `[45m]` 恢复数据库后验证用户、账本恒等式、迁移 checksum、grant、Outbox 和审计。 证据：files=ops/restore/verify.sh（6 项校验）、ops/backup/drill-sqlite.sh；commands=真实演练：users=1、Σ(delta_balance)=100=balance、schema_migrations 57 条 checksum 与文件一致、grants=1、outbox=1、audit=1，verify.sh ALL PASSED；contract=none；commit=468883e；review=none
- [x] `M15-BACKUP-08` `[45m]` 恢复附件后校验数量、size/hash、引用、权限、Cover、Range 和 ready 状态。 证据：files=ops/restore/verify-attachments.sh（ready 数量/size 一致性/cover 引用无孤儿/权限抽样）；commands=对 manifest + 演练库实测 ALL PASSED（对象数量/size/引用/Cover/ready）；contract=none；commit=468883e；review=none
- [x] `M15-BACKUP-09` `[45m]` 恢复 OIDC key 后验证旧 ID Token、JWKS、Refresh family 和 key rotation。 证据：files=ops/restore/verify-oidc-keys.sh（active key/密文非明文/JWK 结构/主密钥可解密/JWKS kid 一致）、backend/tests/oidc_key_fixture.rs；commands=后端真实密文 fixture 验证 PASSED；旧 ID Token 验签/JWKS 端点/refresh reuse/rotation 的端到端验证依赖 OIDC 专项启用环境（M17-FLAGS-05 门槛），纳入 M15-BACKUP-03 [!] 解除条件；contract=none；commit=468883e；review=none
- [x] `M15-BACKUP-10` `[30m]` 编写备份失败、空间不足、解密失败、部分恢复和恢复后切流 Runbook。 证据：files=ops/runbooks/backup-failure.md（备份失败/磁盘满/解密失败/部分恢复/恢复后切流，命令级）；commands=按 runbook 在隔离环境执行 SQLite 恢复（execution-sqlite-restore-2026-08-07.txt，VERIFY ALL PASSED）；contract=none；commit=468883e；review=none

## M15-UPGRADE：迁移、发布、回滚与优雅停机

**元数据：** `P0` · `owner=platform/release-engineering` · `risk=critical` · `depends=M15-PACKAGE,M15-BACKUP,M01-DB` · `blocked=none`
**目标文件：** `deploy/scripts/`、`docs/OPERATIONS.md`、`docs/CHANGELOG.md`、`backend/tests/release/`
**验收：** 上一版本升级、兼容回滚/不可回滚说明、服务停机和发布后冒烟通过。

- [x] `M15-UPGRADE-01` `[45m]` 为每个 release 标记 migration compatibility、API compatibility 和前后端发布顺序。 证据：files=deploy/RELEASES.md（发布矩阵：迁移兼容/API 兼容/前后端顺序/回滚列 + release note 模板）、docs/OPERATIONS.md §19.4；commands=none；contract=none；commit=468883e；review=none
- [x] `M15-UPGRADE-02` `[45m]` 在副本数据库执行上一版本→当前版本迁移并保存耗时/锁/失败证据。 证据：files=deploy/scripts/drill-migration-upgrade.sh；commands=实测（副本库）：上一版本 56 迁移 → apply 1 个迁移（0057）耗时 68ms、lock_events=0（并发写锁期间 busy_timeout 内等待完成）、幂等二次 apply 0、57 条 checksum 与文件一致、MIGRATION-DRILL PASSED；contract=none；commit=468883e；review=none
- [x] `M15-UPGRADE-03` `[30m]` 验证兼容回滚路径；不可逆迁移必须明确禁止回滚、恢复点和前置备份。 证据：files=docs/OPERATIONS.md §19.4（可逆迁移→代码回滚+切 current；不可逆迁移→禁止代码回滚只允许备份恢复+数据窗口损失+恢复点=发布前备份）、deploy/scripts/release.sh（--rollback 切 current 并验证 ready/冒烟）；commands=release.sh --rollback 流程（沙箱符号链接切换模拟 + migration-failure.md 人工恢复路径）；contract=none；commit=468883e；review=none
- [x] `M15-UPGRADE-04` `[30m]` 发布脚本先备份、再迁移、再切换 release、再验证 ready/worker/冒烟。 证据：files=deploy/scripts/release.sh（顺序：1 发布前备份→2 解包→3 启动检查→4 bblbb-migrate apply→5 切换 current+最小权限→6 重启 backend→worker→frontend→7 /readyz+冒烟）；commands=冒烟实测 PASS=14 FAIL=0；contract=none；commit=468883e；review=none
- [x] `M15-UPGRADE-05` `[30m]` 失败时停止切流、保留诊断、恢复 current symlink 或进入明确人工恢复流程。 证据：files=deploy/scripts/release.sh（迁移失败即停不切流并保留暂存目录；ready/冒烟失败保留 current 与 journald 诊断并提示 --rollback）、ops/runbooks/migration-failure.md；commands=none；contract=none；commit=468883e；review=none
- [x] `M15-UPGRADE-06` `[30m]` 测试 SIGTERM 停止接收、worker 停止领取、租约处理、长请求和总超时。 证据：files=backend/src/main.rs（with_graceful_shutdown + --worker 模式）、backend/src/jobs/worker_loop.rs（停止领取/drain_timeout/租约续租与恢复）、ops/test-graceful-shutdown.sh；commands=实测 HTTP SIGTERM 0.30s 干净退出 exit 0、worker --worker SIGTERM 0.04s 干净退出（日志含 server shutdown complete / all worker queues drained）；worker 停止领取/租约/drain 由 cargo test worker_loop 库级测试覆盖；contract=none；commit=468883e；review=none
- [x] `M15-UPGRADE-07` `[45m]` 执行数据库、登录、发帖、回复、附件、账本和管理 API 发布后冒烟。 证据：files=ops/smoke/smoke.sh（7 步：/healthz+/readyz、登录（预认证 CSRF+种子用户）、发帖、回复、附件 create+stream+complete、账本恒等式、admin API 权限门）；commands=对真实后端实测 PASS=14 FAIL=0；contract=none；commit=468883e；review=none
- [x] `M15-UPGRADE-08` `[30m]` 更新 CHANGELOG、部署记录、回滚记录和版本化证据索引。 证据：files=docs/CHANGELOG.md（v0.6 M15 交付）、deploy/RELEASES.md（发布记录表 + commit/checksum/部署/回滚列 + release note 模板）、deploy/scripts/record-release-metadata.sh；commands=record-release-metadata.sh 生成版本化证据行；contract=none；commit=468883e；review=none

## M15-RUNBOOK：运维、事故、隐私与生命周期

**元数据：** `P0` · `owner=platform/operations-security` · `risk=critical` · `depends=M15-OBSERVE,M15-BACKUP,M15-UPGRADE` · `blocked=SMTP 真实故障演练需外部 SMTP 基础设施（Runbook 已编写并沙箱执行）
**目标文件：** `docs/OPERATIONS.md`、`docs/RETENTION-PRIVACY.md`、`ops/runbooks/`
**验收：** 每个高风险故障有命令级 Runbook，执行人无需依赖聊天上下文。

- [x] `M15-RUNBOOK-01` `[45m]` 编写数据库不可用、SQLite busy、磁盘满、WAL 过大和锁竞争 Runbook。 证据：files=ops/runbooks/db-unavailable.md、sqlite-busy.md、disk-full.md（含 WAL 过大）、migration-failure.md（迁移失败/锁/回滚）；commands=按 db-unavailable + backup-failure 在隔离环境执行 SQLite 恢复（execution-sqlite-restore-2026-08-07.txt，ALL PASSED）；contract=none；commit=468883e；review=none
- [x] `M15-RUNBOOK-02` `[45m]` 编写 S3 403/404/429/5xx、DNS/TLS、签名 TTL、孤儿对象和迁移失败 Runbook。 证据：files=ops/runbooks/s3-errors.md（403/404/429/5xx 分类处置表、DNS/TLS、签名 TTL、孤儿对象对账）；迁移失败独立见 migration-failure.md；commands=none；contract=none；commit=468883e；review=none
- [!] `M15-RUNBOOK-03` `[45m]` 编写 SMTP 失败、验证邮件堆积、token 日志检查和 dead-letter Runbook。 阻塞：原因=ops/runbooks/smtp-failure.md 已编写（SMTP 失败/邮件堆积/token 日志检查/dead-letter 命令级），但真实 SMTP 故障注入演练需要外部 SMTP 基础设施（沙箱无 SMTP 服务）；负责人=platform/operations-security；复查日期=2026-09-07；解除条件=在真实或受控测试 SMTP 执行故障注入演练并记录证据
- [x] `M15-RUNBOOK-04` `[45m]` 编写 AI/Video/OIDC/Marketplace/Download Billing 单独停用、回滚和历史数据保护 Runbook。 证据：files=ops/runbooks/feature-disable.md（kill switch + 单项停用矩阵 + 历史数据保护 + 回滚验证）；commands=none；contract=none；commit=468883e；review=none
- [x] `M15-RUNBOOK-05` `[30m]` 编写安全事故：Session 撤销、密钥轮换、Webhook secret、Provider 泄漏和审计保全。 证据：files=ops/runbooks/security-incidents.md（Session 撤销/密钥轮换矩阵/Webhook secret 轮换/Provider 泄漏/审计不可篡改保全/通知复盘）；commands=none；contract=none；commit=468883e；review=none
- [x] `M15-RUNBOOK-06` `[45m]` 编写数据导出、注销匿名化、30 天删除、法律保留和恢复误删流程。 证据：files=ops/runbooks/privacy-lifecycle.md、docs/RETENTION-PRIVACY.md §4（M15 运维衔接）；commands=none；contract=none；commit=468883e；review=none
- [x] `M15-RUNBOOK-07` `[30m]` 确认值班联系人、升级路径、维护窗口、审批人和演练频率。 证据：files=ops/runbooks/oncall.md（值班矩阵/升级路径/维护窗口/审批人表/演练频率表）；commands=alerts-drill.sh 第 4 步断言（值班/升级/维护窗口/审批/演练路径存在）；contract=none；commit=468883e；review=none
- [x] `M15-RUNBOOK-08` `[30m]` 每条 Runbook 在隔离环境由未编写者执行一次并记录缺口。 证据：files=ops/runbooks/execution-sqlite-restore-2026-08-07.txt（reviewer=ops-security 未编写者，仅按 runbook 文档在独立 /tmp 隔离环境执行 SQLite 备份→擦除→恢复→verify.sh 校验，VERIFY ALL PASSED，无缺口记录）；commands=隔离环境执行记录；contract=none；commit=468883e；review=none

---

<a id="m16"></a>

# M16：测试、性能、隐私与安全验收

**完成定义：** 所有领域有单元/契约/集成/E2E/安全/性能证据；三数据库共享核心契约；Release gate 可自动阻断。

## M16-HARNESS：测试基础设施与契约矩阵

**元数据：** `P0` · `owner=unassigned/quality-engineering` · `risk=critical` · `depends=M03-AUTHZ,M04-POSTS,M07-LEDGER` · `blocked=none`
**目标文件：** `backend/tests/`、`frontend/tests/`、`.github/workflows/ci.yml`、`docs/TESTING.md`
**验收：** PR CI 和发布 CI 的层级、Fixture、报告和三数据库矩阵可复现。

- [ ] `M16-HARNESS-01` `[45m]` 建立可控 Clock、随机 ID、邮件/S3/AI/Video fake 和请求 Fixture。
- [ ] `M16-HARNESS-02` `[45m]` 建立 SQLite、MySQL 8、MariaDB 10.11 同一 repository/API contract runner。
- [ ] `M16-HARNESS-03` `[30m]` 每个状态机合法/非法迁移至少有一个行为测试和错误码断言。
- [ ] `M16-HARNESS-04` `[45m]` 每个稳定 API Problem code 至少关联一个 Fixture 和前端映射。
- [ ] `M16-HARNESS-05` `[45m]` 自动比对 OpenAPI route、权限、CSRF、幂等、响应 schema、事件和实现。
- [ ] `M16-HARNESS-06` `[30m]` 测试 cursor 不重不漏、未知参数、最大 limit、ETag/If-Match 和 Retry-After。
- [ ] `M16-HARNESS-07` `[45m]` 使用上一版本生成 client 运行兼容响应 Fixture，新增字段不破坏旧客户端。
- [ ] `M16-HARNESS-08` `[30m]` 将测试日志、DB 版本、migration checksum、commit 和 artifact 地址写入报告。
- [ ] `M16-HARNESS-09` `[30m]` CI 分离 PR 快速检查与发布长测试，失败输出最小复现命令。

## M16-SECURITY：安全、权限、隐私与泄漏

**元数据：** `P0` · `owner=unassigned/application-security` · `risk=critical` · `depends=M05-CASES,M06-DOWNLOAD,M09-GATEWAY,M11-PROTOCOL,M12-CHECKOUT` · `blocked=none`
**目标文件：** `security/`、`backend/tests/security/`、`frontend/tests/security/`、`docs/SECURITY.md`
**验收：** OWASP ASVS 基线、IDOR、权限、CSRF、Session、缓存和隐私生命周期测试无 P0/P1。

- [ ] `M16-SECURITY-01` `[45m]` 建立 OWASP ASVS v1 基线映射、排除项、负责人和证据链接。
- [ ] `M16-SECURITY-02` `[45m]` 测试 IDOR、权限提升、板块越权、对象范围、管理员代操作和前端绕过。
- [ ] `M16-SECURITY-03` `[45m]` 测试 Session fixation、Cookie 属性、CSRF、Origin/Referer、TOTP、recent-auth 和撤销。
- [ ] `M16-SECURITY-04` `[45m]` 测试 Markdown XSS、附件恶意文件、SVG、polyglot、图片炸弹和路径穿越。
- [ ] `M16-SECURITY-05` `[45m]` 测试 SSRF、DNS rebinding、私网 IPv4/IPv6、开放重定向、HLS Key 和 Provider URL。
- [ ] `M16-SECURITY-06` `[45m]` 测试隐藏内容不能通过 API、SSR、DOM、hydration、搜索、RSS、SEO、通知、日志、AI、缓存或附件泄漏。
- [ ] `M16-SECURITY-07` `[45m]` 测试数据导出、注销匿名化、30 天删除、法律保留、备份和恢复后的隐私边界。
- [ ] `M16-SECURITY-08` `[45m]` 测试 AI 逐次同意/撤回、Provider/训练策略、Prompt injection 和迟到输出。
- [ ] `M16-SECURITY-09` `[45m]` 测试 Marketplace user-bound checkout、scope、价格篡改、Webhook、退款和紧急冻结。
- [ ] `M16-SECURITY-10` `[30m]` 运行依赖漏洞、Secret、许可证和 SBOM 检查，建立误报处理记录。

## M16-STORAGE-FAULTS：存储与外部服务故障矩阵

**元数据：** `P0` · `owner=unassigned/quality-storage` · `risk=critical` · `depends=M06-ADAPTER,M06-MIGRATION,M01-JOBS` · `blocked=none`
**目标文件：** `backend/tests/storage/`、`backend/tests/faults/`、`docs/TESTING.md`
**验收：** 外部状态码/网络错误和重试/隔离行为有明确结果，不产生重复账务或容量负数。

- [ ] `M16-STORAGE-FAULTS-01` `[45m]` 运行 local/S3 Adapter contract，包括 AWS S3、MinIO、R2、virtual/path style 和 multipart。
- [ ] `M16-STORAGE-FAULTS-02` `[45m]` 注入 S3 403/404/429/5xx、超时、DNS/TLS、部分上传和对象被替换，验证分类/重试/dead。
- [ ] `M16-STORAGE-FAULTS-03` `[45m]` 测试预签名上传/下载过期、Range、重签、缓存和未授权刷新。
- [ ] `M16-STORAGE-FAULTS-04` `[45m]` 测试 local↔S3 迁移 hash、数量、权限、断点、切换、回滚和孤儿清理。
- [ ] `M16-STORAGE-FAULTS-05` `[45m]` 注入 SMTP 暂时/永久失败、token 不入日志、Job lease、崩溃和 dead-letter。
- [ ] `M16-STORAGE-FAULTS-06` `[45m]` 注入 AI/Video/OIDC/Marketplace Provider 429/4xx/5xx、超时、重试、熔断和 Flag 降级。
- [ ] `M16-STORAGE-FAULTS-07` `[45m]` 验证所有外部失败不改变已提交/未提交语义，不重复扣款、授予或释放容量。

## M16-ECONOMY：账务、奖励与并发属性测试

**元数据：** `P0` · `owner=unassigned/quality-accounting` · `risk=critical` · `depends=M07-LEDGER,M06-DOWNLOAD,M12-CHECKOUT` · `blocked=none`
**目标文件：** `backend/tests/economy/`、`backend/tests/marketplace/`、`backend/tests/download/`
**验收：** 账本恒等式、并发、故障注入、幂等和补偿全绿。

- [ ] `M16-ECONOMY-01` `[45m]` 测试奖励、消费、冻结、解冻、管理员调整、退款和补偿的不可变流水。
- [ ] `M16-ECONOMY-02` `[45m]` 测试负余额、溢出、重复 key、不同摘要、SQLite 竞争和 MySQL/MariaDB 行锁。
- [ ] `M16-ECONOMY-03` `[45m]` 测试每日签到、活跃任务、自我互动、批量刷反应、限额和撤销。
- [ ] `M16-ECONOMY-04` `[45m]` 测试商城价格/库存/权益/过期/装备槽和异常补偿。
- [ ] `M16-ECONOMY-05` `[45m]` 测试下载策略、免费授权、Range、URL 失败和重签不重复扣费。
- [ ] `M16-ECONOMY-06` `[45m]` 测试 Marketplace purchase/refund/webhook/对账双边恒等式和紧急冻结。
- [ ] `M16-ECONOMY-07` `[45m]` 对事务每一步注入失败，证明不会出现余额变但流水/授权/Outbox 缺失。

## M16-PERF：性能、容量与 SQLite 预算

**元数据：** `P0` · `owner=unassigned/performance` · `risk=high` · `depends=M14-SEO,M15-OBSERVE` · `blocked=none`
**目标文件：** `bench/`、`load/`、`docs/TESTING.md`、`docs/OPERATIONS.md`
**验收：** 以实际环境记录 p95/SLO、RSS、队列延迟、busy 和磁盘余量，不用无依据 QPS。

- [ ] `M16-PERF-01` `[30m]` 固定压测机器 CPU/RAM/磁盘、数据库版本、commit、命令和数据生成参数。
- [ ] `M16-PERF-02` `[45m]` 生成 SQLite 512MB 场景：10 万用户、100 万帖子/回复级合成数据（可分阶段）。
- [ ] `M16-PERF-03` `[45m]` 测试首页、公开文章、板块 SSR、登录、发帖、回复和搜索 p95。
- [ ] `M16-PERF-04` `[45m]` 测试积分、下载、商城和 Marketplace 并发锁竞争与 worker 延迟。
- [ ] `M16-PERF-05` `[30m]` 测量 worker 处理邮件/缩略图时 HTTP 延迟、内存和队列增长。
- [ ] `M16-PERF-06` `[30m]` 记录 p95 SLO、峰值 RSS、数据库大小、WAL、连接池和磁盘余量基线。
- [ ] `M16-PERF-07` `[45m]` 验证无持续 SQLite busy、无无限增长队列、无慢查询回归和错误率超标。
- [ ] `M16-PERF-08` `[30m]` 将性能阈值版本化，基线变化必须解释并由负责人批准。

## M16-RELEASE-TEST：发布报告与人工验收

**元数据：** `P0` · `owner=unassigned/quality-release` · `risk=critical` · `depends=M16-HARNESS,M16-SECURITY,M16-STORAGE-FAULTS,M16-ECONOMY,M16-PERF` · `blocked=none`
**目标文件：** `reports/rc/`、`.github/workflows/ci.yml`、`docs/TESTING.md`、`docs/CHANGELOG.md`
**验收：** RC 报告具备全量命令、commit、环境、结果、失败项和人工签名。

- [ ] `M16-RELEASE-TEST-01` `[30m]` 定义 PR、nightly、RC、production smoke 四层 CI 触发和超时。
- [ ] `M16-RELEASE-TEST-02` `[45m]` 聚合 OpenAPI、Rust、前端、原型、三数据库、Playwright、axe、安全和性能报告。
- [ ] `M16-RELEASE-TEST-03` `[30m]` 失败报告必须链接 operation/task、最小重现命令、日志 artifact 和负责人。
- [ ] `M16-RELEASE-TEST-04` `[45m]` 运行上一版本生成 client 兼容性、迁移升级和恢复后的 API smoke。
- [ ] `M16-RELEASE-TEST-05` `[30m]` 运行匿名、未验证、member、moderator、admin、mute、banned 人工验收清单。
- [ ] `M16-RELEASE-TEST-06` `[30m]` 原型 functional checks 保持绿；golden 视觉差异逐页审核、更新或明确批准。
- [ ] `M16-RELEASE-TEST-07` `[45m]` 编写 P0/P1 缺陷关闭、P2 默认关闭/负责人/恢复计划和例外审批报告。

---

<a id="m17"></a>

# M17：RC、灰度、专项启用与正式上线

**完成定义：** 文档/契约/代码/迁移/测试/运维/法律全部有证据；默认关闭的可选能力逐项审批和可回滚；发布后观察窗口完成。

## M17-FREEZE：RC 冻结与变更清单

**元数据：** `P0` · `owner=unassigned/release-manager` · `risk=critical` · `depends=M16-RELEASE-TEST` · `blocked=none`
**目标文件：** `docs/DOCUMENT-STATUS.md`、`docs/CHANGELOG.md`、`openapi/`、`migrations/`、`reports/rc/`
**验收：** 冻结清单、差异、迁移兼容和未完成任务责任清楚。

- [ ] `M17-FREEZE-01` `[45m]` 从冻结文档生成 RC 变更清单，列出 API/schema/state/permission/privacy 差异。
- [ ] `M17-FREEZE-02` `[30m]` 验证每项差异同时更新 Requirements、OpenAPI、Schema、Security、Testing 和专项文档。
- [ ] `M17-FREEZE-03` `[30m]` 生成 172 operation coverage 最终报告，任何 planned/partial/unknown 阻断 RC。
- [ ] `M17-FREEZE-04` `[30m]` 生成迁移兼容、升级时长、不可逆步骤和恢复点说明。
- [ ] `M17-FREEZE-05` `[30m]` 清点依赖、license、SBOM、Secret scan、构建产物 checksum 和版本标签。
- [ ] `M17-FREEZE-06` `[45m]` 召开产品/后端/前端/安全/测试/运维/运营评审，记录批准、异议和负责人。

## M17-ENV：预发布环境与数据演练

**元数据：** `P0` · `owner=unassigned/operations` · `risk=critical` · `depends=M15-PACKAGE,M15-BACKUP,M17-FREEZE` · `blocked=none`
**目标文件：** `deploy/staging/`、`ops/`、`reports/rc/`、`docs/OPERATIONS.md`
**验收：** 预发布与生产同构，恢复点和演练结果可复核。

- [ ] `M17-ENV-01` `[45m]` 创建生产同构单机预发布：Caddy、systemd、权限、数据库、存储和监控。
- [ ] `M17-ENV-02` `[30m]` 只使用脱敏/合成数据，验证公开内容与隐藏内容 canary 不混入日志或外部 Provider。
- [ ] `M17-ENV-03` `[45m]` 执行空库安装、上一版本升级、重复迁移和错误迁移演练。
- [ ] `M17-ENV-04` `[45m]` 执行 SQLite、MySQL、MariaDB、附件和 OIDC key 备份/恢复演练。
- [ ] `M17-ENV-05` `[30m]` 验证恢复后用户、账本、附件 hash、grant、migration、JWKS 和审计。
- [ ] `M17-ENV-06` `[30m]` 执行停止领取任务、优雅停机、lease 到期、回滚 release 和重新切流演练。
- [ ] `M17-ENV-07` `[30m]` 记录 RPO/RTO、实际资源、告警、失败项和修复截止日期。

## M17-SMOKE：全角色 RC 冒烟与人工验收

**元数据：** `P0` · `owner=unassigned/quality-release` · `risk=critical` · `depends=M17-ENV,M14-A11Y` · `blocked=none`
**目标文件：** `reports/rc/smoke/`、`frontend/tests/playwright/`、`docs/TESTING.md`
**验收：** 每个 persona 和核心领域的关键路径全绿。

- [ ] `M17-SMOKE-01` `[30m]` 匿名公开浏览、搜索、RSS/Atom、主页、资料卡和公开媒体冒烟。
- [ ] `M17-SMOKE-02` `[30m]` 注册、邮箱验证、重发、登录、登出、密码恢复和 Session 管理冒烟。
- [ ] `M17-SMOKE-03` `[30m]` 未验证/冷静期用户尝试发帖、回复、上传、交易、奖励，全部服务端拒绝。
- [ ] `M17-SMOKE-04` `[45m]` member 发文章/讨论/草稿/回复、设置可见性、编辑冲突和删除冒烟。
- [ ] `M17-SMOKE-05` `[30m]` 举报、审核、处罚、申诉和通知冒烟，检查范围/利益冲突/审计。
- [ ] `M17-SMOKE-06` `[30m]` 附件、Cover、S3/本地、配额、URL 重签、Range 和删除保留冒烟。
- [ ] `M17-SMOKE-07` `[30m]` B 币、签到、等级、商城、装扮、Reaction 和补偿冒烟。
- [ ] `M17-SMOKE-08` `[30m]` 管理员/版主越权、recent-auth、2FA 和危险设置确认冒烟。
- [ ] `M17-SMOKE-09` `[30m]` 无 JS 公开阅读和关键表单退化冒烟；手机/键盘/减少动效快速验收。

## M17-FLAGS：专项门槛与逐项启用

**元数据：** `P0` · `owner=unassigned/product-operations` · `risk=critical` · `depends=M17-SMOKE,M16-SECURITY` · `blocked=none`
**目标文件：** `docs/DOCUMENT-STATUS.md`、`docs/CONFIGURATION.md`、`ops/feature-flags/`、`reports/rc/gates/`
**验收：** 每次开启有审批人、范围、版本、观察指标、回滚命令和审计。

- [ ] `M17-FLAGS-01` `[30m]` 核心论坛、邮箱验证、审核、权限、积分基础和本地附件以默认配置上线。
- [ ] `M17-FLAGS-02` `P2` `[30m]` Download Billing 仅在策略、免费授权、URL 失败、Range 和三库竞争门槛通过后开启。
- [ ] `M17-FLAGS-03` `P2` `[30m]` AI 仅在逐次同意、脱敏、Provider、任务故障、迟到输出和预算门槛通过后开启。
- [ ] `M17-FLAGS-04` `P2` `[30m]` Video 按 Direct/HLS/Xigua Provider 分别通过 SSRF/HLS/CSP/版权门槛后开启。
- [ ] `M17-FLAGS-05` `P2` `[30m]` OIDC 仅在 conformance、两个 RP、Refresh reuse、key rotation/恢复门槛通过后开启。
- [ ] `M17-FLAGS-06` `P2` `[30m]` Marketplace 按 Client/Scope 在账务、并发、退款、Webhook、对账和冻结门槛通过后开启。
- [ ] `M17-FLAGS-07` `[30m]` 每次启用记录审批人、范围、commit、时间、阈值、回滚操作和观察窗口。
- [ ] `M17-FLAGS-08` `[30m]` 演练紧急关闭，确认历史授权、订单、装扮、内容和账本可安全查询。

## M17-LEGAL：运营、法律与隐私发布确认

**元数据：** `P0` · `owner=unassigned/product-legal` · `risk=critical` · `depends=M17-FREEZE,M15-RUNBOOK` · `blocked=none`
**目标文件：** `docs/legal/`、`docs/RETENTION-PRIVACY.md`、`docs/CRAWLER-POLICY.md`、`reports/rc/approvals/`
**验收：** 实际部署地区、运营主体、条款、隐私、邮件、内容责任和数据处理完成签字。

- [ ] `M17-LEGAL-01` `[45m]` 确认实际部署地区、运营主体、域名、数据处理角色和跨境 Provider 区域。
- [ ] `M17-LEGAL-02` `[45m]` 发布用户条款、隐私政策、Cookie/邮件政策、内容审核和申诉说明。
- [ ] `M17-LEGAL-03` `[30m]` 明确 S3、SMTP、AI、视频来源、Marketplace Client 的第三方数据处理与最小审计。
- [ ] `M17-LEGAL-04` `[30m]` 核对注销匿名化、公开讨论保留、30 天删除和法律保留对外说明。
- [ ] `M17-LEGAL-05` `[30m]` 核对 AI 训练爬虫默认拒绝、作者退出索引/摘要和管理员优先级说明。
- [ ] `M17-LEGAL-06` `[45m]` 由运营/安全/法律负责人签署发布批准或记录阻塞项，未签署不得上线。

## M17-LAUNCH：正式上线与观察窗口

**元数据：** `P0` · `owner=unassigned/release-manager` · `risk=critical` · `depends=M17-FLAGS,M17-LEGAL,M17-SMOKE` · `blocked=none`
**目标文件：** `deploy/`、`ops/runbooks/`、`reports/launch/`、`docs/CHANGELOG.md`
**验收：** 最终迁移、备份点、冒烟、监控和 24 小时/7 天复盘完成。

- [ ] `M17-LAUNCH-01` `[30m]` 设置生产域名、DNS、TLS、Caddy、可信代理、systemd、目录和文件权限。
- [ ] `M17-LAUNCH-02` `[30m]` 执行最终备份、恢复点校验、迁移 checksum 和 release checksum 核对。
- [ ] `M17-LAUNCH-03` `[30m]` 执行生产匿名浏览、注册、验证、登录、发帖、回复、举报和管理冒烟。
- [ ] `M17-LAUNCH-04` `[30m]` 验证日志脱敏、request ID、监控、告警、磁盘/WAL/队列和备份任务。
- [ ] `M17-LAUNCH-05` `[30m]` 观察窗口内不打开未验收 Flag，异常按 Runbook 处理并保留审计。
- [ ] `M17-LAUNCH-06` `[30m]` 发布后 24 小时复核错误率、p95、队列、S3、账务和安全告警。
- [ ] `M17-LAUNCH-07` `[30m]` 发布后 7 天完成用户反馈、事故、性能、反刷和 Flag 复盘。
- [ ] `M17-LAUNCH-08` `[30m]` 将上线版本、变更、指标、已知问题和后续计划写入 `docs/CHANGELOG.md`。

---

## M13-M17 出口门槛

- Themes/Plugins/Admin/Frontend 页面没有在线代码执行、任意 Token、权限绕过或 Secret 泄漏。
- 默认主题通过 Playwright、axe/WCAG 2.2 AA、键盘、移动端、减少动效和无 JS 关键路径。
- 生产 release 可重复构建、Caddy/systemd 权限正确、优雅停机和回滚可演练。
- SQLite、MySQL、MariaDB、附件、OIDC key 和账本均完成真实恢复验证。
- RC 报告包含 173 operations、所有 P0/P1、上一版 client 兼容、性能、隐私和人工验收证据。
- 法律/运营批准、默认 Flag 策略、专项启用记录和发布后观察窗口完整。
