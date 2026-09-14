# TRUST-LEVELS — 信任等级（LinuxDo 式 TL0–TL4）

> 状态：M20-TRUST（2026-09 实现）。移植自 linux.do（Discourse 信任等级体系）。
> 术语与数据模型见 `docs/TERMINOLOGY.md`、`docs/SCHEMA.md` §9「信任等级」；迁移 `migrations/{sqlite,mysql,mariadb}/0070_trust_levels.sql`。

## 1. 设计目标

- 把 linux.do 的五级用户信任体系（新用户 / 基本用户 / 成员 / 活跃用户 / 领导者）移植到 BBLBB，作为**全站唯一的用户等级与行为可信度**体系：鼓励阅读与长期活跃，让长期可靠的贡献者获得协助整理社区的信任。
- **等级单轨制（2026-09 起全面落地）**：彻底停用原 10 级经验等级（LV.1~LV.10）体系。历史 `exp` 账本数据仅保留作审计/迁移兼容，新的活动奖励与管理调账统一使用 `coin`（B 币），不再用于评定等级；前台与后台统一仅展示并结算 TL0–TL4 信任等级。

### 1.1 体系定位（等级合并单轨）

全站等级逻辑完全收归 LinuxDo 信任等级单轨，前后台所有等级入口均映射到 TL0–TL4：

| 数据表 | 入口位置 | 运行时角色 |
|---|---|---|
| `trust_level_rules`（0070） | **/admin/levels（等级管理）与 /me/level（信任等级中心）** | **全站唯一等级主线**：TL0–TL4 自动评估（§3）+ 管理员手动授予，事件与审计完整 |
| `users.trust_level` | 全站用户等级真源（PublicProfile / Me / Session 的兼容 `level` 投影） | 统一表示用户的信任等级（0–4），驱动内容可见性、商城资格与配额；旧 `users.level` 仅作迁移期兼容列，不再参与运行时裁决 |
| `level_rules`（0062） / 历史 10 级阶梯方案 | **已彻底下线** | 移除全部 10 级经验阶梯表与展示；历史数据仅保留为数据库存档，不再参与任何运行时计算 |

因此，全站判定「用户是什么等级、怎么升上去」统一且仅以本文件（LinuxDo 信任等级）为准。附件空间配额与保留期档位键 = `users.trust_level`（TL0–TL4，`quota_policy_revisions.level`；默认档五档互异，`default_policy_for_level`）。

## 2. 领域映射（LinuxDo → BBLBB）

| LinuxDo / Discourse 概念 | BBLBB 落地 |
|---|---|
| 话题 topic | `posts`（统一内容，`deleted_at IS NULL AND status != 'draft'` 计入窗口分母） |
| 帖子/楼层 post | `comments`（楼层回复；「阅读帖子」= 阅读楼层） |
| 点赞 like | `user_reactions` 且 `reaction = 'like'`（reactions 服务唯一写入路径；`post_reactions`/`comment_reactions` 为遗留空表） |
| 收到的赞的归属 | 目标为 post/comment 时按 `posts.author_id` / `comments.author_id` 归属；目标为 user 时直接指向该用户（TL3 窗口统计口径） |
| 标记 flag | `reports`（`status = 'resolved'` 视为版主确认；按目标内容作者归属，支持 target_type=post/comment/user） |
| 禁言/暂停 silence/suspend | `sanctions`（`kind IN ('mute','ban')` 且 `revoked_at IS NULL`，`starts_at` 在近 6 个月内） |
| 访问天数 days visited | `trust_visits`（每人每日一行，UTC 日；会话续期路径写入） |
| 阅读时长 time read | `trust_read_time`（客户端心跳，服务端钳制） |

## 3. 等级与晋升语义

| 等级 | 名称 | 达到方式 |
|---|---|---|
| TL0 | 新用户 | 注册默认 |
| TL1 | 基本用户 | 满足累计条件（§4）自动晋升 |
| TL2 | 成员 | 满足累计条件自动晋升 |
| TL3 | 活跃用户 | 满足滚动窗口条件自动晋升；**可能降级**（2 周宽限期后） |
| TL4 | 领导者 | 仅工作人员手动授予；自动评估永不改动 |

评估引擎（`backend/src/trust/service.rs`）：

