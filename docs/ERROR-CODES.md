# BBLBB — 稳定错误码注册表

> 基线：v0.4。错误响应统一使用 `application/problem+json`。`code` 是客户端和测试依赖的稳定机器码；`detail` 可本地化且不得泄漏内部信息。

| code | HTTP | 适用场景 | 客户端动作 |
|---|---:|---|---|
| `invalid_request` | 400 | JSON、参数或枚举无效 | 修正请求，不重试 |
| `visibility_level_exceeds_author` | 422 | 帖子/文章最低可见等级高于作者当前等级 | 降低最低可见等级后重试 |
| `invalid_url` | 400 | 视频或 Provider URL 无效 | 修正地址 |
| `idempotency_conflict` | 409 | 同一幂等键对应不同请求 | 使用新业务请求 ID |
| `version_conflict` | 409 | If-Match/version 过期 | 重新读取后合并 |
| `authentication_required` | 401 | 缺少或失效身份 | 重新登录/刷新令牌 |
| `invalid_token` | 401 | Bearer 无效、过期或撤销 | 重新授权 |
| `forbidden` | 403 | 无权限但资源存在 | 不重试 |
| `step_up_required` | 403 | 高风险操作要求近期重认证（M02-MFA-07） | 经 `/api/v1/auth/re-auth` 重认证后重试 |
| `not_found` | 404 | 资源不存在或按策略隐藏 | 不枚举重试 |
| `csrf_failed` | 403 | Session 写请求缺少/错误 CSRF | 获取新 CSRF 后重试 |
| `origin_not_allowed` | 400 | Cookie 写请求 Origin（缺则 Referer）不匹配 Host 或 allowed_origins | 从允许来源发起请求 |
| `host_not_allowed` | 400 | 严格模式（allowed_hosts 已配置）下 Host 不在允许列表 | 使用允许的主机名 |
| `rate_limited` | 429 | 用户、IP、对象或 Provider 限流 | 按 `Retry-After` 重试 |
| `crawler_denied` | 403 | AI 训练/抓取爬虫类别默认拒绝（M08-CRAWL-08） | 停止抓取；不影响已授权用户 |
| `challenge_required` | 403 | 需要一次性挑战 token（响应头 `X-BBLBB-Challenge`） | 重试并携带该 token（M08-CRAWL-06） |
| `temporarily_banned` | 403 | 行为风控临时封禁（M08-CRAWL-07） | 按 `Retry-After` 稍后重试；可向管理员申诉复核 |
| `feature_disabled` | 409 | 功能或 Provider 当前关闭 | 等待管理员开启 |
| `policy_disabled` | 409 | 当前范围策略关闭 | 不重试同一操作 |
| `policy_version_changed` | 409 | 客户端策略版本过期 | 重新读取策略 |
| `insufficient_funds` | 409 | 余额不足 | 不自动重复扣款 |
| `daily_limit_exceeded` | 409 | 用户/Client 日限额超出 | 等待下一个周期 |
| `checkout_interaction_invalid` | 409 | 托管确认 interaction 无效或过期 | 重新创建 Intent |
| `checkout_user_mismatch` | 403 | Session 用户与 Token/Intent 用户不一致 | 使用正确账号重新授权 |
| `checkout_intent_expired` | 409 | 结账意图过期 | 创建新意图 |
| `checkout_intent_consumed` | 409 | 意图已被其他请求消费 | 查询原 Purchase |
| `offer_version_changed` | 409 | 报价版本变化 | 重新读取 Offer |
| `refund_not_allowed` | 409 | 退款不符合政策 | 转人工或展示原因 |
| `product_unavailable` | 409 | 内部商城商品未发布、过期或停售 | 重新读取商品 |
| `product_version_changed` | 409 | 商品价格/库存版本变化 | 展示新价格并重新确认 |
| `shop_purchase_limit_exceeded` | 409 | 商品限购或活动上限 | 不重试同一购买 |
| `shop_stock_exhausted` | 409 | 商品库存不足 | 重新读取商品 |
| `entitlement_not_usable` | 409 | 持有物过期、撤销或数量不足 | 卸下/重新购买 |
| `presentation_slot_conflict` | 409 | 装备版本或槽位冲突 | 重新读取衣柜 |
| `activity_already_claimed` | 409 | 当日自动签到/任务已领取 | 刷新活动摘要；页面访问无需报错或重试 |
| `activity_not_eligible` | 409 | 未达到任务条件或命中风控 | 展示安全原因 |
| `attachment_not_ready` | 409 | 附件仍在处理或隔离 | 查询附件状态 |
| `download_authorization_pending` | 202 | 授权已提交，URL尚未签发 | 查询授权/用原幂等键重试 |
| `download_url_unavailable` | 503 | 已有授权但暂时无法签发 URL | 查询授权，不重复扣费 |
| `media_blocked` | 409 | 来源、版权或策略阻止视频 | 移除或使用外链 |
| `media_probe_failed` | 422 | MIME/playlist 探测失败 | 修正来源或降级 |
| `hls_policy_exceeded` | 422 | HLS 深度、分片或字节预算超限 | 使用更小流或外链 |
| `provider_unavailable` | 503 | 外部 Provider 暂时不可用 | 按任务状态重试 |
| `ai_consent_required` | 403 | AI 数据发送缺少独立同意 | 展示同意页 |
| `ai_budget_exceeded` | 409 | Provider/用途/用户预算超出 | 等待周期或改用人工 |
| `ai_suggestion_stale` | 409 | 目标 revision 已变化 | 重新生成建议 |
| `job_not_retryable` | 409 | 任务永久错误或已结束 | 展示失败，不重试 |
| `storage_unavailable` | 503 | 本地/S3 暂时不可用 | 按幂等语义重试 |
| `internal_error` | 500 | 未分类内部错误 | 使用 request ID 联系管理员 |

- 503 响应不得表示数据库事务已成功，除非同时返回明确的资源 ID 和上述可查询状态码。
- `download_url_unavailable` 不得重复扣费；客户端使用原 authorization 或幂等键恢复。
- 新错误码必须更新本表、OpenAPI、前端映射和测试向量；错误码语义不能复用。
- 可见性（M04-VISIBILITY）：读路径未解锁**不是错误**——响应 200，正文键缺失并返回 `access_summary`；`visibility_level_exceeds_author`（422）只用于写路径等级越级。grant 查询（`content_access_grants`）失败时评估 fail-closed（按未解锁处理），不产生错误响应。
