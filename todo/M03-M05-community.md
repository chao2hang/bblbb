# M3-M5：用户、权限、内容与社区治理

> 总索引：[`../TODO.md`](../TODO.md)
> 通用规则和证据格式见 [`M00-M02-foundation.md`](M00-M02-foundation.md)。
> 本文件所有 checkbox 都是唯一叶子任务；工作包标题和出口门槛不重复计数。

---

<a id="m3"></a>

# M3：用户资料、授权、板块与标签

**完成定义：** 用户公开/私有投影严格分离；角色和对象授权只有 Rust 裁决；板块、标签和搜索仓储在三数据库行为一致。

## M03-SCHEMA：用户、角色与板块数据模型

**元数据：** `P0` · `owner=backend-users` · `risk=high` · `depends=M02-UX` · `blocked=none`
**目标文件：** `migrations/{sqlite,mysql,mariadb}/`、`backend/src/users/`、`backend/src/authz/`、`backend/src/boards/`
**验收：** 三数据库空库/升级迁移、约束和 repository contract 通过。

- [x] `M03-SCHEMA-01` `P0` `[45m]` 新增用户资料、隐私设置、展示偏好、等级缓存和 profile revision 字段迁移。证据：files=migrations/{sqlite,mysql,mariadb}/0019_profile.sql,backend/tests/profile_schema.rs,docs/SCHEMA.md；commands=cargo fmt --check 通过; cargo clippy --all-features --all-targets 0 警告; cargo test --all-features 全绿（含 profile_schema 5 项 + migration_equivalence 4 项）; make check（迁移等价 + lifecycle + Roadmap 783 叶子 OK、OpenAPI 183/183）; contract=users 新列：level（默认 1，可重建等级缓存）、level_updated_at（NULL=未计算）、avatar_attachment_id（软引用附件）、signature、last_login_at、delete_requested_at、deleted_at；user_preferences（timezone 默认 UTC/locale 默认 zh-CN/theme_name/notification_json，行首访惰性创建，FK 级联）；user_privacy（email_visible_to 默认 nobody、profile_visible_to 默认 everyone，CHECK 约束三库强制）；profile_revisions（UNIQUE(user_id,revision)、changes_json、actor_user_id，FK 级联，资料写操作同事务写 revision）; commit=bb385cb; review=三库迁移等价 + 列契约/默认值/CHECK/唯一/级联 5 项测试 + make check 全绿
- [x] `M03-SCHEMA-02` `P0` `[30m]` 新增头像与 Cover 的稳定 attachment_id 引用，禁止保存远程 URL 或签名 URL。证据：files=migrations/{sqlite,mysql,mariadb}/0020_attachment_refs.sql,backend/tests/profile_schema.rs,docs/SCHEMA.md；commands=cargo fmt --check 通过; cargo clippy --all-features --all-targets 0 警告; cargo test --all-features 全绿（profile_schema 6 项 + migration_equivalence 4 项 + migration_lifecycle 8 项）; make check（迁移等价/lifecycle + Roadmap 783 叶子 OK、OpenAPI 183/183）; contract=users 新列 cover_attachment_id（TEXT NULL 软引用）；头像/Cover 引用约定：仅存附件 UUID（ProfileCoverSet.attachment_id format: uuid），禁止远程 URL/签名 URL——来源与格式校验在 M3-PROFILE 服务层，attachments 表（M6 存储）落地后补外键；测试验证两列存在、UUID 往返可读、未设置默认 NULL; commit=5dfcf8b; review=三库迁移等价 + profile_schema 6 项 + make check 全绿
- [x] `M03-SCHEMA-03` `P0` `[45m]` 新增 roles、permissions、role_permissions 和全局 user_role_assignments 迁移。证据：files=migrations/{sqlite,mysql,mariadb}/0021_rbac.sql,backend/tests/rbac_schema.rs；commands=cargo fmt --check 通过; cargo clippy --all-features --all-targets 0 警告; cargo test --all-features 全绿（rbac_schema 5 项 + migration_equivalence 4 项）; make check（迁移等价/lifecycle + Roadmap 783 叶子 OK、OpenAPI 183/183）; contract=roles（name 唯一、is_system 内置角色、不存权限 JSON 避免双事实来源）；permissions（name 唯一对应 x-permission、risk_level normal/sensitive/system 默认 normal + CHECK 三库强制、is_system）；role_permissions（复合主键 (role_id,permission_id)，删角色/权限级联）；user_roles（复合主键 (user_id,role_id) 全局 assignment，granted_by/granted_at/expires_at 可空永久，删用户级联；板块级见 SCHEMA-04 board_role_assignments）；SCHEMA.md §5 落地; commit=f7e1004; review=三库迁移等价 + rbac_schema 5 项 + make check 全绿
- [x] `M03-SCHEMA-04` `P0` `[45m]` 新增 boards、board_roles 和带有效期的 board_role_assignments 迁移。证据：files=migrations/{sqlite,mysql,mariadb}/0022_board_roles.sql,backend/tests/board_schema.rs,docs/SCHEMA.md；commands=cargo fmt --check 通过; cargo clippy --all-features --all-targets 0 警告; cargo test --all-features 全绿（board_schema 3 项 + migration_equivalence 4 项）; make check（迁移等价/lifecycle + Roadmap 783 叶子 OK、OpenAPI 183/183）; contract=boards 新列：parent_id（软自引用层级，ALTER 不能带 FK，层级/环路校验在服务层）、visibility（public/members/restricted/hidden 默认 public + CHECK）、posting_mode（normal/approval/readonly/closed 默认 normal + CHECK）；board_roles（复合主键 (board_id,role_id)——板块启用哪些角色，删板块/角色级联）；board_role_assignments（id PK + UNIQUE(board_id,user_id,role_id)、granted_by/granted_at/expires_at 可空=永久，删板块/用户/角色级联）；SCHEMA.md §6 同步（board_roles 落地，父节点软引用说明）; commit=50006ba; review=三库迁移等价 + board_schema 3 项（新列/默认值/CHECK/复合主键/UNIQUE/级联）+ make check 全绿
- [x] `M03-SCHEMA-05` `P1` `[30m]` 新增 tags、tag_groups 和 board/tag 关联迁移及 slug 唯一约束。证据：files=migrations/{sqlite,mysql,mariadb}/0023_tags.sql,backend/tests/tags_schema.rs,docs/SCHEMA.md；commands=cargo fmt --check 通过; cargo clippy --all-features --all-targets 0 警告; cargo test --all-features 全绿（tags_schema 4 项 + migration_equivalence 4 项）; make check（迁移等价/lifecycle + Roadmap 783 叶子 OK、OpenAPI 183/183）; contract=tag_groups（slug 全局唯一、sort_order 默认 0）；tags 演进（0003 骨架）：新增 group_id（软引用 tag_groups，ALTER 不能带 FK）、slug（可空，非空时全局唯一，存量行为 NULL，服务层写入时必填）、description 默认 ''、color 可空，usage_count 保留为可重建缓存（listTags 已读取）；board_tags（复合主键 (board_id,tag_id)，删板块/标签级联，与 board_roles 对称）；post_tags 属 M04-SCHEMA-05；SCHEMA.md §6 同步; commit=e14e08c; review=三库迁移等价 + tags_schema 4 项（分组 slug 唯一/标签默认值与 slug 非空唯一/软引用/board_tags 复合主键与级联）+ make check 全绿
- [x] `M03-SCHEMA-06` `P0` `[30m]` 为 role、permission、board 和 assignment 的删除/停用语义设置外键与应用约束。证据：files=migrations/{sqlite,mysql,mariadb}/0024_delete_semantics.sql,backend/tests/delete_semantics.rs,docs/SCHEMA.md；commands=cargo fmt --check 通过; cargo clippy --all-features --all-targets 0 警告; cargo test --all-features 全绿（delete_semantics 5 项 + migration_equivalence 4 项 + migration_lifecycle 8 项，共 56 套件）; make check（迁移等价/lifecycle + Roadmap 783 叶子 OK、OpenAPI 183/183）; contract=boards 软删除（deleted_at 默认 NULL）+ 停用（is_active=0）+ 活跃投影 is_active=1 AND deleted_at IS NULL + 索引 (parent_id,sort_order)/(visibility,deleted_at)；应用约束：roles/permissions 的 is_system=1 不可删除/改名（服务层 M03-AUTHZ 强制；数据库无触发器，测试锁定"数据库不背锅"）、非系统删除级联 role_permissions/user_roles/board_roles/board_role_assignments；boards 存在子板块禁止硬删除（服务层）、删除级联 board_roles/board_role_assignments/board_tags；assignments 的 granted_by 软引用删除置 NULL、expires_at 可空=永久、过期按未生效（M03-AUTHZ-03）；SCHEMA.md §5/§6 同步; commit=97e7a83; review=三库迁移等价 + delete_semantics 5 项（软删/停用投影与索引/删角色级联四表/删权限级联/删板块级联三表/系统行物理可删证明）+ make check 全绿
- [x] `M03-SCHEMA-07` `P0` `[45m]` 编写三数据库 schema 等价 Fixture，覆盖唯一性、过期 assignment 和非法状态。证据：files=backend/tests/schema_fixture.rs,migrations/{sqlite,mysql,mariadb}/0025_board_checks.sql,docs/SCHEMA.md,.github/workflows/ci.yml；commands=cargo fmt --check 通过; cargo clippy --all-features --all-targets 0 警告; cargo test --all-features 全绿（schema_fixture SQLite 流 + migration_equivalence 4 项 + migration_lifecycle 8 项，共 57 套件）; make check（迁移等价/lifecycle + Roadmap 783 叶子 OK、OpenAPI 183/183）; contract=Fixture 覆盖唯一性（tag_groups.slug/tags.slug 非空/board_roles 复合主键/board_role_assignments UNIQUE(board,user,role)/role_permissions/user_roles/post_tags 复合主键）、过期 assignment（expires_at 可空=永久，过期按未生效过滤但保留行）、非法状态（boards.visibility/posting_mode、permissions.risk_level、user_privacy.email_visible_to、posts.status 的 CHECK 三库统一拒绝）、外键完整性（board_tags/role_permissions/board_role_assignments 悬空引用三库统一拒绝）；Fixture 暴露 mysql/mariadb 0022 缺 boards CHECK 的真实缺口，0025 补齐（MySQL 8 3819/MariaDB 4025 分类断言，sqlite 留注释版保持版本平行）；CI mysql-family 任务新增 schema_fixture; commit=51e285c; review=三库迁移等价 + schema_fixture（SQLite 流全过，mysql/mariadb 遵循 auth_crossdb 模式）+ make check 全绿
- [x] `M03-SCHEMA-08` `P1` `[30m]` 同步 `docs/SCHEMA.md`、状态枚举、事件目录和迁移清单。证据：files=docs/SCHEMA.md,docs/STATE-MACHINES.md,docs/EVENT-CATALOG.md；commands=make check（事件目录 OK 22/22、Roadmap 783 叶子 OK、OpenAPI 183/183、TS types 可复现、svelte 0 error）; contract=迁移清单 0001-0025 每版本一行（文件/内容），已发布迁移不可修改、修正新增版本（0025 补齐 0022 缺 CHECK 为例）；§6 boards 列集与 0022/0024 物理列对齐，settings_json/created_by 标注为目标模型由 M03-BOARDS 落地；状态机新增 Board 稳定枚举（visibility/posting_mode/活跃=is_active=1 AND deleted_at IS NULL）与 Authorization 枚举（risk_level/is_system/expires_at 可空永久）；事件目录只收已实现事件（check-event-catalog.rb 强制目录与 events.rs 一致）; commit=ad2010c; review=make check 全绿（事件/路线图/OpenAPI/TS/前端）

