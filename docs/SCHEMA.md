# BBLBB — 数据模型与双数据库约定

> 版本：v0.4
> 本文描述逻辑模型和跨数据库约束；可执行 DDL 分别位于未来的 `backend/migrations/sqlite/` 与 `backend/migrations/mysql/`。表结构变更必须同步更新 OpenAPI、迁移和三数据库测试。

## 1. 支持矩阵

| 引擎 | 最低版本 | 主要用途 |
|---|---:|---|
| SQLite | 3.40+，启用 FTS5 | 默认、小型单机部署 |
| MySQL | 8.0+ | 更高写并发、现有 MySQL 环境 |
| MariaDB | 10.11+ | MySQL 系兼容部署 |

选择数据库发生在安装时。BBLBB 不承诺通过更换 `DATABASE_URL` 完成在线迁移；跨引擎迁移使用专用导出/导入工具并经过校验。

### 1.1 MariaDB 兼容策略

MariaDB 通过 MySQL 协议接入：`DATABASE_URL` 的 `mariadb://` scheme 在 `backend/src/db/pool.rs` 中归一化为 `mysql://`，由 sqlx 的 MySQL 驱动（`features = ["runtime-tokio", "sqlite", "mysql"]`）连接。连接层固定 `charset=utf8mb4` 与 `collation=utf8mb4_bin`（MySQL 8 与 MariaDB 10.11 均支持）。

三条硬性规则：

1. **不得使用 MySQL 8 独有能力**：例如 `utf8mb4_0900_ai_ci` 排序规则（MariaDB 不支持）、原生 JSON 运算符 `->>`/`JSON_EXTRACT`（MariaDB 的 `JSON` 是 `LONGTEXT` 别名）。JSON 一律在应用层用版本化 schema 校验，数据库侧只存 `TEXT`/`LONGTEXT` 或两者都支持的 `JSON` 列，不依赖 JSON 专用表达式做高频过滤。
2. **共享语法必须验证双端行为**：`INSERT ... ON DUPLICATE KEY UPDATE col = VALUES(col)` 在 MySQL 8.0.20+ 标记弃用但可运行、MariaDB 10.11 原生支持；窗口函数、`WITH RECURSIVE`、`CHECK` 约束、`FULLTEXT` 索引（中文检索用 `ngram` parser）双端都有，但细节差异由三数据库契约测试覆盖（M01-DB-09/M01-DB-10）。
3. **差异即测试**：任何在 MySQL 8 与 MariaDB 10.11 之间行为不同的特性，要么在迁移中显式归一化（如 `sql_mode`、事务隔离、字符集），要么写入三数据库故障矩阵测试；不允许依赖未测试的隐式行为。

SQLite、MySQL 8、MariaDB 10.11 三份迁移结构等价由 `migrations/{sqlite,mysql,mariadb}/` 的同一版本号文件保证，CI 对三库执行迁移与关键契约测试（支持矩阵见第 1 节；结构等价断言见 M01-DB-09）。

## 2. 跨数据库约定

### 2.1 主键

- 业务表主键统一为应用生成的 UUID v7。
- Rust 领域层使用 `uuid::Uuid`。
- SQLite 使用 `TEXT` 存储规范化 UUID 字符串。
- MySQL/MariaDB 使用 `CHAR(36) CHARACTER SET ascii COLLATE ascii_bin`。
- 不混用自增主键，避免跨库、导入导出和插件引用产生两套语义。

### 2.2 时间

- 所有时间均为 UTC Unix 毫秒，数据库列统一使用有符号 `BIGINT` 语义。
- Rust 使用一个明确的 `UnixMillis(i64)` 类型封装，不在领域层混用秒和毫秒。
- 字段命名使用 `created_at`、`updated_at`、`expires_at`、`deleted_at`。

### 2.3 布尔、枚举与 JSON

- SQLite 布尔值使用 `INTEGER` 的 `0/1`；MySQL/MariaDB 使用 `BOOLEAN`/`TINYINT(1)`。
- 枚举在数据库中使用短 `VARCHAR`，由 Rust 枚举完成主要校验；迁移可增加等价 `CHECK`，但业务正确性不依赖它。
- SQLite JSON 使用 `TEXT`；MySQL/MariaDB 使用 `JSON`。所有 JSON 必须在应用层用版本化 schema 校验。
- 高频查询字段不能只放在 JSON 中。

### 2.4 大小写与唯一性

- 用户名和邮箱分别保存展示值与规范化值：`username_normalized`、`email_normalized`。
- 规范化使用明确、版本化的 Unicode NFKC/case-fold 与邮箱规则；值以 UTF-8 二进制排序规则建立唯一索引，不依赖数据库默认大小写行为。
- UUID 使用 ASCII 二进制排序；Slug 由应用规范化为小写 ASCII/百分号编码规则并建立唯一索引。
- 规范化算法变化属于数据迁移，必须先检测冲突再切换。

### 2.5 外键与删除策略

- SQLite 每个连接执行：`PRAGMA foreign_keys=ON`、`PRAGMA journal_mode=WAL`、`PRAGMA busy_timeout=<配置值>`。
- 用户内容默认软删除；关系表和一次性 token 可物理删除。
- `ON DELETE CASCADE` 仅用于明确的附属数据，例如 `role_permissions`、`post_tags`。
- 审计和账本记录不级联删除；注销用户时用匿名主体保留必要审计信息。

### 2.6 SQL 与仓储层