1. 目标等级 = 1..=3 中满足全部条件的最高级（规则读 `trust_level_rules`，缺失回退代码内置默认）；**停用（is_enabled=0）的级不参与目标选择**（用户不会自动升入停用级）。
2. 当前 < 目标 → 晋升（事件 `reason='promotion'`）。
3. 当前 == 3 且目标 < 3 → TL3 窗口不达标，降级回 2（事件 `reason='demotion'`）；但最近一次到达 3 级事件后 `TL3_GRACE_MS`（2 周）内不降级；**TL3 规则被停用时脱离自动管理，不做降级**。
4. 其余情况不变；TL1/TL2 不会自动降级。
5. TL4 只能经 `POST /api/v1/admin/users/{user_id}/trust-level` 手动授予（唯一通道），事件 `reason='manual'` 并写审计 `admin.trust_level.set`；手动授予不受停用影响。

规则为**管理端可配置**（2026-09）：`PATCH /api/v1/admin/trust-levels/{level}`（If-Match + reason 审计）可编辑名称/摘要/启用/全部阈值条件；`POST .../reset` 恢复内置 LinuxDo 默认。改动立即生效于下一次评估（评估为惰性 + 事件驱动）。

评估触发点（惰性 + 事件驱动，与 Discourse 的每日批处理等效，保证展示即真实）：

- 登录用户 GET 帖子详情 → 记「进入话题」+ 评估；
- 登录用户 GET 楼层列表 → 记「阅读楼层」+ 评估；
- 会话续期（随 60s 续期节流）→ 记「当日访问」；
- 反应增删（toggle/DELETE）→ 同时评估表态方与内容作者；
- 处罚创建/撤销（mute/ban）→ 评估目标用户；
- GET `/me/trust-level` → 惰性评估后展示。

## 4. 默认阈值（种子 = linux.do 数值；管理端可编辑）

阈值条件即 `requirements_json` 键，**管理端可配置**（`PATCH /api/v1/admin/trust-levels/{level}`；未知键拒绝写入，TL0 禁止条件、TL4 强制 manual_only，TL1–TL3 禁止 manual_only）。下表为内置默认（种子/回退值；`POST .../reset` 恢复）：

`requirements_json` 键（缺省 = 不要求）：

| 键 | 语义 | TL1 | TL2 | TL3 |
|---|---|---|---|---|
| `topics_entered` | 累计进入话题 | 5 | 20 | — |
| `posts_read` | 累计阅读楼层 | 30 | 100 | — |
| `time_read_seconds` | 累计阅读时长（秒） | 600 | 3600 | — |
| `days_visited` | 累计访问天数 | — | 15 | — |
| `likes_given` | 累计送出的赞 | — | 1 | — |
| `likes_received` | 累计收到的赞 | — | 1 | — |
| `topics_replied_to` | 累计回复的不同话题 | — | 3 | — |
| `window_days` | >0 = 滚动窗口口径（天） | — | — | 100 |
| `visit_ratio` | 窗口内访问天数比例（required = ceil(ratio×window_days)） | — | — | 0.5（=50 天） |
| `replied_topics_window` | 窗口内回复的不同话题 | — | — | 10 |
| `viewed_ratio` / `viewed_cap` | 浏览窗口期新建话题：required = min(ceil(ratio×窗口期新建话题数), cap) | — | — | 0.25 / 500 |
| `read_ratio` / `read_cap` | 阅读窗口期新建楼层：同上 | — | — | 0.25 / 20000 |
| `likes_received_window` | 窗口内收到的赞 | — | — | 20 |
| `likes_given_window` | 窗口内送出的赞 | — | — | 30 |
| `like_distinct_user_divisor` | 多样性：不同用户数 ≥ 总数 / divisor | — | — | 5 |
| `like_distinct_day_divisor` | 多样性：不同天数 ≥ 总数 / divisor | — | — | 4 |
| `max_flags` | 被确认不当标记数上限（≤ 才达标） | — | — | 5 |
| `no_sanction_months` | 近 N 个月无禁言/封禁 | — | — | 6 |
| `manual_only` | 仅手动授予 | — | — | —（TL4 = true） |

点赞多样性口径（linux.do Wiki）：收到的赞须来自 ≥ 收赞数/5 个不同用户、≥ 收赞数/4 个不同天数；送出的赞对称地按不同收赞人 / 不同天数统计；私信不计（BBLBB 无私信点赞场景）。

窗口期「新建话题/楼层」分母为**全体用户**在该窗口内创建的对象数（排除草稿与已删除），与 Discourse 的 TL3 复核逻辑一致。

## 5. API（documented non-contract 端点，同 M12/M13 先例）

同既有运营/个人域扩展端点先例，不进入冻结 223-op 契约；登记于 `scripts/check-route-coverage.rb` `DOCUMENTED_NON_CONTRACT`。权限复用既有注册：`user.read_own` / `user.edit_own` / `level.manage`（无新增权限，`PERMISSION-MATRIX` 无需新行）。

