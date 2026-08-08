# BBLBB — 稳定错误码注册表

> 基线：v0.4（M13 扩展：theme/plugin 错误码）。错误响应统一使用 `application/problem+json`。`code` 是客户端和测试依赖的稳定机器码；`detail` 可本地化且不得泄漏内部信息。

| code | HTTP | 适用场景 | 客户端动作 |
|---|---:|---|---|
| `invalid_request` | 400 | JSON、参数或枚举无效 | 修正请求，不重试 |
| `theme_invalid` | 400 | 主题数据包/Token 违反封闭 schema（CSS/HTML/JS/SVG/远程资源/未知 key/非法值）或 kind 非 data | 修正数据包后重试 |
| `theme_incompatible` | 400 | 主题 `supports` range 与核心不兼容，或 schema_version 不支持 | 使用兼容版本 |
| `theme_not_found` | 404 | 主题不存在或不可选（偏好/管理操作） | 选择已安装主题 |
| `theme_conflict` | 409 | 主题已存在/内置或当前默认不可删除/修订乐观锁冲突 | 刷新后重试 |
| `plugin_invalid` | 400 | 插件 manifest/settings 违反封闭规则（未知 capability、危险 URL、代码内容、未知事件、schema 未知键）或 kind 非 config | 修正配置包 |
| `plugin_incompatible` | 400 | 插件 `supports` range 或 schema_version 不支持 | 使用兼容版本 |
| `plugin_not_found` | 404 | 插件不存在 | 检查插件 ID |
| `plugin_conflict` | 409 | 插件已安装/未停用不可卸载/policy_revision 乐观锁冲突 | 刷新后重试 |
| `visibility_level_exceeds_author` | 422 | 帖子/文章最低可见等级高于作者当前等级 | 降低最低可见等级后重试 |
| `invalid_url` | 400 | 视频或 Provider URL 无效 | 修正地址 |
| `idempotency_conflict` | 409 | 同一幂等键对应不同请求 | 使用新业务请求 ID |
| `version_conflict` | 409 | If-Match/version 过期 | 重新读取后合并 |
| `unauthorized` | 401 | 缺少或失效身份（Session 缺失/过期/Bearer 无效，M16-HARNESS-04 对齐实现） | 重新登录/刷新令牌 |
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
| `insufficient_funds` | 409 | 余额不足 | 不自动重复扣款 |
| `daily_limit_exceeded` | 409 | 用户/Client 日限额超出 | 等待下一个周期 |
| `checkout_interaction_invalid` | 409 | 托管确认 interaction 无效或过期 | 重新创建 Intent |
| `checkout_user_mismatch` | 403 | Session 用户与 Token/Intent 用户不一致 | 使用正确账号重新授权 |
| `checkout_intent_expired` | 409 | 结账意图过期 | 创建新意图 |
| `checkout_intent_consumed` | 409 | 意图已被其他请求消费 | 查询原 Purchase |
| `offer_version_changed` | 409 | 报价版本变化 | 重新读取 Offer |
| `refund_not_allowed` | 409 | 退款不符合政策 | 转人工或展示原因 |
| `marketplace_disabled` | 409 | Client/Scope 未激活、被禁用或紧急停用 | 联系管理员；历史交易仍可查询 |
| `marketplace_invalid_client` | 401 | Client 未知、非 Confidential 或凭证无效 | 修正 Client 凭证 |
| `refund_exceeds_purchase` | 409 | 累计退款超过原购买金额 | 使用剩余可退金额或转人工 |
| `merchant_balance_insufficient` | 409 | 商户余额不足，退款进入 requested 且新销售被冻结 | 管理员补足/冲正后重试 |
| `webhook_invalid_signature` | 401 | Webhook 签名校验失败（错误密钥/过期时间窗/event_id 不匹配） | 校验密钥与 5 分钟时间窗后重放 |
| `product_unavailable` | 409 | 内部商城商品未发布、过期或停售 | 重新读取商品 |
| `shop_purchase_limit_exceeded` | 409 | 商品限购或活动上限 | 不重试同一购买 |
| `shop_stock_exhausted` | 409 | 商品库存不足 | 重新读取商品 |
| `entitlement_not_usable` | 409 | 持有物过期、撤销或数量不足 | 卸下/重新购买 |
| `presentation_slot_conflict` | 409 | 装备版本或槽位冲突 | 重新读取衣柜 |
| `activity_already_claimed` | 409 | 当日自动签到/任务已领取 | 刷新活动摘要；页面访问无需报错或重试 |
| `activity_not_eligible` | 409 | 未达到任务条件或命中风控 | 展示安全原因 |
| `download_url_unavailable` | 503 | 已有授权但暂时无法签发 URL | 查询授权，不重复扣费 |
| `ai_consent_required` | 403 | AI 数据发送缺少独立同意 | 展示同意页 |
| `ai_budget_exceeded` | 409 | Provider/用途/用户预算超出 | 等待周期或改用人工 |
| `ai_suggestion_stale` | 409 | 目标 revision 已变化 | 重新生成建议 |
| `invalid_storage_request` | 400 | 存储参数/配置非法（路径越界、空 key、非法 TTL 等） | 修正请求后重试 |
| `storage_partial_upload` | 400 | 部分上传或 multipart 生命周期错误 | 重新发起上传 |
| `storage_forbidden` | 403 | 存储权限错误（S3 403/本地拒绝） | 联系管理员 |
| `storage_auth_failed` | 401 | 存储认证错误（S3 401） | 联系管理员 |
| `storage_rate_limited` | 429 | 存储供应商限流或并发上传超限 | 按 `Retry-After` 重试 |
| `quota_exceeded` | 409 | 配额不足（等级上限/总容量/日上传量/预留超卖） | 不自动重试 |
| `storage_conflict` | 409 | 存储请求冲突（S3 409/并发 complete） | 查询附件状态 |
| `storage_state_error` | 409 | 附件状态机非法迁移（如未 ready 附件关联公开内容） | 查询附件状态 |
| `storage_verification_failed` | 409 | 大小/hash/Content-Type 与声明不符 | 重新上传 |
| `storage_network_error` | 503 | 存储网络超时/DNS/TLS 错误（瞬时） | 按幂等语义重试 |
| `storage_upstream_error` | 503 | 存储供应商 5xx 或未知服务错误（瞬时） | 按幂等语义重试 |
| `bad_request` | 400 | 通用参数错误（未细分稳定码的 400） | 修正请求，不重试 |
| `conflict` | 409 | 通用冲突（未细分稳定码的 409） | 刷新后重试 |
| `not_implemented` | 501 | 已登记但尚未实现的占位操作 | 等待上线 |
| `video_insecure_scheme` | 422 | 视频 URL 非 HTTPS | 使用 HTTPS 链接 |
| `video_invalid_url` | 422 | 视频 URL 结构无效 | 修正地址 |
| `video_host_invalid` | 422 | 视频 URL Host 非精确白名单 | 使用支持的主机 |
| `video_port_not_allowed` | 422 | 视频 URL 端口非 443 | 使用 443 链接 |
| `video_private_ip` | 422 | 视频 URL 解析为私网/loopback 地址 | 使用公网链接 |
| `video_signed_url` | 422 | 视频 URL 带签名参数 | 使用干净来源 |
| `video_userinfo_not_allowed` | 422 | 视频 URL 含 userinfo | 去除账号信息 |
| `video_fragment_not_allowed` | 422 | 视频 URL 含不允许的 fragment | 去除片段 |
| `video_unsupported_type` | 422 | 视频类型不支持 | 使用支持的类型 |
| `video_not_video_page` | 422 | 非视频页面 URL | 使用视频页链接 |
| `video_invalid` | 422 | 视频引用校验失败 | 重新解析 |
| `video_mime_mismatch` | 422 | MIME 与声明不符 | 更换来源或降级 |
| `video_no_embed_permission` | 403 | 平台无嵌入权限 | 使用外链 |
| `video_takedown` | 409 | 来源、版权或策略阻止视频 | 移除或使用外链 |
| `video_provider_disabled` | 409 | 视频 Provider 被停用 | 等待管理员开启 |
| `video_provider_host_not_allowed` | 422 | Provider Host 不在白名单 | 使用支持的主机 |
| `video_provider_ratelimited` | 429 | Provider 限流 | 按 `Retry-After` 重试 |
| `video_provider_unavailable` | 503 | Provider 暂时不可用 | 稍后重试或降级外链 |
| `video_policy_changed` | 409 | 引用策略已收紧 | 降级外链并重检 |
| `video_policy_version_conflict` | 409 | 客户端策略版本过期 | 重新读取策略 |
| `video_poster_attachment_invalid` | 400 | 封面附件引用无效 | 重新上传封面 |
| `video_resolution_expired` | 409 | 解析结果过期 | 重新解析 |
| `video_embed_not_found` | 404 | 视频引用不存在 | 创建新引用 |
| `video_embed_referenced` | 409 | 视频引用已被使用 | 查询原引用 |
| `video_target_conflict` | 409 | 目标内容引用冲突 | 查询原引用 |
| `video_target_forbidden` | 403 | 目标内容无权限 | 不重试 |
| `video_target_not_found` | 404 | 目标内容不存在 | 检查内容 |
| `video_version_conflict` | 409 | 视频引用版本冲突 | 刷新后重试 |
| `video_egress_http_error` | 502 | 出站探测 HTTP 错误 | 稍后重试 |
| `video_egress_private_ip` | 422 | 出站探测命中私网 | 使用公网链接 |
| `video_egress_timeout` | 504 | 出站探测超时 | 稍后重试 |
| `video_egress_too_large` | 422 | 出站响应过大 | 使用更小来源 |
| `video_egress_too_many_redirects` | 422 | 出站重定向过多 | 使用直连链接 |
| `video_egress_unavailable` | 503 | 出站探测源不可用 | 稍后重试 |
| `video_hls_invalid` | 422 | HLS playlist 结构无效 | 使用更小流或外链 |
| `video_hls_depth_exceeded` | 422 | HLS 递归深度超限 | 使用更小流或外链 |
| `video_hls_segment_count_exceeded` | 422 | HLS 分片数量超限 | 使用更小流或外链 |
| `video_hls_duration_exceeded` | 422 | HLS 总时长超限 | 使用更小流或外链 |
| `video_hls_cross_origin_segment` | 422 | HLS 分片跨域不合法 | 使用更小流或外链 |
| `video_hls_key_not_allowed` | 422 | HLS Key 不合法 | 使用更小流或外链 |
| `video_hls_map_not_allowed` | 422 | HLS Map 不合法 | 使用更小流或外链 |
| `video_hls_signed_uri` | 422 | HLS 分片带签名参数 | 使用干净来源 |
| `internal_error` | 500 | 未分类内部错误 | 使用 request ID 联系管理员 |

- 503 响应不得表示数据库事务已成功，除非同时返回明确的资源 ID 和上述可查询状态码。
- `download_url_unavailable` 不得重复扣费；客户端使用原 authorization 或幂等键恢复。
- 新错误码必须更新本表、OpenAPI、前端映射和测试向量；错误码语义不能复用。
- 可见性（M04-VISIBILITY）：读路径未解锁**不是错误**——响应 200，正文键缺失并返回 `access_summary`；`visibility_level_exceeds_author`（422）只用于写路径等级越级。grant 查询（`content_access_grants`）失败时评估 fail-closed（按未解锁处理），不产生错误响应。