## M03-PROFILE：资料投影、隐私与注销匿名化

**元数据：** `P0` · `owner=backend-users` · `risk=high` · `depends=M03-SCHEMA` · `blocked=none`
**目标文件：** `backend/src/users/`、`backend/src/routes/users/`、`backend/tests/users/`、`docs/RETENTION-PRIVACY.md`
**验收：** `Users` tag 的公开、本人和管理投影泄漏测试通过。

- [x] `M03-PROFILE-01` `P0` `[30m]` 定义 public profile、Me 和 admin user 三套显式 DTO，不复用数据库实体序列化。证据：files=backend/src/users/{mod,dto}.rs,backend/src/lib.rs,backend/src/routes/{users,auth}.rs,backend/tests/user_dto.rs,openapi/openapi.yaml,frontend/src/lib/api/{types,client}.ts,frontend/src/lib/api/generated/v1/types.ts,frontend/src/routes/users/[username]/+page.svelte；commands=cargo fmt --check 通过; cargo clippy --all-features --all-targets 0 警告; cargo test --all-features 全绿（user_dto 3 项，共 58 套件）; npm test 185 通过; make check（OpenAPI 183/183、TS types 可复现、svelte-check 0 error、事件目录 22/22、Roadmap 783 叶子 OK）; contract=PublicProfile（id/username/display_name/bio/level/avatar_attachment_id/signature/created_at 严格公开 allowlist，不含邮箱/Session/IP/处罚/审计）；Me（本人投影，Me::from_session 显式构建）；AdminUser（管理投影含 status/删除注销时间，不含凭据）；OpenAPI PublicUser/Me/AdminUser schema 对齐（可空字段 3.1 数组语法），getAdminUser 200 改挂 AdminUser；前端 User 类型 Omit 修复 display_name 可空交集、PublicProfile 接入 getUser; commit=861c411; review=user_dto 3 项（公开键集精确/Me 本人字段/AdminUser 无凭据）+ 全量测试 + make check 全绿
- [x] `M03-PROFILE-02` `P0` `[30m]` 为公开 DTO 建立字段 allowlist，排除邮箱、IP、Session、内部处罚、私有资产和审计信息。证据：files=backend/src/users/dto.rs,backend/tests/user_dto.rs,backend/tests/public_profile_leak.rs,docs/SCHEMA.md；commands=cargo fmt --check 通过; cargo clippy --all-features --all-targets 0 警告; cargo test --all-features 全绿（public_profile_leak 2 项 + user_dto 4 项，共 59 套件）; make check（OpenAPI 183/183、TS types 可复现、svelte-check 0 error、事件目录 22/22、Roadmap 783 叶子 OK）; contract=PUBLIC_PROFILE_ALLOWLIST 精确八字段：id/username/display_name/bio/level/avatar_attachment_id/signature/created_at；公开端点响应键集 ⊆ allowlist 且文本不含敏感值（邮箱/封禁状态/IP/session/password_hash/audit/sanction/delete_requested_at/last_login_at）；deleted 用户 404 且不含邮箱（instance 按 RFC 7807 回显请求路径属请求方自供信息不算泄漏）；SCHEMA.md §4 落地; commit=44bfb57; review=user_dto 键集==allowlist 常量 + 泄漏端到端 2 项 + make check 全绿
- [~] `M03-PROFILE-03` `P1` `[45m]` 实现昵称、简介、签名、时区、主题偏好和隐私设置读取与更新。
- [ ] `M03-PROFILE-04` `P0` `[30m]` 所有更新使用版本/`If-Match`，并校验长度、Unicode、链接和富文本禁用规则。
- [ ] `M03-PROFILE-05` `P1` `[30m]` 实现公开主页和作者资料卡投影，Cover 只返回稳定内容端点引用。
- [ ] `M03-PROFILE-06` `P0` `[30m]` 用户不存在、注销、封禁和 Cover 不可用时返回不泄漏内部原因的安全降级投影。
- [ ] `M03-PROFILE-07` `P0` `[45m]` 实现注销匿名化：保留公开讨论，替换作者标识并断开可识别资料关系。
- [ ] `M03-PROFILE-08` `P0` `[45m]` 实现注销请求、冷却/取消、执行 Job、法律保留例外和不可删除审计。
- [ ] `M03-PROFILE-09` `P0` `[45m]` 测试 API、SSR、Hover Card、日志和缓存均不泄漏私有用户字段。
- [ ] `M03-PROFILE-10` `P1` `[30m]` 更新 Users operation coverage 的 handler、权限、测试和状态。