| 端点 | 方法 | 权限 | 说明 |
|---|---|---|---|
| `/api/v1/me/trust-level` | GET | `user.read_own` | 当前等级 + 下一级逐项进度（`{level, name, summary, updated_at, grace_until, window, next_level:{level, name, summary, manual_only, eligible, requirements:[{key,label,current,required,met}]}}`）；惰性评估；`private, no-store` |
| `/api/v1/me/trust-level/read-time` | POST | `user.edit_own` + CSRF | 阅读心跳 `{"seconds": 1..=60}`（`additionalProperties: false`）；响应 `{credited_seconds, day_total_seconds}` |
| `/api/v1/admin/trust-levels` | GET | `level.manage` | 每级 `{level, name, summary, requirements, is_enabled, version, user_count}`（全量行，含停用） |
| `/api/v1/admin/trust-levels/{level}` | PATCH | `level.manage` + CSRF | 编辑单级规则：`{"name": "1..=50 字", "summary": "≤200 字|null", "is_enabled": bool, "requirements": {...}, "reason": "1..=500 字符"}`，`If-Match` = 当前 rule version（冲突 409）。requirements 未知键/负数/越界拒绝（TL0 无条件、TL1–TL3 禁 manual_only、TL4 强制 manual_only）；写审计 `admin.trust_level_rules.update`；响应 `{item}` |
| `/api/v1/admin/trust-levels/{level}/reset` | POST | `level.manage` + CSRF | 恢复该级为内置 LinuxDo 默认：`{"reason": "1..=500 字符"}`；version + 1；写审计 `admin.trust_level_rules.reset`；响应 `{item}` |
| `/api/v1/admin/users/{user_id}/trust-level` | POST | `level.manage` + CSRF | 手动设置 `{"level": 0..=4, "reason": "1..=500 字符"}`（`additionalProperties: false`）；响应 `{user_id, from_level, to_level, changed}`；写审计 `admin.trust_level.set` |

错误映射：未认证 401（`unauthenticated`）、无权限 403、体校验失败 400（`invalid_request`）、目标用户不存在 404。

### 5.1 后台管理界面

- **/admin/levels（等级管理，2026-09 合并单轨 + 规则可配置）**：原 /admin/trust-levels 独立路由并入。展示 TL0–TL4 每级规则（名称/摘要/阈值条目/用户数，`GET /admin/trust-levels`），每行「⋮」→「编辑规则」弹层（名称/摘要/启用开关 + 逐条阈值条件增删改，比例按 % 输入；保存 = PATCH，弹层内「恢复默认」= reset）、「手动设置信任等级」卡（user_id + 等级 + 原因 → `?/setLevel`）与「附件空间配额（按信任等级）」（`GET/PATCH /admin/levels/{level}/attachment-quota`，If-Match + reason 审计 + step-up），action 走 authedPost/authedPatch + CSRF，反馈用统一 action-toast。
- **/admin/users（用户管理）**：每行 LV 徽章旁新增 TL 徽章；行操作新增「信任」按钮 → Dialog（等级预选当前值 + 原因必填，`?/setTrust` → `POST /api/v1/admin/users/{id}/trust-level`）。管理用户投影（列表 + 单个 GET，`admin_user_json` / `admin_user_json_mysql`）新增 `trust_level` 字段（新增字段，不破坏旧客户端）。

## 6. 防滥用边界

- 统计写入全部带 user 维度唯一键（幂等），重复请求不放大计数。
- 阅读心跳服务端钳制：单请求 ≤60s、每人每日 ≤7200s；仅认证会话 + CSRF 可写。
- 进入话题/阅读楼层依赖真实内容读取路径（帖子详情、楼层列表），匿名不计。
- 被确认标记（resolved reports）与未撤销 mute/ban 直接参与 TL3 判定；处罚创建/撤销即时重评估。
- 已知简化（后续可收紧）：`viewed_ratio`/`read_ratio` 的分母不区分板块可见性；举报「确认」以 `status='resolved'` 近似版主确认语义；阅读心跳未接限流器（依赖钳制上限）。

## 7. 测试

- 纯逻辑：`backend/src/trust/rules.rs` `#[cfg(test)]`（阈值解析、0→1→2、窗口满分、点赞多样性、上限/标记/禁言、数学与报告形状，8 例）。
- 集成：`backend/tests/trust_levels.rs`（幂等与钳制、累计晋升、TL3 窗口晋升、宽限与降级、TL4 手动授予与审计、me/admin 路由鉴权与形状，7 例）。