- 领域层只依赖仓储 trait，不接触 `sqlx::Row`、SQL 或数据库连接池。
- 使用数据库无关的领域类型；基础 CRUD 尽量使用可移植 SQL。
- SQLite 与 MySQL/MariaDB 在以下路径使用专有实现：事务起始、行锁、UPSERT、全文搜索和批量插入。
- 不使用一个“万能 SQL”掩盖语义差异。

### 2.7 类型映射与分页排序（M01-DB-08）

逻辑类型到三数据库的表示必须统一，不允许同一逻辑字段在不同库出现不同语义：

| 逻辑类型 | SQLite | MySQL/MariaDB | 约束 |
|---|---|---|---|
| UUID v7（`id`/`*_id`/`target_id`） | `TEXT COLLATE BINARY` | `CHAR(36) CHARACTER SET ascii COLLATE ascii_bin` | 36 字符小写；时间有序，字符串排序即插入顺序 |
| Unix 毫秒时间戳（`*_at`） | `INTEGER` | `BIGINT` | 一律 UTC 毫秒；禁止秒；有符号 64 位 |
| 布尔（`is_*`/`pinned`/`*_verified`/`is_active`） | `INTEGER` 0/1 | `TINYINT(1)` | `1` 为真 |
| 枚举（`status`/`type`/`visibility`/`content_format`…） | `TEXT` + `CHECK` | `VARCHAR(32)` + `CHECK` | 值为 ASCII；Rust 枚举为主校验，`CHECK` 为双保险 |
| 计数/序号（`*_count`/`floor`/`attempts`/`sort_order`） | `INTEGER` | `INT`/`BIGINT` | 按字段语义选宽度 |
| JSON | `TEXT` | `JSON` | 应用层版本化 schema 校验 |

分页与排序约定：

- 列表排序必须是确定性的：默认 `created_at DESC, id DESC`；板内评论用
  `floor ASC`。UUID v7 时间有序，使 `id` 成为与数据库无关的稳定 tie-breaker。
- 深分页使用基于 `(sort_key, id)` 元组的游标，不用 `OFFSET`；游标的编码与
  表示（base64/opaque string）对三数据库一致。
- 排序键（时间戳、floor、sort_order）的表示与比较语义三库一致，避免同一查询
  在不同库返回不同顺序。


## 3. 系统与迁移

### `schema_migrations`

| 字段 | 说明 |
|---|---|
| `version` | 迁移版本，主键，防止同版本重复应用 |
| `name` | 迁移名称 |
| `checksum` | 迁移文件全文 SHA-256；已应用后内容变化必须失败 |
| `applied_at` | 应用时间（Unix 毫秒） |

迁移历史/checksum 表契约（M01-DB-07）：

- 由迁移执行器在首次应用前创建（`backend/src/db/migrate.rs` 的
  `ensure_migration_table`），不进入迁移文件本身；SQLite 与 MySQL/MariaDB
  结构等价（MySQL 侧固定 `utf8mb4`/`utf8mb4_bin`）。
- `checksum` 是迁移文件全文的 SHA-256；同一版本内容一旦应用后变更，
  `migrate --check` 报告不匹配且 `migrate apply` 拒绝执行（含数据库超前时
  的未知迁移拒绝）。
- 四个字段全部 NOT NULL；`version` 唯一。

### `site_settings`

| 字段 | 说明 |
|---|---|
| `key` | 主键，例如 `site.name` |
| `value_json` | 版本化 JSON 值 |
| `is_secret` | 秘密配置不得通过普通 API 返回 |
| `updated_by` | 管理员用户 ID，可空 |
| `updated_at` | 更新时间 |

秘密优先从环境变量/秘密文件加载；数据库中的秘密必须加密，不得只靠 `is_secret` 标识。

## 4. 用户、凭据与会话

### `users`

| 字段 | 约束/说明 |
|---|---|
| `id` | UUID v7 主键 |
| `username` | 展示用户名 |
| `username_normalized` | 唯一、不可空 |
| `email` | 原始展示邮箱，私有字段 |
| `email_normalized` | 唯一、不可空 |
| `password_hash` | Argon2id PHC 字符串 |
| `display_name` | 昵称 |
| `avatar_attachment_id` | 可空，关联附件 |
| `signature`、`bio` | 个人资料 |
| `status` | `pending/active/restricted/banned/pending_delete/deleted` |
| `email_verified_at` | 可空 |
| `failed_login_count` | 连续失败次数 |
| `locked_until` | 登录锁定截止时间，可空 |
| `last_login_at` | 可空 |
| `delete_requested_at` | 可空 |
| `created_at`、`updated_at`、`deleted_at` | 时间字段 |

注销硬删除时，必须先执行匿名化流程；是否释放原邮箱/用户名由隐私策略明确规定。

规范化约定（M02-IDENTITY-02）：`username_normalized` / `email_normalized`
入库前先 `trim → Unicode NFKC → lowercase`（`auth::normalize_username` /
`normalize_email`）；规范化列排序规则大小写敏感（SQLite `COLLATE BINARY` /
MySQL/MariaDB `utf8mb4_bin`），因此唯一索引对大小写与全角/连字等 Unicode
变体去重。

### `user_preferences`

| 字段 | 说明 |
|---|---|
| `user_id` | 主键、外键 |
| `timezone` | IANA 时区 |
| `locale` | 语言，例如 `zh-CN` |
| `theme_name` | 已安装主题名，可空 |
| `notification_json` | 通知偏好 |
| `updated_at` | 更新时间 |

### `user_sessions`