## M03-AUTHZ：RBAC、板块范围与对象授权

**元数据：** `P0` · `owner=unassigned/security-backend` · `risk=critical` · `depends=M03-SCHEMA,M03-PROFILE` · `blocked=none`
**目标文件：** `backend/src/authz/`、`backend/src/middleware/authz*`、`backend/tests/authz/`、`docs/PERMISSION-MATRIX.md`
**验收：** persona × role × board × object 权限矩阵在 API 层通过；不存在仅前端授权。

- [ ] `M03-AUTHZ-01` `P0` `[30m]` 实现 `resource.action` 权限注册表，并拒绝数据库中的未知权限名。
- [ ] `M03-AUTHZ-02` `P0` `[45m]` 实现 administrator、global moderator、board moderator、member 和自定义角色聚合。
- [ ] `M03-AUTHZ-03` `P0` `[30m]` 实现带生效/到期时间的全局与板块 assignment 实时判断。
- [ ] `M03-AUTHZ-04` `P0` `[45m]` 定义动作授权输入：actor、账号状态、角色、board、resource owner、resource state 和 policy version。
- [ ] `M03-AUTHZ-05` `P0` `[45m]` 为 Handler 建立统一 require-action + require-object-scope 调用模式，默认拒绝。
- [ ] `M03-AUTHZ-06` `P0` `[30m]` 让未验证、冷静期、restricted、mute、board_mute 和 banned 状态实时参与授权。
- [ ] `M03-AUTHZ-07` `P0` `[30m]` 规定管理员/版主读取隐藏内容必须显式使用管理投影并记录理由和审计。
- [ ] `M03-AUTHZ-08` `P0` `[45m]` 测试当前板块版主、其他板块版主、全局版主和管理员的正负边界。
- [ ] `M03-AUTHZ-09` `P0` `[45m]` 测试自己/他人、草稿/公开/隐藏/删除、锁定板块和过期 assignment 组合。
- [ ] `M03-AUTHZ-10` `P0` `[30m]` 验证 Feature Flag、前端字段或请求体不能授予任何额外权限。
- [ ] `M03-AUTHZ-11` `P0` `[30m]` 自动比较权限注册表、OpenAPI `x-permission` 和权限矩阵文档。
- [ ] `M03-AUTHZ-12` `P1` `[30m]` 更新 Roles/Admin role operation coverage 并附越权测试证据。

## M03-BOARDS：板块、标签与管理 CRUD

