# BBLBB — API 资源与 DTO 冻结基线

> 基线：v0.5。本文补充 `API.md` 的资源级契约；机器可读事实来源为 [`../openapi/openapi.yaml`](../openapi/openapi.yaml)。OpenAPI 必须逐项表达本文、`PERMISSION-MATRIX.md`、`ERROR-CODES.md` 和 `STATE-MACHINES.md`，不得重新发明字段。所有未标记可空的字段均 required；所有 ID 为 UUID v7 字符串，时间为 Unix 毫秒整数。

## 1. 公共结构

```text
ResourceMeta { id, version, created_at, updated_at }
Page<T> { items: T[], next_cursor: string|null, has_more: boolean }
TaskAccepted { task_id, status:"queued", poll_url, cancel_url|null, source_revision|null, policy_version }
Money { currency:"coin"|string, amount:int64 }
```

- `null` 与字段缺失不可混用；无权限字段必须缺失，业务上“没有值”才返回 `null`。
- 更新请求携带 `If-Match` 或 `version`；创建响应 201，异步任务 202，幂等重放保持首次业务状态和资源 ID。

## 2. 核心资源

| 资源 | 端点 | 请求关键字段 | 成功投影 |
|---|---|---|---|
| Session | `POST /auth/login`、`DELETE /auth/session`、`GET /me` | identifier（用户名或邮箱）、password、totp 可选 | `Me`，Session Cookie 单独设置 |
| User | `GET/PATCH /me`、`GET /users/{username}` | display_name、bio、timezone、theme | `Me` 或 `PublicUser`，不得共用巨型 DTO |
| Board | `GET /boards`、`GET /boards/{slug}`、admin CRUD | name、slug、description、parent_id、version | `BoardProjection` 含当前请求方 capabilities |
| Post | `GET/POST /posts`、`GET/PATCH/DELETE /posts/{id}` | type、title、markdown、board_id、access_policy、scheduled_at | `PostProjection`，隐藏正文不出现在未授权响应 |
| Comment | `GET/POST /posts/{id}/comments`、`PATCH/DELETE /comments/{id}` | markdown、parent_id 可选、version | `CommentProjection` |
| Tag | `GET /tags`、admin CRUD | name、slug、group、version | `TagProjection` |
| Report | `POST /reports`、`GET/PATCH /admin/moderation/*` | target、reason_code、details | 权限分级投影 |
| Notification | `GET /notifications`、`POST /notifications/{id}/read` | version | 不含不可见目标摘要 |
| Attachment | `POST /attachments`、`POST /attachments/{id}/complete`、GET/DELETE | filename、size、declared_media_type、target context | 状态、配额、短效上传参数 |

```text
PostCreate {
  type, title, markdown, board_id, visibility_level:int32,
  access_policy, scheduled_at|null, client_request_id
}
```

- `visibility_level` 最小值为 1，服务端必须重新读取作者当前等级并拒绝 `visibility_level > author.level`，返回 `422 visibility_level_exceeds_author`。
- 前端可以隐藏超出当前等级的选项，但这不是安全边界；草稿保存、发布、定时发布和管理员代发均必须走同一服务端校验。
- 创建成功后，作者至少拥有查看自己帖子元数据和正文的权限；不得创建作者本人无法访问的最低等级策略。

`PostProjection` 至少包含：`id, version, post_type, title, author, board, status, closed_at, published_at, capabilities, access_summary`。只有授权后才包含 `body_html` 和当前可见的受限块；未授权正文不是 `null`，而是完全缺失。

## 3. Download Billing

```text
DownloadRequest {
  target_type: "post"|"comment"|null,
  target_id: uuid|null,
  expected_policy_version: int64|null,
  client_request_id: string
}
DownloadResult {
  authorization_id, attachment_id,
  charged: Money,
  download_url, url_expires_at,
  authorization_expires_at,
  reused_authorization
}
DownloadAuthorizationProjection {
  id, attachment_id, status, charged: Money,
  valid_from, expires_at, can_sign_url
}
```

- `POST /attachments/{id}/download` 不接受 user、owner、amount 或 currency。
- 已扣费但 URL 签发失败返回 Problem `download_url_unavailable`，扩展字段固定为 `authorization_id, authorization_expires_at, charged, reused_authorization`。

## 4. Marketplace

```text
OfferCreate {
  external_offer_id, title, description,
  currency_id, unit_amount:int64, quantity_min:int32, quantity_max:int32,
  stock_policy:"unlimited"|"finite", stock_remaining:int64|null
}
OfferProjection { id, external_offer_id, version, status, title, description_safe, price:Money, stock_policy, stock_remaining }
CheckoutIntentCreate { offer_id, expected_offer_version, merchant_order_id, quantity:int32 }
CheckoutIntentProjection {
  id, status, expires_at, offer_snapshot,
  amount:Money, platform_fee:Money, interaction_id, confirmation_url
}
PurchaseProjection { id, client_order_ref, status, offer_id, offer_version, quantity, amount:Money, refunded:Money, committed_at }
RefundCreate { amount:Money|null, reason_code, merchant_refund_id }
RefundProjection { id, purchase_id, status, amount:Money, created_at, completed_at|null }
```

- Checkout Token 必须 user-bound；请求不含 `user_id`、收款账户、价格或费率。
- Marketplace 响应不返回内部 User ID、完整余额、Merchant Account 余额或账本明细。
- 托管确认页 GET 返回 Session 用户可见快照；POST 只接受 `interaction_id, decision, expected_intent_version` 并要求 CSRF。