| 字段 | 说明 |
|---|---|
| `id` | UUID 主键，同时作为可展示的设备会话 ID |
| `user_id` | 用户 ID，索引 |
| `token_hash` | 随机 Session Token 的 SHA-256/HMAC 哈希，唯一 |
| `csrf_secret_hash` | CSRF secret 的哈希 |
| `user_agent` | 截断后的 UA |
| `ip_prefix_hash` | 可选，用于安全提醒，不作为唯一身份依据 |
| `created_at`、`last_seen_at` | 时间 |
| `idle_expires_at` | 滑动过期时间 |
| `absolute_expires_at` | 最长有效期 |
| `revoked_at`、`revoke_reason` | 可空 |

浏览器 Cookie 只持有高熵随机 token；数据库不存明文 token。

### `email_verification_tokens` / `password_reset_tokens`

共同字段：

- `id`、`user_id`、`token_hash`、`expires_at`、`consumed_at`、`created_at`。
- `token_hash` 唯一。
- 新 token 可使同用户旧 token 失效。
- 密码重置成功后撤销其他 Session。

### `totp_credentials`（v1 可选）

- `id`、`user_id`、加密后的 TOTP secret、`confirmed_at`、`created_at`、`revoked_at`。
- 恢复码单独存哈希，不存明文。

## 5. 角色与授权

### `permissions`

| 字段 | 说明 |
|---|---|
| `id` | UUID 主键 |
| `name` | 唯一，例如 `post.edit_any` |
| `description` | 管理后台说明 |
| `risk_level` | `normal/sensitive/system` |
| `is_system` | 系统权限不能删除 |

### `roles`

- `id`、`name`（唯一）、`display_name`、`description`、`is_system`、`created_at`、`updated_at`。
- 角色本身不保存权限 JSON，避免出现两个事实来源。

### `role_permissions`

- 复合主键：`(role_id, permission_id)`。
- 外键删除角色时级联删除映射。

### `user_roles`

- 复合主键：`(user_id, role_id)`。
- 仅代表全局角色。
- 字段另含 `granted_by`、`granted_at`、`expires_at`。

### `board_role_assignments`

- 主键 `id`。
- 唯一约束：`(board_id, user_id, role_id)`。
- 字段另含 `granted_by`、`granted_at`、`expires_at`。
- 角色权限只在指定板块及其明确配置的后代范围内生效；默认不自动继承到子板块。

详细判定顺序见 [`AUTHORIZATION.md`](AUTHORIZATION.md)。

## 6. 板块与标签

### `boards`

| 字段 | 说明 |
|---|---|
| `id` | 主键 |
| `parent_id` | 可空，自关联 |
| `slug` | 唯一 |
| `name`、`description` | 展示内容 |
| `visibility` | `public/members/restricted/hidden` |
| `posting_mode` | `normal/approval/readonly/closed` |
| `sort_order` | 同级排序 |
| `settings_json` | 附件、标签等低频配置 |
| `created_by` | 创建人 |
| `created_at`、`updated_at`、`deleted_at` | 时间 |

索引：`(parent_id, sort_order)`、`(visibility, deleted_at)`。

### `tag_groups`

- `id`、`name`、`slug`（唯一）、`sort_order`、`created_at`。

### `tags`

- `id`、`group_id`（可空）、`name`、`slug`（唯一）、`description`、`color`、`post_count`、`created_at`。
- `post_count` 是可重建缓存，不是真实来源。

### `post_tags`

- 复合主键：`(post_id, tag_id)`。

## 7. 帖子、回复与访问策略

### `posts`

| 字段 | 说明 |
|---|---|
| `id` | UUID 主键 |
| `board_id`、`author_id` | 外键 |
| `post_type` | `article/discussion` |
| `slug` | 可空但唯一；文章必须有 slug |
| `title`、`excerpt` | 标题和公开摘要 |
| `body_markdown` | 公开 Markdown |
| `body_html` | 后端生成并清洗的公开 HTML |
| `restricted_markdown`、`restricted_html` | 可空，受限部分 |
| `access_policy_id` | 可空，受限内容策略 |
| `cover_attachment_id` | 可空 |
| `status` | `draft/pending_review/rejected/published/hidden/deleted` |
| `closed_at` | 可空；非空表示禁止新增回复，不改变发布/可见状态 |
| `pinned_at`、`featured_at` | 可空，替代多组布尔值 |
| `scheduled_at`、`published_at` | 可空 |
| `canonical_url`、`seo_title`、`seo_description` | 博客/SEO 字段 |
| `view_count`、`reply_count` | 可重建缓存 |
| `last_reply_id`、`last_reply_at` | 列表排序缓存 |
| `version` | 乐观并发版本 |
| `created_at`、`updated_at`、`deleted_at` | 时间 |

主要索引：

- 唯一 `slug`。
- `(board_id, status, pinned_at, last_reply_at)`。
- `(author_id, status, created_at)`。
- `(post_type, status, published_at)`。

### `post_slug_redirects`

- `old_slug` 主键、`post_id`、`created_at`。
- 修改文章 slug 后保留永久 301 映射。

### `comments`

| 字段 | 说明 |
|---|---|
| `id`、`post_id`、`author_id` | 主键与外键 |
| `parent_id`、`quoted_comment_id` | 可空 |
| `floor_no` | **主题内**楼层号 |
| `body_markdown`、`body_html` | 公开内容 |
| `restricted_markdown`、`restricted_html` | 可空 |
| `access_policy_id` | 可空 |
| `status` | `pending/published/hidden/deleted` |
| `version` | 乐观并发版本 |
| `created_at`、`updated_at`、`deleted_at` | 时间 |

唯一约束：`(post_id, floor_no)`。楼层分配必须在事务内完成。

### `post_revisions` / `comment_revisions`