**元数据：** `P1` · `owner=unassigned/backend-content` · `risk=high` · `depends=M03-AUTHZ` · `blocked=none`
**目标文件：** `backend/src/boards/`、`backend/src/tags/`、`backend/src/routes/{boards,tags,admin}/`、`backend/tests/boards/`
**验收：** Boards/Tags/Admin 对应 operation 在三数据库通过权限、分页和版本契约。

- [ ] `M03-BOARDS-01` `P1` `[45m]` 实现板块层级读取，限制最大深度并检测循环父级。
- [ ] `M03-BOARDS-02` `P1` `[30m]` 实现板块 slug、标题、说明、排序、状态和发帖规则校验。
- [ ] `M03-BOARDS-03` `P0` `[30m]` 实现 public/members/restricted/hidden 板块可见性并统一套用授权服务。
- [ ] `M03-BOARDS-04` `P1` `[45m]` 实现板块列表/详情 cursor 分页、稳定排序和 Cache-Control。
- [ ] `M03-BOARDS-05` `P1` `[45m]` 实现管理员创建/更新板块的版本冲突、reason 和审计。
- [ ] `M03-BOARDS-06` `P1` `[30m]` 实现标签组、标签 slug、展示名和禁用状态读取。
- [ ] `M03-BOARDS-07` `P1` `[45m]` 实现标签创建/更新的唯一性、版本冲突、权限和审计。
- [ ] `M03-BOARDS-08` `P0` `[30m]` 禁止通过板块/标签计数、面包屑或错误差异推断隐藏资源。
- [ ] `M03-BOARDS-09` `P1` `[45m]` 运行父级循环、并发 slug、隐藏板块、跨板块角色和 cursor 不重不漏测试。
- [ ] `M03-BOARDS-10` `P1` `[30m]` 更新 Boards/Tags 对应 operation coverage 和管理端证据。

## M03-SEARCH-STORE：跨库搜索仓储基础

**元数据：** `P1` · `owner=unassigned/backend-search` · `risk=high` · `depends=M03-BOARDS` · `blocked=none`
**目标文件：** `migrations/*/`、`backend/src/search/`、`backend/tests/search/`
**验收：** SQLite FTS5、MySQL FULLTEXT、MariaDB FULLTEXT 的索引生命周期和基础查询契约一致。

- [ ] `M03-SEARCH-STORE-01` `P1` `[45m]` 定义搜索文档最小模型、公开投影字段、source revision 和 policy revision。
- [ ] `M03-SEARCH-STORE-02` `P1` `[45m]` 创建 SQLite FTS5 迁移、触发/Job 更新策略和重建命令。
- [ ] `M03-SEARCH-STORE-03` `P1` `[45m]` 创建 MySQL 8 FULLTEXT 迁移、分词限制和重建命令。
- [ ] `M03-SEARCH-STORE-04` `P1` `[45m]` 创建 MariaDB 10.11 FULLTEXT 迁移并记录与 MySQL 的已知差异。
- [ ] `M03-SEARCH-STORE-05` `P0` `[30m]` 索引写入只接受经过可见性裁决的安全文本，不存 restricted_html。
- [ ] `M03-SEARCH-STORE-06` `P1` `[45m]` 实现创建、更新、隐藏、删除、恢复和退出索引的幂等 Job。
- [ ] `M03-SEARCH-STORE-07` `P1` `[45m]` 用同一 Fixture 验证查询、删除、重建和旧 revision 不覆盖新索引。

## M03-UI：资料、板块和角色前端

**元数据：** `P1` · `owner=unassigned/frontend-community` · `risk=medium` · `depends=M03-PROFILE,M03-BOARDS` · `blocked=none`
**目标文件：** `frontend/src/routes/users/`、`frontend/src/routes/boards/`、`frontend/src/routes/admin/`、`frontend/tests/`
**验收：** 匿名/member/moderator/admin 的 SSR、键盘、移动端和无 JS 流程通过。

- [ ] `M03-UI-01` `P1` `[45m]` 实现用户主页 SSR，处理不存在、匿名化、封禁和资料隐私状态。
- [ ] `M03-UI-02` `P1` `[45m]` 实现资料编辑表单、版本冲突、字段错误和保存后投影刷新。
- [ ] `M03-UI-03` `P1` `[45m]` 实现鼠标 Hover 与键盘 Focus 共用资料卡，支持离开延迟和 Escape 关闭。
- [ ] `M03-UI-04` `P1` `[30m]` 资料卡使用 portal/fixed 边界，窄屏改为点击/底部卡，不阻挡原导航。
- [ ] `M03-UI-05` `P0` `[30m]` 资料卡和主页不持久化签名 URL或私密字段，Cover 失败时安全降级。
- [ ] `M03-UI-06` `P1` `[45m]` 实现板块树、板块详情、标签筛选、空状态和权限提示。
- [ ] `M03-UI-07` `P1` `[45m]` 实现管理员板块、标签、角色和 assignment 页面，所有拒绝仍由后端裁决。
- [ ] `M03-UI-08` `P1` `[45m]` 测试键盘、触屏、减少动效、无 JS 公开资料/板块浏览和响应式布局。
- [ ] `M03-UI-09` `P1` `[30m]` 将原型对应路由映射到生产路由矩阵，不导入 mock/store 数据源。

---

<a id="m4"></a>

# M4：Markdown、帖子、文章、回复与可见性

**完成定义：** article/discussion/draft/comment 全链路可用；Markdown 安全渲染；隐藏正文不会从任何非授权投影泄漏；并发更新有稳定冲突语义。

## M04-SCHEMA：内容与版本数据模型

**元数据：** `P0` · `owner=unassigned/backend-db` · `risk=critical` · `depends=M03-AUTHZ,M03-BOARDS` · `blocked=none`
**目标文件：** `migrations/*/`、`backend/src/content/model*`、`docs/SCHEMA.md`
**验收：** 三数据库约束、楼层并发、revision 和软删除 repository contract 通过。