## 5. AI

```text
AiTaskProjection { id, purpose, status, source_revision, policy_version, created_at, completed_at|null, error_code|null, suggestion_id|null }
AiSuggestionProjection { id, task_id, type, status, base_version, schema_version, payload, created_at }
AiConsentCreate { provider_id, purpose, data_mode:"full_with_consent", disclosure_version, disclosure_hash }
```

Suggestion payload：

- `formatting.v1`：`replacement_markdown, diff_summary[]`。
- `seo.v1`：`seo_title, description, canonical_url|null, keywords[]`。
- `tagging.v1`：`tags[], summary`。
- `moderation.v1`：`risk_categories[], confidence_basis, reviewer_notes`，仅审核权限投影。

接受请求只含 `expected_base_version` 和可选用户选中的 suggestion 子集；服务端重新验证全部字段。

## 6. Video

```text
VideoResolveRequest { source_url, target_type:"post"|"comment", target_id|null }
VideoResolveResult {
  resolution_id, provider:"direct"|"hls"|"xigua",
  canonical_url, media_type|null, title|null, poster_url|null,
  duration_seconds|null, render_mode:"native"|"official_iframe"|"external_link",
  warnings[], policy_version, expires_at
}
VideoEmbedCreate { resolution_id, target_type, target_id, expected_policy_version }
VideoEmbedPatch { title_override|null, poster_override_attachment_id|null, version }
VideoEmbedProjection { id, provider, canonical_url, title, poster_url, duration_seconds, status, render_mode, source_label, capabilities, policy_version }
```

- 创建只接受短效 `resolution_id`，不再次接受可信 MIME、iframe HTML、Key、签名流 URL或任意 metadata。
- `POST /video-embeds/{id}/refresh` 返回 `TaskAccepted`。
- Render Projection 由服务端按当前 CSP/权限生成；前端不能自行把 `source_url` 拼进 iframe。

## 7. Internal Shop、装扮与活跃

```text
ShopProductProjection {
  id, slug, version, kind, title, description_safe, icon_token,
  price:Money, stock_remaining|null, required_level|null,
  validity_seconds|null, sale_start_at|null, sale_end_at|null,
  slot, owned, purchasable, unavailable_reason|null
}
ShopOrderCreate { product_id, expected_product_version, quantity:int32, client_request_id }
ShopOrderProjection { id, product_snapshot, quantity, charged:Money, status, entitlement_ids[], committed_at }
EntitlementProjection { id, product, status, quantity, remaining_quantity, valid_from, expires_at|null, equipped }
PresentationProjection { user_id, version, nickname, avatar, badges[], profile, post_author, reduced_motion }

ProfileCoverProjection {
  attachment_id, content_url, width, height, alt_text, position, updated_at
}

- `content_url` 必须由服务端根据资料可见性签发或代理，不能接受客户端任意远程 URL。
- Cover 使用附件 `attachment_id` 关联；S3 临时 URL 到期只使 URL 失效，不删除附件。
- 更新和移除 Cover 必须校验当前用户、附件所有权、MIME、像素、大小和附件配额，并写审计事件。
- Cover 与头像、帖子图片和普通附件共享用户 `total_bytes`；响应中的 `used_bytes/remaining_bytes` 必须包含 Cover 的 `quota_bytes_charged`，更换或移除后直到旧对象完成物理清理才释放对应容量。

UserHoverCardProjection {
  user_id, nickname, avatar, cover|null, level, roles[], bio_safe,
  post_count, reply_count, contribution, profile_url, presentation_version
}

- 帖子、文章和回复中的作者 Hover Card 使用同一安全投影，不包含邮箱、登录状态、精确 IP、私有资产或签名参数。
- Cover 通过稳定内容 URL 加载；未授权、处理失败或链接过期时降级为内置背景，不能影响内容列表渲染。
ActivitySummary { checked_in_today, streak_days, today_earned:Money[], point_operation_id|null }
```

- `ShopOrderCreate` 不接受价格、货币、用户、库存或展示 Token。
- Equip 请求只接受 `expected_presentation_version`；Entitlement 和 Slot 由服务端读取。
- 公共 User/Post/Comment DTO 只嵌入编译后的 `PresentationProjection` 子集，不返回 entitlement、订单或余额。
- `POST /activity/visit` 由已认证页面访问触发，不要求用户手动签到；成功可返回 Activity Claim 的 `point_operation_id` 公共引用、奖励和新摘要，不返回内部风控判定。
- 同日重复访问可以返回既有摘要或 `activity_already_claimed`，前端必须静默处理；服务端唯一键为 `user_id + rule_id + activity_day`。

## 8. 管理 Policy DTO

所有管理 Policy 使用：

```text
PolicyProjection { provider_or_scope, enabled, version, effective_at, editable_fields, values, secret_configured? }
PolicyPatch { expected_version, reason, changes }
```

- `changes` 字段由每个策略的 OpenAPI schema 白名单限定；未知键 400。
- Secret 只在专用 rotate/write-only 字段提交，GET 永不回显。
- 成功更新返回新 `version` 和脱敏 diff；冲突返回 `policy_version_changed`。

## 8. 契约完成定义

每个 OpenAPI operation 必须声明：operationId、tags、security、scope/permission 扩展、CSRF、Idempotency-Key、If-Match、请求/响应 schema、全部 Problem code、Cache-Control、示例和字段可见性。缺少其中任一项的端点不视为冻结。