- `id`、资源 ID、`editor_id`、Markdown 快照、受限 Markdown 快照、`change_reason`、`version`、`created_at`。
- 普通用户能否查看历史由权限控制；审核员始终可查看。

### `content_access_policies`

| 字段 | 说明 |
|---|---|
| `id` | 主键 |
| `kind` | `after_reply/level_or_reply/purchase` |
| `min_level_id` | 可空 |
| `currency_id`、`amount` | 付费策略时使用 |
| `reply_grant_persists` | 删除回复后授权是否保留 |
| `created_by`、`created_at` | 审计字段 |

应用层验证字段组合，不允许金额为负或引用禁用货币。

### `content_access_grants`

- `id`、`user_id`、`post_id`、`comment_id`、`policy_id`。
- 应用和 DDL 保证 `post_id`、`comment_id` 二者恰有一个；为避免跨库 `NULL` 唯一语义差异，分别建立“帖子授权键”和“回复授权键”生成/归一字段，或拆成两张物理表。
- `source_kind`：`reply/purchase/moderator/import`。
- `source_id`、`point_operation_id`（可空）、`granted_at`、`revoked_at`。
- 对规范化目标键与用户建立唯一约束，确保重复请求不会重复扣费。

### `bookmarks`

- 复合主键：`(user_id, post_id)`；另含 `created_at`。

### `post_reactions` / `comment_reactions`

- 复合主键：`(user_id, target_id, reaction)`。
- v1 可只开放 `like`，但表结构允许未来增加其他反应。

## 8. 审核与处罚

### `reports`

- `id`、`reporter_id`、`target_type`、`target_id`、`reason_code`、`details`。
- `status`：`open/triaged/investigating/resolved/rejected/reopened/withdrawn`；合法迁移见 `STATE-MACHINES.md`。
- `assigned_to`、`created_at`、`updated_at`。

### `moderation_cases`

- `id`、`title`、`status`（`open/triaged/investigating/resolved/rejected/reopened`）、`priority`、`assigned_to`、`created_by`、时间字段。
- 举报可通过 `case_reports(case_id, report_id)` 关联到同一案件。

### `moderation_actions`

- `id`、`case_id`（可空）、`actor_id`、`action`、`target_type`、`target_id`、`reason`、`metadata_json`、`created_at`。
- 只追加，不覆盖历史。

### `user_sanctions`

- `id`、`user_id`、`board_id`（可空表示全局）、`kind`（`warning/rate_limit/mute/board_mute/ban`）、`status`（`scheduled/active/expired/revoked`）、`reason`。
- `board_mute` 必须带 `board_id`；其他 kind 默认拒绝携带板块范围，除非未来协议明确扩展。
- `starts_at`、`ends_at`（永久时可空）、`created_by`、`revoked_at`、`revoked_by`。

### `moderation_appeals`

- `id`、`sanction_id`、`user_id`、`message`、`status`、`reviewed_by`、`decision_note`、时间字段。

状态机见 [`MODERATION.md`](MODERATION.md)。

## 9. 货币、账本与等级

### `currencies`

| 字段 | 说明 |
|---|---|
| `id` | 主键 |
| `code` | 唯一，例如 `exp`、`coin` |
| `name` | 展示名 |
| `kind` | `experience/spendable/reputation` |
| `allow_negative` | 默认 false |
| `is_enabled` | 是否可用 |
| `created_at`、`updated_at` | 时间 |

所有金额使用整数最小单位；v1 不支持浮点金额。

### `point_accounts`

- 复合主键：`(user_id, currency_id)`。
- `balance`、`frozen_balance`、`version`、`updated_at`。
- 约束：默认不允许负余额；冻结余额不得为负。

### `point_operations`

| 字段 | 说明 |
|---|---|
| `id` | 一次业务操作 ID |
| `idempotency_scope`、`idempotency_key` | 组合唯一 |
| `kind` | `award/consume/shop_purchase/transfer/freeze/unfreeze/adjust/reversal` |
| `actor_id` | 可空，系统操作 |
| `source_type`、`source_id` | 来源 |
| `reverses_operation_id` | 补偿时引用原操作 |
| `memo` | 原因 |
| `created_at` | 时间 |

### `point_transactions`

- `id`、`operation_id`、`user_id`、`currency_id`。
- `delta_balance`、`delta_frozen`。
- `balance_after`、`frozen_after`。
- `created_at`。
- 账本只追加，禁止 UPDATE/DELETE；撤销通过新建反向操作。

### `point_rules`

- `id`、`event_name`、`currency_id`、`amount`、`daily_limit`、`conditions_json`、`is_enabled`、时间字段。
- 规则变更只影响未来事件；已产生流水不重算。

### `level_schemes`、`levels`

- `level_schemes`：`id`、`name`、`currency_id`、`is_active`。
- `levels`：`id`、`scheme_id`、`name`、`threshold`、`sort_order`、`icon`、`color`、`benefits_json`。
- 唯一约束：`(scheme_id, threshold)` 与 `(scheme_id, sort_order)`。

### `user_levels`

- 复合主键：`(user_id, scheme_id)`。
- `level_id`、`computed_from_balance`、`updated_at`。
- 这是可重建缓存；真实来源是经验账户和等级阈值。

### `level_events`

- `id`、`user_id`、`scheme_id`、`from_level_id`、`to_level_id`、`reason`、`created_at`。

### 跨数据库积分事务

MySQL/MariaDB：

1. 开始事务，并先尝试插入/读取唯一的 `point_operations` 幂等记录；已有相同摘要则返回原结果，不同摘要则冲突。
2. `SELECT ... FOR UPDATE` 锁账户。
3. 校验余额并更新账户版本。
4. 插入 `point_transactions` 并完成 operation 结果。
5. 提交。

