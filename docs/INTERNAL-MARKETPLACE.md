# BBLBB — 内部积分商城与社区活跃系统

> 基线：v0.4。内部商城是站点核心经济模块，不是公开 Marketplace；用户使用站内 `coin` 购买装扮、身份展示和论坛互动道具。所有扣费、持有、装备和撤销由 Rust 核心服务裁决。

## 1. 目标与边界

内部商城用于消耗社区积分，增强身份表达和论坛活跃度：

- 昵称颜色、昵称渐变、头像挂件、头像边框、等级徽章外观。
- 个人主页、帖子作者行、回复楼层、通知、排行榜等全局用户展示位置统一生效。
- 主题皮肤、签名装饰、帖子/回复互动特效、感谢/鼓掌/庆祝等非现金社区反应。
- 每日签到、连续活跃、任务、成就和周榜奖励，用于获得 `exp/coin/contribution`。

非目标：

- 不兑换现金、法币、站外资产或公开市场余额。
- 不允许用户自定义 HTML/CSS/JavaScript、远程图片、iframe、脚本或任意动画。
- 不购买权限、审核结果、内容可见性、管理员角色、封禁豁免或积分调整能力。
- 不把“活跃”设计成刷屏；奖励受每日上限、冷却、反滥用和质量条件约束。

## 2. 商品类型

| `kind` | 示例 | 持有方式 | 装备方式 |
|---|---|---|---|
| `cosmetic_nickname` | 蓝色昵称、彩虹昵称 | 永久/限时 | 同一槽位一件 |
| `cosmetic_avatar` | 圆环、发光边框 | 永久/限时 | `avatar_frame` 槽位 |
| `cosmetic_avatar_attachment` | 小猫、星球、徽章挂件 | 永久/限时 | `avatar_attachment` 槽位 |
| `cosmetic_badge` | 论坛贡献者、早期成员 | 永久 | `profile_badge` 多选有限 |
| `profile_effect` | 个人主页背景纹理 | 永久/限时 | 同一槽位一件 |
| `post_effect` | 首帖高亮、感谢卡片 | 消耗品/限时 | 发帖或互动时使用 |
| `reaction_pack` | 鼓掌、围观、庆祝反应 | 数量型 | 回复/帖子 Reaction |
| `title_prefix` | “夜猫子”“热心居民” | 永久/限时 | 昵称前缀槽位 |
| `utility` | 改名卡、撤回编辑卡 | 消耗品 | 受严格规则限制 |

商品内容使用安全 JSON/Token，不保存用户可执行代码。商品可以有 `required_level`、`start_at`、`end_at`、库存和购买次数限制。

## 3. 展示槽位

```text
nickname_decoration
nickname_color
avatar_frame
avatar_attachment
profile_badges
profile_effect
post_author_effect
reaction_pack
```

- 同一槽位只能装备一个（`profile_badges` 允许最多 3 个）。
- 装备不扣费；购买后可以随时卸下或换装。
- 限时商品到期自动转为未装备，不删除持有历史。
- 用户被封禁、受限或隐私设置关闭时，安全/隐私规则优先，展示可降级为默认样式。
- 所有用户投影只返回经过服务端白名单编译的 `presentation_tokens`，前端不解释任意 CSS。

## 4. 购买与账务

购买使用单一站内 `coin` 货币，金额为非负整数最小单位。商品价格和库存来自服务端快照，客户端不能提交价格、用户、货币或收款方。

购买事务：

```text
Idempotency-Key
→ 锁定商品/版本/库存
→ 锁定用户 coin account
→ 校验用户状态、等级、限购、冷却和风险限制
→ 创建 internal_order
→ 追加 point_operation(kind=shop_purchase)
→ 更新 point_accounts
→ 创建 user_entitlement / consumable_balance
→ 写 audit_logs + outbox_events
→ 提交
```

- 同一个 `Idempotency-Key` 只产生一次扣费和一次持有记录。
- 库存不足、余额不足、商品下架、版本冲突和限购冲突不产生扣费。
- 数字装扮默认不退款；管理员退款必须是补偿 operation，撤销 entitlement，不编辑历史交易。
- 商品价格变更创建新版本；已完成订单和已持有商品保留原快照。
- 限时商品的有效期从购买提交时间起算，除非商品明确使用活动固定时间窗。

## 5. 运营与活跃系统

### 签到