- [ ] `M04-SCHEMA-01` `P0` `[45m]` 新增 posts，覆盖 article/discussion、board、author、slug、状态、revision 和发布时间。
- [ ] `M04-SCHEMA-02` `P0` `[45m]` 新增 post_contents/revisions，保存 Markdown、清洗 HTML、renderer version 和安全摘要。
- [ ] `M04-SCHEMA-03` `P0` `[30m]` 新增 drafts 与 scheduled_at，并为 owner、更新时间和 cursor 建立索引。
- [ ] `M04-SCHEMA-04` `P0` `[45m]` 新增 comments、parent_id、floor、revision、状态和软删除字段。
- [ ] `M04-SCHEMA-05` `P0` `[30m]` 新增 post/tag、附件引用、封面引用和引用回复关联。
- [ ] `M04-SCHEMA-06` `P0` `[45m]` 新增 access policy：public/logged_in/after_reply/level/paid 及策略版本。
- [ ] `M04-SCHEMA-07` `P0` `[30m]` 建立板块内 slug、主题内楼层、客户端请求 ID 和 revision 唯一约束。
- [ ] `M04-SCHEMA-08` `P0` `[45m]` 测试并发楼层、slug 冲突、非法 parent、孤儿附件引用和软删恢复。
- [ ] `M04-SCHEMA-09` `P1` `[30m]` 同步 Schema、状态机、事件目录和 OpenAPI schema 差异。

## M04-MARKDOWN：安全渲染管线

**元数据：** `P0` · `owner=unassigned/backend-security` · `risk=critical` · `depends=M04-SCHEMA` · `blocked=none`
**目标文件：** `backend/src/content/markdown/`、`backend/tests/markdown/`、`frontend/src/lib/components/SafeHtml.svelte`
**验收：** XSS corpus、链接/图片策略、renderer version 和前端 sink 扫描通过。

- [ ] `M04-MARKDOWN-01` `P0` `[30m]` 请求只接受 Markdown；显式拒绝原始 HTML、BBCode 和未知内容格式。
- [ ] `M04-MARKDOWN-02` `P0` `[45m]` 选择并封装 CommonMark 渲染器，禁用原始 HTML 和危险扩展。
- [ ] `M04-MARKDOWN-03` `P0` `[45m]` 建立标签、属性、协议、图片、外链 rel/target 和 iframe Provider allowlist。
- [ ] `M04-MARKDOWN-04` `P0` `[30m]` 对标题锚点、代码块、引用、表格和超长嵌套设置确定性输出和上限。
- [ ] `M04-MARKDOWN-05` `P0` `[30m]` 保存 renderer/sanitizer policy version，升级时通过 Job 重渲染旧 revision。
- [ ] `M04-MARKDOWN-06` `P0` `[45m]` 生成公开摘要时先执行可见性和 Markdown 安全处理，禁止从隐藏正文截断。
- [ ] `M04-MARKDOWN-07` `P0` `[45m]` 建立 XSS corpus：事件属性、javascript/data URL、SVG、MathML、畸形 HTML 和 Unicode 绕过。
- [ ] `M04-MARKDOWN-08` `P0` `[30m]` 前端只允许 `SafeHtml` 使用 `{@html}`，通过静态检查阻止其他 sink。
- [ ] `M04-MARKDOWN-09` `P1` `[30m]` 测试纯文本、代码、链接、图片、长文和无 JavaScript 渲染一致性。
- [ ] `M04-MARKDOWN-10` `P1` `[30m]` 记录渲染策略变更的迁移、缓存失效和回滚方案。

## M04-POSTS：草稿、文章与讨论服务

**元数据：** `P1` · `owner=unassigned/backend-content` · `risk=high` · `depends=M04-MARKDOWN` · `blocked=none`
**目标文件：** `backend/src/content/posts/`、`backend/src/routes/{posts,drafts}/`、`backend/tests/posts/`
**验收：** Posts/Drafts/Revisions operation 在三数据库通过 CRUD、调度、权限和冲突契约。

- [ ] `M04-POSTS-01` `P1` `[30m]` 定义 article/discussion 创建命令和服务端字段校验，不信任 author、状态或统计值。
- [ ] `M04-POSTS-02` `P1` `[45m]` 实现创建草稿、读取自己的草稿和 cursor 列表。
- [ ] `M04-POSTS-03` `P1` `[45m]` 实现草稿更新、client_request_id 幂等、版本冲突和软删除。
- [ ] `M04-POSTS-04` `P1` `[45m]` 实现预览，只返回当前用户临时安全 HTML，不写公开索引或缓存。
- [ ] `M04-POSTS-05` `P0` `[45m]` 发布前重新读取作者等级、账号状态、板块规则、附件状态和 access policy。
- [ ] `M04-POSTS-06` `P1` `[45m]` 实现即时发布和 scheduled 发布 Job；执行时再次运行全部授权和等级校验。
- [ ] `M04-POSTS-07` `P1` `[45m]` 实现详情、列表、板块列表和作者列表的 cursor/ETag/Cache-Control。
- [ ] `M04-POSTS-08` `P1` `[45m]` 实现编辑并创建不可变 revision；管理员代发/代改要求 reason、recent-auth 和审计。
- [ ] `M04-POSTS-09` `P1` `[30m]` 实现 pin/feature/close/move/merge 的 domain command 接口，具体治理权限接 M5。
- [ ] `M04-POSTS-10` `P0` `[45m]` 测试发布事务回滚、重复请求、定时执行时降级和旧 revision 并发覆盖。
- [ ] `M04-POSTS-11` `P1` `[30m]` 实现 revisions 列表/详情，普通作者只能看自身允许版本，管理查看写审计。
- [ ] `M04-POSTS-12` `P1` `[30m]` 更新 Posts/Drafts/Revisions operation coverage 证据。

## M04-COMMENTS：回复、引用与楼层

**元数据：** `P1` · `owner=unassigned/backend-content` · `risk=high` · `depends=M04-POSTS` · `blocked=none`
**目标文件：** `backend/src/content/comments/`、`backend/src/routes/comments/`、`backend/tests/comments/`
**验收：** Comments operation 的楼层、锁定、可见性、编辑和删除契约在三数据库一致。