SQLite：

1. 使用 `BEGIN IMMEDIATE` 获取写锁。
2. 先插入/读取唯一幂等记录，并按请求摘要处理重复与冲突。
3. 查询账户并校验。
4. 使用 `UPDATE ... WHERE version = ?`，必须验证 `rows_affected == 1`。
5. 插入流水、完成 operation 结果后提交。

唯一幂等约束负责并发串行化；如果同 key 的 operation 仍处于处理中，调用方等待事务结果或获得可重试冲突，不得执行第二次账务变更。任何步骤失败都回滚。

## 10. 公开市场交易

完整协议见 [`MARKETPLACE.md`](MARKETPLACE.md)。所有金额继续复用整数最小单位和不可变积分账本。

### `marketplace_clients`

- `id`、`oauth_client_id`（唯一，只允许 Confidential Client）、`owner_user_id`、`status`（`pending/enabled/suspended/disabled`）。
- `approved_scopes_json`、`per_transaction_limit`、`daily_limit`、`risk_level`。
- `terms_url`、`privacy_url`、`webhook_url`、`webhook_secret_hash/encrypted_secret`、`webhook_key_version`。
- `approved_by`、`approved_at`、`created_at`、`updated_at`。
- Webhook Secret 仅创建/轮换时返回一次；按实现选择不可逆验证 hash 或可解密发送密钥，解密主密钥必须与数据库隔离。

### `marketplace_merchant_accounts`

- `id`、`client_id`、`owner_user_id`、`currency_id`、`available_balance`、`pending_balance`、`frozen_balance`、`version`、`status`、时间字段。
- `(client_id, currency_id)` 唯一；余额均为整数且不得为负。所有变更必须由 point operation/transaction 支撑，禁止管理端直接改余额。
- v1.0 仅为站内托管价值，不支持法币或站外提现；完整规则见 [`MARKETPLACE-ACCOUNTING.md`](MARKETPLACE-ACCOUNTING.md)。

### `marketplace_offers`

- `id`、`client_id`、`external_offer_id`、`version`、`status`。
- `title`、`description_safe`、`currency_id`、`unit_amount`、`stock_policy`、`stock_remaining`、`fee_bps`、`fee_refundable`、`settlement_delay_seconds`。
- `metadata_json` 仅保存受限、非敏感展示字段；`created_at`、`updated_at`。
- 唯一约束 `(client_id, external_offer_id, version)`；金额必须非负且受站点上限约束。
- 换价创建新版本或增加 version；已经创建的 Checkout Intent 保留不可变快照。

### `marketplace_checkout_intents`

- `id`（高熵不可猜）、`client_id`、`user_id`、`offer_id`、`offer_version`、`merchant_order_id`。
- `currency_id`、`quantity`、`unit_amount`、`total_amount` 是服务端报价快照。
- `status`（`requires_confirmation/succeeded/expired/cancelled`）、`expires_at`、`consumed_at`、`purchase_id`。
- `created_at`；唯一约束 `(client_id, merchant_order_id)`。
- 成功状态与 `purchase_id` 必须同时出现；一个 Intent 最多关联一个成功 Purchase。

### `marketplace_purchases`

- `id`、`client_id`、`user_id`、`checkout_intent_id`、`merchant_order_id`、`offer_id`、`offer_version`。
- `currency_id`、`quantity`、`total_amount`、`point_operation_id`。
- `status`（v1 提交后为 `succeeded/refunded/partially_refunded`）、`refunded_amount`、`created_at`、`updated_at`。
- 唯一约束 `checkout_intent_id`、`point_operation_id` 和 `(client_id, merchant_order_id)`。
- 普通接口禁止删除；状态只能由受控退款事务按合法状态机推进。

### `marketplace_refunds`

- `id`、`purchase_id`、`client_id`、`amount`、`reason_code`、`point_operation_id`、`status`、`created_at`。
- `idempotency_scope`、`idempotency_key` 组合唯一；`point_operation_id` 唯一。
- 同一 Purchase 的已提交退款总额不得超过购买金额；在锁定原 Purchase 的事务内验证和更新。

### `marketplace_webhook_deliveries`

- `id`、`client_id`、`outbox_event_id`、`event_id`、`endpoint_snapshot`、`key_version`。
- `status`、`attempts`、`next_attempt_at`、`last_status_code`、`last_error_code`、`delivered_at`、`created_at`。
- 唯一约束 `(client_id, event_id)`；不保存完整签名、Secret、Token 或包含敏感信息的响应体。

### 市场购买事务

MySQL/MariaDB 锁定顺序固定为：幂等 operation → Checkout Intent → Offer/库存 → Purchase（退款时）→ point account，避免不同 handler 反向加锁。SQLite 使用 `BEGIN IMMEDIATE`。

事务内依次校验 Client/scope/用户/限额和 Intent，条件消费库存与 Intent，创建 Purchase，更新 `point_accounts`，追加 `point_operations` 与 `point_transactions`，最后写 `audit_logs` 和 `outbox_events`。任何步骤失败全部回滚。相同幂等键重放读取首次持久化结果，不再次扣款。

退款在锁定原 Purchase 后创建 `marketplace_refunds` 和 `reversal` point operation，更新累计退款并写 Outbox；禁止修改原账本流水。

## 11. 附件与对象存储

### `attachments`