- 不要求用户点击签到；登录用户每日首次有效页面访问由服务端自动尝试领取。
- 每个用户每天最多领取一次，按用户配置时区（缺失时回退站点时区）定义自然日，服务端记录 `activity_day`。
- “有效访问”必须是通过 Session/Bearer 鉴权并进入正常业务页面的请求；健康检查、静态资源、预取、爬虫、失败请求、后台任务和匿名访问不能触发。
- 连续签到奖励设置上限和断签规则；补签必须消耗专门道具或 coin，并受月度次数限制。
- 自动领取使用 `user_id + rule_id + activity_day` 唯一键，所有并发页面请求共享同一幂等结果，不重复奖励。

### 活跃任务

任务分为：发帖、有效回复、被点赞/收藏、完善资料、参与审核、连续访问等。奖励必须有：

- 每日/每周上限。
- 同一目标去重键。
- 最短内容和冷却规则。
- 风控状态和撤销机制。
- 不能因自我回复、自点赞、重复编辑、批量刷反应、删除后重发获得重复奖励。

### 社区氛围道具

- Reaction 使用有限频率和每日总量；可配置是否通知作者。
- 感谢、鼓掌、庆祝只表达社区互动，不改变内容排序、审核、权限或现金价值。
- 活跃榜默认展示脱敏统计；榜单奖励必须写入不可变账本并可追溯。

## 6. 后台与前端入口

用户页面：`/shop`、`/shop/{slug}`、`/me/closet`、`/activity`。

管理页面：`/admin/shop`、`/admin/shop/products`、`/admin/activity`、`/admin/shop/orders`。

商品详情必须显示价格、库存、等级门槛、有效期、退款规则和展示位置；购买确认显示当前余额、扣除金额、购买后余额和不可退款提示。

后台可以配置：商品名称、说明、图标、Token、价格、货币、库存、销售时间、等级门槛、用户限购、有效期、展示槽位、安全审核状态、是否可退款和活动标签。上传资源必须经过附件安全处理；商品图标优先使用内置图标或受控附件，禁止远程 URL。

## 7. 管理 API

```text
GET/PATCH /api/v1/admin/shop/config
GET/POST/PATCH /api/v1/admin/shop/products
POST       /api/v1/admin/shop/products/{id}/publish
POST       /api/v1/admin/shop/products/{id}/disable
GET        /api/v1/admin/shop/orders
POST       /api/v1/admin/shop/orders/{id}/refund
GET/PATCH  /api/v1/admin/activity/config
GET/POST/PATCH /api/v1/admin/activity/tasks
```

## 8. 用户 API

```text
GET  /api/v1/shop/products?category=cosmetic&after=...
GET  /api/v1/shop/products/{id}
POST /api/v1/shop/orders
GET  /api/v1/shop/orders/{id}
GET  /api/v1/me/entitlements
POST /api/v1/me/entitlements/{id}/equip
POST /api/v1/me/entitlements/{id}/unequip
GET  /api/v1/me/presentation
POST /api/v1/activity/visit
GET  /api/v1/activity/summary
POST /api/v1/posts/{id}/reactions
DELETE /api/v1/posts/{id}/reactions/{reaction}
POST /api/v1/comments/{id}/reactions
DELETE /api/v1/comments/{id}/reactions/{reaction}
```

`POST /shop/orders` 要求 `Idempotency-Key`，只接受 `product_id`、`expected_product_version`、`quantity` 和客户端请求 ID。购买成功返回订单、扣费金额和 entitlement；不返回内部账本详情。

## 9. 安全与隐私

- 商城购买不是公开 Marketplace，不开放外部 Client Scope。
- 余额、商品价格、库存、等级门槛和奖励均由后端重新计算。
- 装扮 Token 必须是后端注册的有限枚举；禁止 style 字符串、HTML、SVG、脚本、远程字体和远程图片。
- 用户可关闭他人的昵称装饰、动画和互动通知；尊重隐私设置和减少动效偏好。
- 禁止隐藏内容泄漏到商品说明、榜单、通知和 AI 输入。
- 交易、装备、活动奖励、撤销、管理员调整和退款均写不可变审计。
- 商品图标和挂件不能用于冒充管理员、官方认证或安全状态；官方徽章使用受保护的系统商品类型。

## 10. 测试门槛

- 幂等重复购买、并发库存、并发余额、商品换版、限购、等级门槛、限时过期。
- SQLite `BEGIN IMMEDIATE` 与 MySQL/MariaDB 行锁等价。
- 退款/补偿不修改历史账本，余额恒等式成立。
- 装备互斥、最多 3 个徽章、过期自动卸下、封禁/隐私降级。
- Token 注入、XSS、远程 URL、恶意 SVG、CSS 逃逸、无障碍和减少动效。
- 签到时区、重复签到、补签、任务去重、自我互动、批量刷活跃、反应限流。
- 排行榜和通知不泄漏隐藏内容，核心发帖不依赖商城/活动 Provider。