- [ ] `M04-COMMENTS-01` `P1` `[45m]` 实现回复创建，重新检查主题、板块、actor 状态和回复开关。
- [ ] `M04-COMMENTS-02` `P1` `[30m]` 校验 parent/quote 属于同一主题且对 actor 可见，禁止跨主题引用泄漏。
- [ ] `M04-COMMENTS-03` `P0` `[45m]` 原子分配楼层号，在三数据库并发创建时唯一且连续语义明确。
- [ ] `M04-COMMENTS-04` `P1` `[45m]` 实现回复列表 cursor、稳定排序、parent 摘要和软删占位投影。
- [ ] `M04-COMMENTS-05` `P1` `[30m]` 实现作者限时编辑、版本冲突、revision 和清洗重渲染。
- [ ] `M04-COMMENTS-06` `P1` `[30m]` 实现作者删除与管理删除的不同状态、审计和通知行为。
- [ ] `M04-COMMENTS-07` `P0` `[45m]` 测试锁帖、禁言、隐藏主题、已删 parent、并发楼层和引用不可见内容。
- [ ] `M04-COMMENTS-08` `P1` `[30m]` 更新 Comments operation coverage 和三数据库证据。

## M04-VISIBILITY：内容访问策略与防泄漏

**元数据：** `P0` · `owner=unassigned/security-content` · `risk=critical` · `depends=M04-POSTS,M04-COMMENTS` · `blocked=none`
**目标文件：** `backend/src/content/visibility/`、`backend/tests/visibility/`、`docs/API-CONTRACTS.md`
**验收：** 隐藏标记字符串在 API/SSR/索引/Feed/通知/缓存/附件中均无法被未授权 persona 获取。

- [ ] `M04-VISIBILITY-01` `P0` `[30m]` 将 public/logged_in/after_reply/level/paid 定义为封闭枚举，拒绝指定用户可见。
- [ ] `M04-VISIBILITY-02` `P0` `[45m]` 实现统一 evaluate(actor, content, context) policy，返回 grant reason 而非直接返回正文。
- [ ] `M04-VISIBILITY-03` `P0` `[30m]` `visibility_level` 只允许 `1..作者当前等级`，越级稳定返回 `visibility_level_exceeds_author`。
- [ ] `M04-VISIBILITY-04` `P0` `[45m]` 创建、编辑、草稿发布、定时发布和管理员代发均重新检查当前作者等级。
- [ ] `M04-VISIBILITY-05` `P0` `[30m]` after_reply 只接受有效且可见的本人回复，删除/处罚后的 grant 语义按冻结规则实现。
- [ ] `M04-VISIBILITY-06` `P0` `[30m]` paid 只读取账本支持的有效 grant；扣款与 grant 创建由 M7 原子完成。
- [ ] `M04-VISIBILITY-07` `P0` `[45m]` 未授权 DTO 完全省略正文、摘要、搜索高亮、附件列表和可逆编码数据。
- [ ] `M04-VISIBILITY-08` `P0` `[30m]` 为公共/私有响应定义 Vary、ETag 和 Cache-Control，禁止跨 persona 304 泄漏。
- [ ] `M04-VISIBILITY-09` `P0` `[45m]` 建立一次可复用的投影过滤器供列表、详情、通知、Feed、SEO、AI 和附件调用。
- [ ] `M04-VISIBILITY-10` `P0` `[45m]` 用唯一 canary 字符串测试 API、错误、日志、审计 metadata、SSR 和 hydration 不泄漏。
- [ ] `M04-VISIBILITY-11` `P0` `[45m]` 测试等级边界、并发降级、after_reply、paid grant、封禁和管理员显式查看。
- [ ] `M04-VISIBILITY-12` `P1` `[30m]` 同步权限矩阵、错误码、OpenAPI 和 operation coverage。

## M04-UI：内容前端与端到端流程

**元数据：** `P1` · `owner=unassigned/frontend-content` · `risk=high` · `depends=M04-VISIBILITY` · `blocked=none`
**目标文件：** `frontend/src/routes/posts/`、`frontend/src/routes/editor/`、`frontend/src/lib/content/`、`frontend/tests/`
**验收：** article/discussion/draft/comment 的 SSR、键盘、移动端、冲突和隐藏正文 E2E 通过。

- [ ] `M04-UI-01` `P1` `[45m]` 实现文章、讨论列表和详情 SSR，使用后端安全投影而非浏览器再裁剪。
- [ ] `M04-UI-02` `P1` `[45m]` 实现编辑器 Markdown 输入/安全预览、字数状态和服务端字段错误。
- [ ] `M04-UI-03` `P1` `[45m]` 实现草稿保存、离开提示、恢复、删除和版本冲突 diff 提示。
- [ ] `M04-UI-04` `P1` `[30m]` 实现 article/discussion 切换、板块、标签、封面和定时发布时间表单。
- [ ] `M04-UI-05` `P0` `[30m]` 可见性选项只展示后端允许等级，前端篡改仍由 API 拒绝。
- [ ] `M04-UI-06` `P1` `[45m]` 实现回复、引用、编辑、删除、楼层定位和锁帖状态。
- [ ] `M04-UI-07` `P1` `[30m]` 实现 hidden/after_reply/level/paid 的可访问占位，不把正文放入 DOM。
- [ ] `M04-UI-08` `P1` `[30m]` 实现 409 版本冲突、422 等级变化、429 和审核中可恢复流程。
- [ ] `M04-UI-09` `P1` `[45m]` 测试无 JavaScript 公开阅读与发帖/回复表单的合理退化。
- [ ] `M04-UI-10` `P0` `[45m]` Playwright 断言未授权隐藏正文不出现在 HTML、hydration、网络缓存和浏览器状态。

---

<a id="m5"></a>

# M5：风险审核、举报、处罚、申诉与通知

**完成定义：** 普通内容默认发布；高风险内容进入人工审核；版主范围、利益冲突、处罚和申诉均服务端强制且可审计；通知不泄漏正文。

## M05-SCHEMA：治理与通知数据模型

**元数据：** `P0` · `owner=unassigned/backend-db` · `risk=high` · `depends=M04-VISIBILITY,M01-JOBS` · `blocked=none`
**目标文件：** `migrations/*/`、`backend/src/moderation/model*`、`backend/src/notifications/model*`
**验收：** 三数据库状态约束、去重、期限和不可变动作历史契约通过。