- `id`、`owner_id`、`storage_backend`（`local/s3`）、`storage_key`。
- `original_name`、`media_type`、`size_bytes`、`sha256`。
- `width`、`height`（图片可空）、`status`（`pending/processing/ready/quarantined/deleted`）。
- `quota_bytes_charged` 记录计入所有者配额的字节数，防止对象变体或重算口径漂移。
- `is_public`、`ref_count`（可重建）、`created_at`、`deleted_at`。
- 索引至少包含 `(owner_id, status, created_at)`；用户已用容量以未删除且计费状态的 `quota_bytes_charged` 求和，必要时使用可重建计数缓存。
- S3 签名链接的 `expires_at` 是临时响应/审计属性，不存为附件对象失效时间，也不驱动附件删除。

### `download_billing_policies`

- `id`、`scope_type`（`site/board/attachment`）、`scope_id`、`mode`（`disabled/free/fixed/inherit/forced_free/forced_paid`）。
- `currency_id`、`amount`、`authorization_ttl_seconds`、`daily_user_limit`、`single_charge_limit`、`attachment_revenue_limit`、`grace_on_disable`、`version`、`is_enabled`、`created_at`、`updated_at`。
- 等级/角色免费规则使用版本化 `conditions_json`，优先级和字段 schema 固定见 [`DOWNLOAD-BILLING.md`](DOWNLOAD-BILLING.md)。
- 价格解析按明确优先级执行：附件覆盖 → 板块覆盖 → 站点默认；等级/角色免费规则和管理员强制策略由服务层解释并审计。

### `download_authorizations`

- `id`、`attachment_id`、`user_id`、`policy_version`、`point_operation_id`（可空，免费授权为空）。
- `status`（`active/expired/revoked`）、`charged_amount`、`currency_id`、`valid_from`、`expires_at`、`created_at`。
- 唯一/索引：有效授权查询 `(user_id, attachment_id, status, expires_at)`；同一扣费 operation 唯一。
- 不存 S3 签名 URL；URL 每次通过稳定 content 端点重新鉴权签发。

### `download_idempotency_records`

- `scope`、`user_id`、`idempotency_key`、`request_hash`、`authorization_id`、`response_status`、`created_at`、`completed_at`。
- `(scope, user_id, idempotency_key)` 唯一；相同摘要重放原结果，不同摘要返回冲突。

### `attachment_links`

- `id`、`attachment_id`、`target_type`、`target_id`、`purpose`、`created_at`。
- 多态目标由服务层维护完整性，并由清理任务检查孤儿记录。

详细生命周期见 [`STORAGE.md`](STORAGE.md)。

## 12. AI Gateway 与任务

### `ai_providers`

- `id`、`name`、`adapter_kind`、`base_url`、`status`、`region`、`retention_statement`。
- `secret_ref`（只引用 Secret Store/加密配置，不存明文）、`models_json`、`created_by`、`created_at`、`updated_at`。
- Base URL 和模型能力由后端白名单校验；Provider 可按用途路由，不能由请求方任意指定。

### `ai_policies`

- `id`、`purpose`（`formatting/moderation/seo/tagging`）、`provider_id`、`model`、`data_mode`、`is_enabled`。
- `max_input_tokens`、`max_output_tokens`、`timeout_ms`、`daily_budget_units`、`redaction_version`、`version`、审计时间字段。
- 新版本只影响新任务；已完成 suggestion 保留使用的策略版本。

### `ai_consents`

- `id`、`user_id`、`provider_id`、`purpose`、`data_mode`、`disclosure_version`、`disclosure_hash`。
- `retention_statement`、`training_statement`、`region_statement` 保存同意时展示声明的安全快照或版本引用；`consented_at`、`revoked_at`、`policy_version`。
- 唯一活动同意由 `(user_id, provider_id, purpose, data_mode, disclosure_version, revoked_at)` 的跨库等价约束/事务保证；新声明版本必须重新同意，不能静默继承。
- 用户撤回后禁止创建需要同意的新任务，取消 queued 任务并丢弃不再获授权的迟到输出；历史审计按隐私保留策略处理。

### `ai_tasks`

- `id`、`purpose`、`actor_user_id`、`target_type`、`target_id`、`source_revision`、`policy_version`。
- `status`（`queued/running/succeeded/failed/cancelled/dead`）、`idempotency_key`、`input_hash`、`output_hash`。
- `provider_id`、`model`、`usage_json`、`error_code`、`attempts`、`created_at`、`completed_at`。
- 默认不保存原始 prompt/response；若管理员开启受限诊断保留，必须单独加密、限权和到期清理。
- 唯一去重键至少为 `(purpose, target_type, target_id, source_revision, policy_version, idempotency_key)`。

### `ai_suggestions`

- `id`、`task_id`、`target_type`、`target_id`、`base_version`、`suggestion_type`、`payload_json`。
- `status`（`pending/accepted/rejected/stale`）、`accepted_by`、`accepted_at`、`created_at`。
- payload 只允许 schema 定义的文本/标签/SEO/风险字段；服务端重新清洗、校验并做版本冲突检查。

## 13. 视频引用、通知、任务与审计

### `video_embeds`

- `id`、`owner_id`、`post_id`/`comment_id`、`provider`（`direct/hls/xigua`）、`source_url`、`canonical_url`、`external_id`。
- `media_type`、`poster_url`、`title`、`duration_seconds`、`status`（`pending/ready/blocked/error/removed`）、`metadata_json`、`policy_version`、`last_checked_at`、时间字段。
- 不保存平台签名播放 URL、Cookie、授权 Header、HLS Key 或任意抓取响应正文；引用目标必须经过服务层完整性校验。

### `video_provider_policies`