- [ ] `M05-SCHEMA-01` `P0` `[45m]` 新增 reports、moderation_cases、case_assignments 和内部 note 迁移。
- [ ] `M05-SCHEMA-02` `P0` `[45m]` 新增 moderation_actions/revisions，动作历史只追加不覆盖。
- [ ] `M05-SCHEMA-03` `P0` `[45m]` 新增 sanctions，覆盖 warning/rate_limit/mute/board_mute/ban、期限和撤销。
- [ ] `M05-SCHEMA-04` `P0` `[30m]` 新增 appeals、appeal decisions 和利益冲突 reviewer 约束所需字段。
- [ ] `M05-SCHEMA-05` `P1` `[45m]` 新增 notifications、notification_preferences 和 delivery dedup key。
- [ ] `M05-SCHEMA-06` `P0` `[30m]` 为举报 actor/target/reason 有效组合、重复窗口和状态建立索引/约束。
- [ ] `M05-SCHEMA-07` `P0` `[45m]` 测试非法状态、累计动作、到期、重复举报和不可变历史。
- [ ] `M05-SCHEMA-08` `P1` `[30m]` 同步 Schema、状态机、事件目录和保留策略。

## M05-RISK：发布前后风险评估

**元数据：** `P0` · `owner=unassigned/trust-safety` · `risk=critical` · `depends=M05-SCHEMA` · `blocked=none`
**目标文件：** `backend/src/moderation/risk/`、`backend/tests/moderation/risk*`、`docs/MODERATION.md`
**验收：** 低风险直接发布；高风险只进入人工队列；AI 失败/关闭不阻塞人工流程。

- [ ] `M05-RISK-01` `P0` `[30m]` 定义风险输入最小集合和版本化 policy，不向 Provider 暴露内部/隐藏数据。
- [ ] `M05-RISK-02` `P0` `[45m]` 实现新用户前 N 帖、链接数、重复内容、敏感词和频率规则。
- [ ] `M05-RISK-03` `P0` `[30m]` 普通内容保持先发布；命中高风险则原子设置 pending_review 且不进入公开投影。
- [ ] `M05-RISK-04` `P0` `[30m]` 定义 AI moderation suggestion 接口与禁用 Null Adapter，结果只能是建议。
- [ ] `M05-RISK-05` `P0` `[30m]` 禁止 AI 直接执行封禁、删除、放行、权限变更或账务动作。
- [ ] `M05-RISK-06` `P0` `[45m]` 作者状态投影只包含安全 reason category，不含举报人、内部 note、规则细节或 Prompt。
- [ ] `M05-RISK-07` `P0` `[45m]` 测试规则超时、AI 关闭/失败/迟到、重复评估和旧 policy 结果。
- [ ] `M05-RISK-08` `P1` `[30m]` 管理员可版本化更新风险阈值，要求 reason、审计和并发版本控制。
- [ ] `M05-RISK-09` `P1` `[30m]` 记录风险命中率、队列时长和误判反馈指标，不记录正文。

## M05-CASES：举报、案件与内容动作

**元数据：** `P0` · `owner=unassigned/backend-moderation` · `risk=critical` · `depends=M05-RISK,M03-AUTHZ` · `blocked=none`
**目标文件：** `backend/src/moderation/cases/`、`backend/src/routes/moderation/`、`backend/tests/moderation/`
**验收：** Moderation operation 的状态、范围、利益冲突、隐藏/恢复和审计测试通过。

- [ ] `M05-CASES-01` `P1` `[30m]` 实现帖子、回复、用户和附件举报 DTO、原因枚举与详情长度。
- [ ] `M05-CASES-02` `P0` `[30m]` 实现举报去重、撤回和统一响应，避免泄漏已有举报人与案件状态。
- [ ] `M05-CASES-03` `P0` `[45m]` 实现 open/triaged/investigating/resolved/rejected/reopened 状态机。
- [ ] `M05-CASES-04` `P0` `[30m]` 案件领取/转派验证版主板块范围、assignment 有效期和 actor 状态。
- [ ] `M05-CASES-05` `P0` `[30m]` 禁止处理自己、自己内容或明确利益冲突案件，并记录阻断审计。
- [ ] `M05-CASES-06` `P0` `[45m]` 实现 hide/restore/delete/move/lock/pin/feature/merge/edit_for_moderation command。
- [ ] `M05-CASES-07` `P0` `[30m]` 每个内容动作写 revision、reason、actor、policy version、审计和 Outbox。
- [ ] `M05-CASES-08` `P0` `[30m]` hide/delete 立即从详情、列表、搜索、Feed、SEO、缓存和附件投影撤除。
- [ ] `M05-CASES-09` `P0` `[45m]` restore/reopen 不复用旧公开缓存，重新运行当前风险、权限和可见性策略。
- [ ] `M05-CASES-10` `P0` `[45m]` 测试跨板块越权、自处理、并发处理、旧 version 和动作中途失败回滚。
- [ ] `M05-CASES-11` `P1` `[30m]` 更新 Moderation operation coverage 和动作审计证据。

## M05-SANCTIONS：处罚、实时生效与撤销

**元数据：** `P0` · `owner=unassigned/security-moderation` · `risk=critical` · `depends=M05-CASES,M02-SESSION` · `blocked=none`
**目标文件：** `backend/src/moderation/sanctions/`、`backend/tests/moderation/sanctions*`
**验收：** 处罚请求时实时生效；封禁撤销 Session/Refresh；到期和撤销不删除历史。

- [ ] `M05-SANCTIONS-01` `P0` `[30m]` 定义 warning/rate_limit/mute/board_mute/ban 的范围、叠加和优先级。
- [ ] `M05-SANCTIONS-02` `P0` `[45m]` 实现处罚创建，要求权限、板块范围、期限上限、reason 和近期认证。
- [ ] `M05-SANCTIONS-03` `P0` `[30m]` 请求时实时计算有效处罚，不把 worker 到期任务作为正确性边界。
- [ ] `M05-SANCTIONS-04` `P0` `[30m]` ban 同事务/提交后可靠撤销用户 Session，并投递 OIDC Refresh family 撤销事件。
- [ ] `M05-SANCTIONS-05` `P0` `[30m]` 实现处罚撤销/到期归档，只追加 reversal 记录，不改原处罚。
- [ ] `M05-SANCTIONS-06` `P0` `[45m]` 防止低权限版主处罚更高权限账号或超出板块/时长上限。
- [ ] `M05-SANCTIONS-07` `P0` `[45m]` 测试处罚即时生效、并发请求、到期边界、撤销、Session 和后续 OIDC 集成。
- [ ] `M05-SANCTIONS-08` `P1` `[30m]` 为用户提供安全处罚状态和到期时间，不泄漏内部依据或举报人。
- [ ] `M05-SANCTIONS-09` `P1` `[30m]` 增加处罚创建/到期/撤销安全通知和运营指标。