- `provider`、`enabled`、`allowed_hosts_json`、`embed_hosts_json`、`allowed_media_types_json`。
- `max_duration_seconds`、`max_bytes`、`max_redirects`、`hls_max_depth`、`hls_max_segments`、`hls_max_bytes`、`timeout_ms`、`policy_version`、时间字段。

### `notifications`

- `id`、`user_id`、`kind`、`actor_id`、`target_type`、`target_id`、`payload_json`、`read_at`、`created_at`。
- 索引：`(user_id, read_at, created_at)`。

### `outbox_events`

- `id`、`event_type`、`payload`、`payload_version`、`status`、`idempotency_key`。
- `created_at`、`next_attempt_at`、`attempts`、`max_attempts`、`processed_at`、`error`。
- 与业务事务同时写入；`idempotency_key` 唯一（消费者去重，M01-JOBS-01）。
- 事务回滚时事件必须同步消失（M01-JOBS-02）。

### `jobs`

- `id`、`queue`、`kind`、`payload`、`payload_version`、`status`。
- `available_at`、`locked_by`、`locked_until`、`attempts`、`max_attempts`。
- `deduplication_key`（唯一，可空）、`last_error`、`completed_at`、`created_at`、`updated_at`。
- `status` 合法值：`queued` / `running` / `retry_wait` / `succeeded` / `cancelled` / `dead`。
- worker 通过 `locked_by`/`locked_until` 租约领取；崩溃后租约过期可重领
  （M01-JOBS-04）。

### `audit_logs`

- `id`、`actor_id`、`effective_role`（执行时生效角色）、`action`。
- `target_type`、`target_id`、`reason`、`policy_version`（权限策略版本）。
- `metadata`（字段 allowlist 过滤，M01-AUDIT-02）、`request_id`、`ip_address`、`created_at`（Unix 毫秒）。
- **不可关闭、只追加**（M01-AUDIT-01）：无 status/disabled 列，代码不提供
  删除/修改审计记录路径。

## 14. 内部商城、装扮与活跃

### `shop_products`

- `id`、`kind`（`cosmetic_nickname/cosmetic_avatar/cosmetic_avatar_attachment/cosmetic_badge/profile_effect/post_effect/reaction_pack/title_prefix/utility`）、`status`（`draft/pending_review/published/disabled/retired`）。
- `slug`、`title`、`description_safe`、`icon_token`、`presentation_tokens_json`、`slot`、`currency_id`、`unit_price`、`quantity_limit`、`stock_remaining`、`required_level`。
- `validity_seconds`、`sale_start_at`、`sale_end_at`、`refund_policy`、`version`、`created_by`、时间字段。
- `icon_token` 和 `presentation_tokens_json` 只允许注册的安全 Token；不得保存任意 HTML/CSS/JS、远程 URL 或脚本。

### `shop_orders`

- `id`、`user_id`、`product_id`、`product_version`、`quantity`、`currency_id`、`unit_price`、`total_amount`、`point_operation_id`。
- `status`（`succeeded/refunded/partially_refunded`）、`idempotency_key`、`request_hash`、时间字段。
- `(user_id, idempotency_key)` 唯一；价格、货币、用户和扣费结果均为服务端快照。

### `user_entitlements`

- `id`、`user_id`、`product_id`、`order_id`、`status`（`owned/equipped/expired/revoked/consumed`）。
- `quantity`、`remaining_quantity`、`valid_from`、`expires_at`、`equipped_at`、`revoked_at`、时间字段。
- 装备由服务端按 `slot` 互斥；`profile_badges` 最多 3 个。到期自动卸下但不删除持有历史。

### `user_presentations`

- `user_id`、`nickname_decoration_id`、`nickname_color_id`、`avatar_frame_id`、`avatar_attachment_id`、`profile_effect_id`、`title_prefix_id`、`profile_badge_ids_json`、`post_effect_id`、`version`、时间字段。
- 所有 ID 必须引用本人有效 entitlement；读取时服务端编译安全 `presentation_tokens`，无权/过期/隐私关闭时返回默认展示。

### `activity_rules`、`activity_claims`

- `activity_rules`：`id`、`kind`（`check_in/task/reaction/post/comment/leaderboard`）、`currency_id`、`amount`、`daily_limit`、`cooldown_seconds`、`conditions_json`、`version`、`is_enabled`。
- `activity_claims`：`id`、`rule_id`、`user_id`、`activity_day`、`deduplication_key`、`point_operation_id`、`status`、时间字段。
- `(rule_id, user_id, deduplication_key)` 和日限额由事务保证；自我互动、重复编辑、批量刷反应不得重复奖励。

### `user_reactions`

- `user_id`、`target_type`、`target_id`、`reaction`、`created_at`；复合唯一键防止重复互动。
- 反应包消耗从 entitlement 的 `remaining_quantity` 原子扣减；反应本身不改变可见性、审核、排序或现金价值。

## 15. 主题与插件元数据

### `themes`

- `id`、`name`（唯一）、`version`、`kind`（`data/precompiled`）、`manifest_json`。
- `is_installed`、`is_enabled`、`installed_at`、`updated_at`。
- 全站默认主题存于 `site_settings`，不是用多个 `is_active` 布尔值竞争。

### `theme_settings`

- 复合主键：`(theme_id, key)`。
- `value_json`、`updated_by`、`updated_at`。

### `plugins`

- `id`、`name`（唯一）、`version`、`kind`（v1 仅 `config/precompiled_ui`）。
- `manifest_json`、`is_installed`、`is_enabled`、`installed_at`、`updated_at`。

### `plugin_settings`