## M05-APPEALS：申诉与独立复核

**元数据：** `P1` · `owner=unassigned/backend-moderation` · `risk=high` · `depends=M05-SANCTIONS` · `blocked=none`
**目标文件：** `backend/src/moderation/appeals/`、`backend/tests/moderation/appeals*`
**验收：** submitted/reviewing/upheld/partially_upheld/rejected/withdrawn 合法迁移和权限测试通过。

- [ ] `M05-APPEALS-01` `P1` `[30m]` 实现可申诉对象、窗口、次数、文字长度和附件引用规则。
- [ ] `M05-APPEALS-02` `P1` `[45m]` 实现用户创建、列表、详情和未审理前撤回。
- [ ] `M05-APPEALS-03` `P0` `[30m]` 分配复核人时排除原处理者、自身、超范围和无有效 assignment 人员。
- [ ] `M05-APPEALS-04` `P0` `[45m]` 实现 uphold/partial/reject decision；接受申诉以补偿/撤销记录修正，不删历史。
- [ ] `M05-APPEALS-05` `P0` `[30m]` 申诉 DTO 分离用户说明与内部 note，双方投影不越界。
- [ ] `M05-APPEALS-06` `P1` `[45m]` 测试窗口、重复提交、利益冲突、并发 decision 和处罚撤销联动。
- [ ] `M05-APPEALS-07` `P1` `[30m]` 更新 appeals operation coverage、状态机和通知事件。

## M05-NOTIFY：站内通知与邮件投递

**元数据：** `P1` · `owner=unassigned/backend-notifications` · `risk=high` · `depends=M05-CASES,M01-JOBS` · `blocked=none`
**目标文件：** `backend/src/notifications/`、`backend/src/email/`、`backend/tests/notifications/`
**验收：** Notifications operation、Outbox/Job 重试、偏好和隐藏内容摘要测试通过。

- [ ] `M05-NOTIFY-01` `P1` `[30m]` 定义回复、引用、提及、审核、处罚、申诉、等级和安全通知模板键。
- [ ] `M05-NOTIFY-02` `P0` `[30m]` 通知 payload 只存资源 ID 与安全模板参数，不复制隐藏正文或内部 note。
- [ ] `M05-NOTIFY-03` `P1` `[45m]` 实现站内通知 cursor 列表、单条/批量已读和未读计数。
- [ ] `M05-NOTIFY-04` `P1` `[30m]` 实现类别偏好；强制安全通知不能被普通偏好完全关闭。
- [ ] `M05-NOTIFY-05` `P1` `[30m]` 以事件/收件人/template 建立去重，重放 Outbox 不重复通知。
- [ ] `M05-NOTIFY-06` `P0` `[45m]` 读取通知时重新检查目标权限；失权后只显示安全失效状态。
- [ ] `M05-NOTIFY-07` `P1` `[45m]` 邮件通过 Job 投递，处理临时/永久失败、退避、dead 和管理员重放。
- [ ] `M05-NOTIFY-08` `P0` `[30m]` 验证邮箱 token、完整邮箱、隐藏正文和 Provider 响应不进入日志。
- [ ] `M05-NOTIFY-09` `P1` `[45m]` 测试偏好、去重、资源隐藏/删除、邮件失败和无正文泄漏。

## M05-UI：举报、审核、申诉与通知前端

**元数据：** `P1` · `owner=unassigned/frontend-moderation` · `risk=high` · `depends=M05-APPEALS,M05-NOTIFY` · `blocked=none`
**目标文件：** `frontend/src/routes/moderation/`、`frontend/src/routes/admin/moderation/`、`frontend/src/routes/notifications/`、`frontend/tests/`
**验收：** member/board moderator/global moderator/admin persona 的 Playwright、键盘和泄漏测试通过。

- [ ] `M05-UI-01` `P1` `[30m]` 实现举报对话框/页面、原因、详情、成功统一状态和撤回入口。
- [ ] `M05-UI-02` `P1` `[45m]` 实现作者审核中/通过/拒绝状态，不展示风险细节或举报人。
- [ ] `M05-UI-03` `P1` `[45m]` 实现版主案件队列、筛选、详情、领取和范围提示。
- [ ] `M05-UI-04` `P1` `[45m]` 实现内容动作和处罚表单，要求 reason、期限、确认和 recent-auth 状态。
- [ ] `M05-UI-05` `P0` `[30m]` 自身案件、跨板块和高权限目标即使前端篡改也由 API 拒绝并呈现稳定错误。
- [ ] `M05-UI-06` `P1` `[45m]` 实现申诉创建、列表、详情、撤回和复核决策页面。
- [ ] `M05-UI-07` `P1` `[30m]` 实现通知列表、未读、偏好和失效资源安全状态。
- [ ] `M05-UI-08` `P0` `[45m]` Playwright 验证隐藏正文、内部 note、举报人和超范围案件不进入 DOM/hydration。
- [ ] `M05-UI-09` `P1` `[45m]` 测试键盘、焦点、移动端、减少动效和无 JavaScript 举报表单退化。
- [ ] `M05-UI-10` `P1` `[30m]` 完成 Moderation/Notifications operation coverage 和路由矩阵证据。

---

## M3-M5 出口门槛

- Users/Roles/Boards/Tags/Posts/Drafts/Revisions/Comments/Moderation/Notifications 对应 operation coverage 均有实现与测试证据。
- 所有 persona 的权限由 API 直接测试，版主板块范围和对象 owner 条件无遗漏。
- 隐藏正文 canary 不出现在 API、SSR、DOM、hydration、搜索、Feed、通知、日志、审计、附件或公共缓存。
- 创建、编辑、定时发布时都不能设置高于作者当前等级的可见等级。
- 高风险内容只进入人工队列，AI 不能直接处罚、删除或放行。
- 举报、处罚和申诉历史只追加；封禁实时撤销 Session，并为 OIDC 撤销留出可靠事件。