- 复合主键：`(plugin_id, key)`。
- `value_json`、`updated_by`、`updated_at`。

### `plugin_data`

- `id`、`plugin_id`、`namespace`、`owner_type`、`owner_id`、`key`、`value_json`、时间字段。
- 唯一约束：`(plugin_id, namespace, owner_type, owner_id, key)`。
- v1 不向插件开放核心表 `extra` 字段，避免表结构和权限边界失控。

## 16. OIDC Provider

### `oauth_clients`

| 字段 | 说明 |
|---|---|
| `id` | 主键 |
| `client_id` | 高熵唯一 ID |
| `client_type` | `public/confidential` |
| `client_secret_hash` | Public Client 为空；Confidential Client 必填 |
| `name`、`description`、`logo_uri` | 展示信息 |
| `owner_user_id` | 客户端所有者 |
| `sector_identifier` | Pairwise Subject 输入 |
| `status` | `pending/active/disabled` |
| `created_at`、`updated_at` | 时间 |

### `oauth_client_redirect_uris`

- `id`、`client_id`、`redirect_uri`、`created_at`。
- 唯一约束 `(client_id, redirect_uri)`；授权时按 RFC/OIDC 注册规则比较，除 localhost 开发例外外不做通配、前缀或随意 URL 规范化。

### `oauth_client_post_logout_redirect_uris`

- `id`、`client_id`、`redirect_uri`、`created_at`。
- 唯一约束 `(client_id, redirect_uri)`；退出时只允许该表中的 URI。

### `oauth_client_scopes`

- 复合主键 `(client_id, scope)`；v1 仅允许已注册 scope。

### `oauth_subjects`

- `id`、`sector_identifier`、`user_id`、`subject`（唯一）、`created_at`。
- 唯一约束 `(sector_identifier, user_id)`；同一 sector 中的多个 Client 得到相同 Pairwise Subject，不同 sector 不能关联。
- `sector_identifier` 的确定和变更遵循 OIDC 规则，不能由 Client 任意改变后继续沿用旧 consent。

### `oauth_consents`

- `id`、`client_id`、`user_id`、`scopes_json`、`granted_at`、`revoked_at`、`last_used_at`。
- 唯一约束 `(client_id, user_id)`。

### `oauth_interactions`

- `id`、`request_hash`、`client_id`、`redirect_uri`、`scopes_json`。
- `state` 仅作为待返回给 Client 的不透明值保存；不得写普通日志。
- `nonce`、`code_challenge`、`code_challenge_method`、`user_id`（登录前可空）。
- `status`：`pending_login/pending_consent/approved/denied/consumed`。
- `expires_at`、`consumed_at`、`created_at`；默认 10 分钟且只能消费一次。
- 该表连接 Rust 协议校验与 SvelteKit 同意 UI；前端不能提交新的 redirect URI 或 scope。

### `oauth_authorization_codes`

- `id`、`code_hash`（唯一）、`client_id`、`user_id`、`redirect_uri`。
- `scopes_json`、`nonce`、`code_challenge`、`code_challenge_method`（仅 `S256`）。
- `auth_time`、`expires_at`、`consumed_at`、`created_at`。
- 授权码有效期建议 5 分钟且只能消费一次。

### `oauth_access_tokens`

- `id`、`token_hash`（唯一）、`client_id`、`user_id`、`subject`、`scopes_json`。
- `expires_at`、`revoked_at`、`created_at`。
- Access Token 为高熵 opaque token，数据库只存哈希。

### `oauth_refresh_token_families`

- `id`、`client_id`、`user_id`、`subject`、`created_at`、`revoked_at`、`revoke_reason`。

### `oauth_refresh_tokens`

- `id`、`family_id`、`token_hash`（唯一）、`parent_token_id`、`replaced_by_id`。
- `scopes_json`、`expires_at`、`used_at`、`revoked_at`、`created_at`。
- 已使用 token 再次出现时，撤销整个 family。

### `oauth_signing_keys`

- `id`、`kid`（唯一）、`algorithm`（v1 为 RS256）。
- `encrypted_private_key`、`public_jwk_json`。
- `status`：`pending/active/retiring/revoked`。
- `not_before`、`sign_until`、`verify_until`、`created_at`。

协议细节见 [`AUTH-OIDC.md`](AUTH-OIDC.md)。

## 17. 关键约束清单

迁移和集成测试必须验证：

- 用户名、邮箱和 slug 的规范化唯一性。
- `comments(post_id, floor_no)` 唯一。
- 角色权限只有一个事实来源：`role_permissions`。
- 全局角色与板块角色使用不同表，不使用可空复合主键。
- 积分账本只追加，幂等键唯一。
- 内容解锁不能重复扣费。
- Confidential OAuth Client 必须有 secret；Public Client 不得要求 secret。
- 授权码只能消费一次。
- Refresh Token 重用会撤销整个 family。
- 主题和插件设置引用已安装实体。
- SQLite、MySQL、MariaDB 的迁移结果在逻辑上等价。

## 18. 迁移策略

- 每个逻辑版本提供 SQLite 和 MySQL 系两套迁移。
- MySQL 迁移必须同时在 MySQL 与 MariaDB 执行。
- 迁移文件一旦发布不可修改；通过新迁移纠错。
- 启动时默认只检查迁移状态；生产环境是否自动执行迁移由配置决定，默认建议显式运行 `bblbb migrate`。
- 破坏性变更采用 expand → backfill → switch → contract，多版本滚动部署时不得先删除旧字段。
- 数据导出采用领域格式（JSONL + 文件清单），而不是假设两种 SQL dump 可互换。
