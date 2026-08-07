// BBLBB DTO 类型接入层（M00-FRONTEND-03）
//
// `frontend/src/lib/api/generated/v1/`（由 `ruby scripts/generate-ts-types.rb`
// 从 openapi/openapi.yaml 生成，禁止手改）是 API DTO 的唯一类型来源。
//
// 本文件是接入层，只做两类声明：
//   1. re-export generated 中与前端 1:1 对应的类型
//      （Page、Money、Author、Health、CsrfToken、各类请求 DTO 与枚举…）；
//   2. 把当前实现返回的列表/详情投影声明为 generated 基类型的显式组合：
//      `Omit<ContractX, '后端尚未实现的契约字段'> & { 实现扩展字段 }`，
//      使公共字段（id/username/email/slug/name…）始终来自契约生成源。
//
// 后端按契约收敛后（M1-M4），第 2 类投影会塌缩为纯 re-export，页面无需改动。
// 任何契约变更先重新生成再联动本文件；client.ts 与页面继续从
// `$lib/api/client`（或 `$lib/api/types`）引用，无需感知本层变化。

import type {
  Me as ContractMe,
  Board as ContractBoard,
  Page as ContractPage,
  PublicUser as ContractPublicUser,
  SearchResult as ContractSearchResult,
  Money,
} from './generated/v1';

// ── 与契约 1:1 对应的类型（re-export，唯一来源）────────────────────────────────

export type {
  RegisterRequest,
  LoginRequest,
  ProfilePatch,
  PostCreate,
  PostPatch,
  CommentCreate,
  TokenRequest,
  PasswordResetRequest,
  PasswordResetConfirm,
  CsrfToken,
  Health,
  PublicUser,
  Author,
  Money,
  Page,
  SearchResult,
  SearchPage,
  DeviceSession,
  DraftCreate,
  DraftPatch,
  // M10 契约请求体（generated 来源；响应为自建投影见文件尾部）
  VideoResolveRequest,
  VideoEmbedCreate,
  VideoEmbedPatch,
} from './generated/v1';

export type {
  ProblemCode,
  PostCreateType,
  PostCreateAccessPolicy,
  PostPatchAccessPolicy,
  AccessSummaryPolicy,
  SearchResultType,
  ReportCreateTargetType,
  ReportCreateReasonCode,
  SanctionCreateType,
  // M6/M7 契约类型（generated 兜底，供 client/页面直接引用）
  AttachmentCreate,
  AttachmentComplete,
  DownloadRequest,
  DownloadResult,
  EntitlementEquip,
  ShopOrderCreate,
  ReactionCreate,
  ActivityVisitResult,
  ProfileCoverSet,
} from './generated/v1';

// ── 实现投影：以 generated 基类型组合，公共字段来自契约 ────────────────────────

/** 当前用户（GET /me）投影：契约 Me + 实现扩展字段（status/display_name/bio/
 * timezone 可空映射）。后端补齐 created_at/updated_at 后，可塌缩为
 * `export type User = Me`。version 已由后端 Me 返回（M03-PROFILE-04 乐观并发
 * 来源），保留为 If-Match 依据。 */
export type User = Omit<
  ContractMe,
  'created_at' | 'updated_at' | 'mfa_enabled' | 'display_name'
> & {
  status: string;
  display_name: string | null;
  mfa_enabled: boolean;
};

/** 公开用户资料（GET /users/{username}）投影：对应后端 PublicProfile DTO
 *  （M03-PROFILE-01），严格公开字段（不含邮箱/状态/Session/IP）。 */
export type PublicProfile = ContractPublicUser & {
  display_name: string | null;
  bio: string | null;
  avatar_attachment_id: string | null;
  signature: string | null;
};

/** 板块（GET /boards）投影：契约 Board（parent_id/visibility/posting_mode/
 * post_count 仅已认证请求方可见，M03-BOARDS-08 防匿名计数/面包屑推断）。
 */
export type Board = Omit<
  ContractBoard,
  | 'version'
  | 'created_at'
  | 'updated_at'
  | 'description'
  | 'post_count'
  | 'is_active'
  | 'parent_id'
  | 'visibility'
  | 'posting_mode'
> & {
  /** Unix 毫秒（ResourceMeta 契约用字符串，前端按时间戳处理）。 */
  version: number;
  created_at: number;
  updated_at: number;
  description: string | null;
  parent_id?: string | null;
  visibility?: 'public' | 'members' | 'restricted' | 'hidden' | null;
  posting_mode?: 'normal' | 'approval' | 'readonly' | 'closed' | null;
  /** 已认证投影才返回（匿名公开投影恒缺）。 */
  post_count?: number;
  /** 后端返回 0/1 整数（活跃投影恒 1）；已认证投影才返回。 */
  is_active?: number;
};

// ── 实现投影：契约暂未覆盖的列表/详情浅投影 ──────────────────────────────────
// 对应契约目标类型见注释；后端实现对应 operation 后按注释机械替换，不改页面。

/** 访问策略（PostCreate/DraftCreate 封闭枚举，与契约 AccessSummary.policy 一致）。 */
export type AccessPolicy = 'public' | 'logged_in' | 'after_reply' | 'level' | 'paid';

/** 内容访问摘要（契约 AccessSummary：policy/unlocked/required_level?）。 */
export interface AccessSummary {
  policy: AccessPolicy;
  unlocked: boolean;
  required_level?: number;
}

/** 帖子/评论作者投影：契约 Author + id（后端列表/详情投影含 id；字段可缺省以
 *  容忍不同接口的投影宽度，见 openapi Author schema）。 */
export interface PostAuthor {
  id: string;
  username?: string | null;
  display_name?: string | null;
  level?: number;
  profile_url?: string;
}

/** 帖子列表行投影（GET /posts、GET /boards/{slug}/posts、GET /search）；
 *  契约目标：Post。公开列表投影不含正文（body_html 仅详情接口按可见性返回）。 */
export interface PostSummary {
  id: string;
  board_id?: string;
  board_slug?: string | null;
  board_name?: string | null;
  post_type?: 'article' | 'discussion';
  title: string;
  /** 列表行作者投影（GET /posts、GET /boards/{slug}/posts）。 */
  author?: { id: string; username?: string | null } | null;
  /** 搜索接口的平面投影（GET /search）。 */
  author_id?: string;
  author_name?: string | null;
  status?: string;
  reply_count: number;
  view_count: number;
  pinned?: boolean;
  pinned_at?: number | null;
  created_at: number;
  updated_at?: number;
  last_reply_at: number | null;
}

/** 帖子详情投影（GET /posts/{id}）；契约目标：Post + body_html。
 *
 * 可见性契约（M04-VISIBILITY）：未授权请求方 `body_html` 字段**缺失**
 * （undefined，而非 null）；access_summary.unlocked=true 才含正文。 */
export interface PostDetail {
  id: string;
  post_type?: 'article' | 'discussion';
  title: string;
  status?: string;
  author?: PostAuthor | null;
  /** 公开字段白名单（posts/[id]/+page.server.ts）逐项挑选。 */
  access_summary?: AccessSummary;
  capabilities?: string[];
  reply_count?: number;
  view_count?: number;
  created_at: number;
  updated_at: number;
  /** 锁帖时间（M04-POSTS-09 治理：closed_at 置位即锁帖，M04-UI-06 锁定横幅）。 */
  closed_at?: number | null;
  /** 未授权时缺失（undefined）；公开/已解锁时后端渲染的清洗 HTML。 */
  body_html?: string | null;
}

/** 评论投影（GET /posts/{id}/comments）；契约目标：Comment + body_html +
 *  floor/parent_id/post_id（后端投影扩展字段）。 */
export interface Comment {
  id: string;
  post_id: string;
  author?: PostAuthor | null;
  parent_id: string | null;
  floor: number;
  status: string;
  /** 已解锁才返回；未授权/受限时缺失。 */
  body_html?: string | null;
  /** 作者限时编辑（M04-COMMENTS-05）所需的原文 Markdown（契约扩展字段，
   *  仅作者/有权限请求方返回；缺失时编辑表单提示重输完整内容）。 */
  markdown?: string | null;
  version: number;
  created_at: number;
  updated_at: number;
}

/** 草稿（GET/POST /drafts）；契约目标：Draft（ResourceMeta + 内容字段）。
 *  时间戳按后端 M01-DB-08 统一为 Unix 毫秒。 */
export interface Draft {
  id: string;
  type: 'article' | 'discussion';
  title: string;
  markdown: string;
  board_id: string | null;
  visibility_level: number;
  access_policy: string;
  scheduled_at?: number | null;
  version: number;
  created_at: number;
  updated_at: number;
  /** M08-INDEX-03：逐帖退出搜索引擎公开索引（后端返回时使用）。 */
  search_index_opt_out?: boolean;
  /** M08-INDEX-03：逐帖退出 AI 摘要生成。 */
  ai_summary_opt_out?: boolean;
}

// ── 写请求输入（POST/PATCH body，契约对应 schema）────────────────────────────

/** POST /api/v1/posts body（契约 PostCreate）。scheduled_at 按后端实现为
 *  Unix 毫秒（契约 date-time 字符串与实现偏差见 M04-UI-04 报告）。 */
export interface PostCreateInput {
  type: 'article' | 'discussion';
  title: string;
  markdown: string;
  board_id: string;
  visibility_level: number;
  access_policy: AccessPolicy;
  scheduled_at?: number | null;
  client_request_id: string;
  /** M08-INDEX-03：作者逐帖退出公开搜索引擎索引（管理员全站/板块策略优先）。 */
  search_index_opt_out?: boolean;
  /** M08-INDEX-03：作者逐帖退出 AI 摘要生成（管理员策略优先）。 */
  ai_summary_opt_out?: boolean;
}

/** POST /api/v1/posts/{postId}/comments body（契约 CommentCreate）。 */
export interface CommentCreateInput {
  markdown: string;
  parent_id?: string | null;
  client_request_id: string;
}

/** PATCH /api/v1/comments/{id} body（评论编辑，M04-COMMENTS-05）。 */
export interface CommentPatchInput {
  markdown: string;
}

/** POST /api/v1/drafts body（契约 DraftCreate）。 */
export interface DraftCreateInput {
  type: 'article' | 'discussion';
  title: string;
  markdown: string;
  board_id?: string | null;
  visibility_level: number;
  access_policy: AccessPolicy;
  scheduled_at?: number | null;
  client_request_id: string;
  /** M08-INDEX-03：逐帖退出搜索引擎公开索引。 */
  search_index_opt_out?: boolean;
  /** M08-INDEX-03：逐帖退出 AI 摘要生成。 */
  ai_summary_opt_out?: boolean;
}

/** PATCH /api/v1/drafts/{id} body（契约 DraftPatch，部分更新）。 */
export interface DraftPatchInput {
  title?: string;
  markdown?: string;
  board_id?: string | null;
  visibility_level?: number;
  access_policy?: AccessPolicy;
  scheduled_at?: number | null;
  /** M08-INDEX-03：逐帖退出搜索引擎公开索引。 */
  search_index_opt_out?: boolean;
  /** M08-INDEX-03：逐帖退出 AI 摘要生成。 */
  ai_summary_opt_out?: boolean;
}

/** 通用分页投影；契约目标：Page（next_cursor/has_more）+ items。 */
export interface PageResult<T> {
  items: T[];
  next_cursor: string | null;
  has_more: boolean;
}

/** 通知投影（GET /notifications）；契约目标：Notification + read_at。 */
export interface Notification {
  id: string;
  type: string;
  title: string;
  body: string | null;
  link: string | null;
  is_read: boolean;
  created_at: number;
  read_at: number | null;
  /** M05-NOTIFY-06 读取时权限复查：资源隐藏/删除后只显示安全失效状态。 */
  unavailable?: boolean;
  category?: string;
  template_key?: string | null;
}

export interface NotificationListResult extends PageResult<Notification> {
  unread_count: number;
}

/** 类别偏好（GET/PUT /notifications/preferences）。 */
export interface NotificationPreference {
  category: 'activity' | 'moderation' | 'system' | 'security' | 'digest';
  email_enabled: boolean;
  in_app_enabled: boolean;
  push_enabled: boolean;
  updated_at: number;
}

/** 我的举报投影（GET /reports）。 */
export interface ReportItem {
  id: string;
  target_type: string;
  target_id: string;
  reason_code: string;
  status: string;
  created_at: number;
  updated_at: number;
}

export interface ReportListResult extends PageResult<ReportItem> {}

/** 我的申诉投影（GET /appeals，申诉人侧，无内部 note）。 */
export interface OwnAppeal {
  id: string;
  sanction_id: string;
  status: string;
  message: string;
  submitted_at: number;
  decided_at: number | null;
  updated_at: number;
}

export interface AppealListResult extends PageResult<OwnAppeal> {}

/** 管理端案件队列投影（GET /admin/moderation/cases）。 */
export interface ModerationCaseItem {
  id: string;
  title: string;
  status: string;
  priority: string;
  assigned_to: string | null;
  created_at: number;
  updated_at: number;
}

export interface ModerationCaseDetail extends ModerationCaseItem {
  resolved_at: number | null;
  resolution: string | null;
}

/** 管理端申诉详情投影（审核员侧，含内部 note）。 */
export interface ModerationAppealDetail {
  id: string;
  sanction_id: string;
  user_id: string;
  status: string;
  message: string;
  reviewed_by: string | null;
  decided_at: number | null;
  submitted_at: number;
  updated_at: number;
  decisions: Array<{
    id: string;
    reviewer_id: string;
    decision: string;
    decision_note: string | null;
    conflict_of_interest: string | null;
    created_at: number;
  }>;
}

/** 标签投影（GET /tags）；契约未定义独立 Tag schema，保留为领域值对象。 */
export interface Tag {
  id: string;
  slug: string;
  name: string;
  description: string | null;
  color: string | null;
  group_id: string | null;
  usage_count: number;
}

/** 标签分组（GET /tags → groups）。 */
export interface TagGroup {
  id: string;
  name: string;
  slug: string;
  sort_order: number;
}

/** GET /tags 完整响应：items（标签）+ groups（分组）。 */
export interface TagListResult {
  items: Tag[];
  groups: TagGroup[];
}

/** 反应切换结果投影（POST /posts/{id}/reactions）；契约目标：ReactionResult。 */
export interface ReactionResult {
  reaction: string;
  active: boolean;
  count: number;
}

// ── 分页类型兼容别名 ────────────────────────────────────────────────────────
// 契约 Page 与实现 PageResult 同构（next_cursor/has_more），供泛型边界使用。

export type ContractPageShape = ContractPage;

// ═══════════════════════════════════════════════════════════════════════════
// M6/M7：附件、下载、商城、权益、活跃（自建投影类型）
// ═══════════════════════════════════════════════════════════════════════════
//
// 契约（openapi/openapi.yaml）中 M6/M7 写响应用 GenericSuccess 兜底、读响应
// 未定义具体 schema（M06-UI/M07-UI 交付时由并行 backend 域 agent 实现）。本层
// 按 migrations/0048-0050 与 docs/SCHEMA.md §11/§14 定义字段形状；后端收敛后
// 可按契约塌缩为 re-export。所有时间戳统一为 Unix 毫秒（与 M01-DB-08 一致）。

// ── 附件与配额（M06） ──────────────────────────────────────────────────────

/** 附件元数据投影（POST/GET /attachments）。契约目标：Attachment。 */
export interface Attachment {
  id: string;
  owner_id?: string;
  storage_backend?: 'local' | 's3';
  original_name?: string | null;
  media_type: string;
  size_bytes: number;
  sha256?: string;
  width?: number | null;
  height?: number | null;
  status: 'pending' | 'processing' | 'ready' | 'quarantined' | 'deleted';
  /** 计入所有者配额的字节数（含策略计费 variant）。 */
  quota_bytes_charged?: number;
  is_public?: boolean;
  processing_error?: string | null;
  created_at: number;
}

/** 上传创建响应（POST /attachments）：附件 id + 上传方式 + 配额摘要。
 *  - S3 预签名：`upload.url` + `upload.headers`（PUT，短 TTL）；
 *  - 本地直传：`upload.mode === 'local'`，走流式 content 端点。
 * 配额字段来自 API.md §12（max_file_bytes/total_bytes/used_bytes/remaining_bytes）。 */
export interface AttachmentCreateResult {
  id: string;
  status?: Attachment['status'];
  upload?: {
    mode: 'presigned_put' | 'local';
    url?: string | null;
    headers?: Record<string, string>;
    expires_at?: number | null;
  } | null;
  quota?: AttachmentQuota | null;
}

/** 容量摘要（创建响应 / GET /attachments quota）。字段缺失时前端兼容降级。 */
export interface AttachmentQuota {
  /** 当前等级单文件上限。 */
  max_file_bytes: number;
  /** 用户附件总容量。 */
  total_bytes: number;
  /** 已用（计费）字节。 */
  used_bytes: number;
  /** 剩余可上传字节。 */
  remaining_bytes: number;
  /** pending/processing 预留字节（可选）。 */
  reserved_bytes?: number;
  /** 已确认计费字节（可选；与 used_bytes 可能口径不同）。 */
  charged_bytes?: number;
  daily_upload_bytes?: number;
  daily_used_bytes?: number;
  retention_days?: number;
}

/** GET /attachments（本人附件列表 + 配额摘要；后端扩展接口，字段缺失容忍）。 */
export interface AttachmentListResult {
  items: Attachment[];
  quota?: AttachmentQuota | null;
}

// ── 下载授权与抵扣（M06-DOWNLOAD） ─────────────────────────────────────────

/** 下载策略投影（GET /attachments/{id}/download-policy）。 */
export interface DownloadPolicyView {
  mode: 'disabled' | 'free' | 'fixed' | 'inherit' | 'forced_free' | 'forced_paid';
  currency?: string;
  /** 单价（最小单位）。 */
  amount?: number;
  authorization_ttl_seconds?: number;
  /** 当前用户是否持有有效授权（有效期内重签不重复扣费）。 */
  has_active_authorization?: boolean;
  authorization_expires_at?: number | null;
  daily_user_limit?: number | null;
  version?: number;
  is_enabled?: boolean;
}

/** 下载授权投影（GET /download-authorizations/{id}）。 */
export interface DownloadAuthorizationView {
  id: string;
  attachment_id: string;
  status: 'active' | 'expired' | 'revoked';
  charged_amount: number;
  currency_id?: string;
  valid_from: number;
  expires_at: number;
}

/** 我的下载流水（GET /me/download-transactions）。 */
export interface DownloadTransaction {
  id: string;
  attachment_id?: string;
  attachment_name?: string | null;
  /** 实扣金额（免费授权为 0 额度）。 */
  charged?: Money;
  reused_authorization?: boolean;
  created_at: number;
}

// ── 商城（M07-SHOP） ───────────────────────────────────────────────────────

export type ProductKind =
  | 'cosmetic_nickname'
  | 'cosmetic_avatar'
  | 'cosmetic_avatar_attachment'
  | 'cosmetic_badge'
  | 'profile_effect'
  | 'post_effect'
  | 'reaction_pack'
  | 'title_prefix'
  | 'utility';

export type ProductStatus = 'draft' | 'pending_review' | 'published' | 'disabled' | 'retired';

export type RefundPolicy = 'non_refundable' | 'compensation_only' | 'full_refund';

/** 商城商品投影（GET /shop/products、GET /admin/shop/products）。
 *  `presentation_tokens` 只含后端注册白名单 Token，禁止任意 CSS/HTML/URL。 */
export interface ShopProduct {
  id: string;
  kind: ProductKind;
  status: ProductStatus;
  slug: string;
  title: string;
  description_safe?: string | null;
  icon_token?: string | null;
  presentation_tokens?: string[] | null;
  slot?: string | null;
  /** 货币（currency_id 或契约 Money.currency 风格）。 */
  currency: string;
  /** 单价（最小单位，coin 为 1）。 */
  unit_price: number;
  /** 用户限购数量。 */
  quantity_limit: number;
  /** 剩余库存；null = 不限。 */
  stock_remaining?: number | null;
  required_level: number;
  /** 限时商品有效期（秒）；null = 永久。 */
  validity_seconds?: number | null;
  sale_start_at?: number | null;
  sale_end_at?: number | null;
  refund_policy: RefundPolicy;
  version: number;
  created_at: number;
  updated_at: number;
  // ── 请求方视角（后端按需返回，缺失容忍） ──
  /** 当前用户是否可购买（等级/库存/窗口由服务端裁决）。 */
  purchasable?: boolean;
  /** 不可购买时的安全原因（服务端给出中文/稳定码）。 */
  purchase_reason?: string | null;
  /** 当前用户已购数量（用于展示限购剩余）。 */
  user_purchase_count?: number;
}

export type OrderStatus = 'succeeded' | 'refunded' | 'partially_refunded';

/** 订单投影（GET /shop/orders/{id}、GET /admin/shop/orders）。 */
export interface ShopOrder {
  id: string;
  user_id?: string;
  product_id: string;
  product_version: number;
  product_title?: string | null;
  quantity: number;
  currency: string;
  unit_price: number;
  total_amount: number;
  status: OrderStatus;
  /** 权益发放状态；补偿/发放中可缺省或为 pending。 */
  entitlement_status?: 'pending' | 'granted' | 'revoked' | null;
  entitlement_id?: string | null;
  idempotency_key?: string;
  created_at: number;
  updated_at: number;
}

/** POST /shop/orders 响应（契约 GenericSuccess；字段缺失容忍）。 */
export interface OrderCreateResult {
  order: ShopOrder;
  entitlement?: Entitlement | null;
  /** 扣费后余额（可选）。 */
  balance?: Money | null;
}

// ── 权益与展示（M07-SHOP） ─────────────────────────────────────────────────

export type EntitlementStatus = 'owned' | 'equipped' | 'expired' | 'revoked' | 'consumed';

/** 我的权益投影（GET /me/entitlements）。 */
export interface Entitlement {
  id: string;
  product_id: string;
  product_title?: string | null;
  kind?: ProductKind;
  slot?: string | null;
  status: EntitlementStatus;
  quantity: number;
  remaining_quantity: number;
  valid_from: number;
  /** 限时商品到期时间；null = 永久。 */
  expires_at?: number | null;
  equipped_at?: number | null;
  revoked_at?: number | null;
  icon_token?: string | null;
  presentation_tokens?: string[] | null;
  created_at: number;
}

/** 我的展示投影（GET /me/presentation）：服务端编译的安全 Token 集合。
 *  `presentation_tokens` 的 key/value 全部来自后端白名单枚举，前端只渲染
 *  自身 allowlist（见 lib/components/wardrobe/tokens.ts），绝不解释任意样式。 */
export interface Presentation {
  version: number;
  nickname_decoration_id?: string | null;
  nickname_color_id?: string | null;
  avatar_frame_id?: string | null;
  avatar_attachment_id?: string | null;
  profile_effect_id?: string | null;
  title_prefix_id?: string | null;
  post_effect_id?: string | null;
  profile_badge_ids?: string[] | null;
  presentation_tokens?: Record<string, string | string[] | null>;
  updated_at: number;
}

// ── 活跃与等级（M07-LEVELS） ───────────────────────────────────────────────

/** 活动摘要（GET /activity/summary）。字段缺失时前端兼容降级。 */
export interface ActivitySummary {
  level: number;
  level_name?: string | null;
  /** 当前经验余额（experience 货币）。 */
  xp: number;
  /** 距下一级所需经验；null = 已满级。 */
  xp_to_next?: number | null;
  /** 本自然日是否已签到（自动领取）。 */
  checked_in_today: boolean;
  /** 连续签到天数。 */
  streak_days: number;
  /** 今日已入账奖励。 */
  today_earned?: Money[];
  /** 账户余额（可多币种；展示取 coin）。 */
  balances?: Money[];
  /** 今日任务（含签到）。 */
  tasks?: Array<{
    id?: string;
    kind: string;
    title?: string | null;
    reward?: Money;
    /** claimed = 已完成、available = 可领、locked = 未满足。 */
    status?: 'claimed' | 'available' | 'locked';
    progress?: number | null;
    target?: number | null;
  }>;
  updated_at?: number;
}

// ── 管理端：存储/配额/下载计费/商城/活跃（M06-UI/M07-UI） ──────────────────

/** 存储配置脱敏视图（GET /admin/storage/config）。
 *  Secret 只返回 secret_configured 布尔；来源为 env 的字段只读。 */
export interface StorageConfig {
  backend: 'local' | 's3';
  /** 配置来源：env（部署配置只读）或 db（后台可改）。 */
  source: 'env' | 'db';
  local_path?: string | null;
  s3_endpoint?: string | null;
  s3_region?: string | null;
  s3_bucket?: string | null;
  s3_path_style?: boolean;
  s3_presigned_uploads?: boolean;
  s3_public_base_url?: string | null;
  /** S3 签名 URL TTL（秒）；修改只影响新签发 URL。 */
  signed_url_ttl_seconds?: number;
  upload_max_bytes?: number;
  /** 是否已配置 Secret（不返回明文）。 */
  secret_configured: boolean;
  /** 由环境变量/Workload Identity 管理、不可在线修改的字段。 */
  managed_fields?: string[];
  version: number;
  updated_at?: number;
}

/** PATCH /admin/storage/config body：只包含变化字段；secret 空串=保持不变。 */
export interface StorageConfigPatch {
  backend?: 'local' | 's3';
  local_path?: string | null;
  s3_endpoint?: string | null;
  s3_region?: string | null;
  s3_bucket?: string | null;
  s3_path_style?: boolean;
  s3_presigned_uploads?: boolean;
  s3_public_base_url?: string | null;
  signed_url_ttl_seconds?: number;
  upload_max_bytes?: number;
  /** 空串表示保持原值；写操作不接受读取。 */
  s3_secret_access_key?: string;
  s3_access_key_id?: string;
  expected_version: number;
  reason: string;
}

/** POST /admin/storage/test 响应：稳定错误码 + 脱敏诊断，不回显凭证。 */
export interface StorageTestResult {
  ok: boolean;
  message: string;
  code?: string | null;
  elapsed_ms?: number | null;
}

/** 等级附件配额（GET /admin/levels/{id}/attachment-quota）。 */
export interface LevelQuotaView {
  level: number;
  max_file_bytes: number;
  total_bytes: number;
  daily_upload_bytes?: number;
  retention_days?: number;
  policy_version?: number;
  updated_at?: number;
}

/** 下载计费站点配置（GET /admin/download-billing/config）。 */
export interface DownloadBillingConfig {
  mode: 'disabled' | 'free' | 'fixed' | 'inherit';
  currency?: string;
  amount?: number;
  authorization_ttl_seconds: number;
  daily_user_limit?: number | null;
  single_charge_limit?: number | null;
  is_enabled: boolean;
  version: number;
  updated_at?: number;
}

/** 商城站点配置（GET /admin/shop/config）。 */
export interface ShopConfig {
  enabled?: boolean;
  currency_id?: string;
  /** 数字装扮默认退款策略。 */
  default_refund_policy?: RefundPolicy;
  max_quantity_per_order?: number;
  version: number;
  updated_at?: number;
}

/** 活跃配置（GET /admin/activity/config）。 */
export interface ActivityConfig {
  /** 自动签到（每日首次有效页面访问）是否开启。 */
  check_in_enabled: boolean;
  /** 签到奖励（exp/coin）。 */
  check_in_reward?: Money;
  /** 连续签到奖励规则（JSON 简化展示）。 */
  streak_bonus_enabled?: boolean;
  /** 经验货币。 */
  exp_currency?: string;
  version: number;
  updated_at?: number;
}

/** 活跃任务（GET /admin/activity/tasks）。 */
export interface ActivityTask {
  id: string;
  kind: 'check_in' | 'task' | 'reaction' | 'post' | 'comment' | 'leaderboard';
  title?: string | null;
  currency: string;
  amount: number;
  daily_limit?: number | null;
  cooldown_seconds?: number | null;
  is_enabled: boolean;
  version: number;
  updated_at: number;
}

// ═══════════════════════════════════════════════════════════════════════════
// M8/M9：搜索、索引退出与 AI（自建投影类型）
// ═══════════════════════════════════════════════════════════════════════════
//
// 契约（openapi/openapi.yaml）中 M8 搜索结果后端投影可能与 contract SearchPage
// 形状存在差异（flat items + next_cursor/has_more）；AI 各接口目前以
// GenericSuccess/GenericRequest 兜底（M08-UI/M09-UI 交付时由并行 backend 域
// agent 实现具体 schema）。本层按 docs/SEARCH.md、docs/AI.md §5 与
// CRAWLER-POLICY.md 定义字段形状，字段缺失一律容忍；后端收敛后可按契约塌缩
// 为 re-export。所有时间戳统一为 Unix 毫秒（与 M01-DB-08 一致）。

// ── 搜索结果（M08） ──────────────────────────────────────────────────────

/** 搜索结果显示投影（契约 SearchResult + 可选安全高亮 + 平面 post 行展示
 * 字段）。
 *
 * `highlight` 只由后端提供（已清洗、受限长度的纯文本片段）；前端仅按纯文本
 * 插值渲染，绝不据此在客户端拼接/还原隐藏正文（M08-UI-02）。缺省为 null。
 * board/author/count 等平面字段只在后端返回时展示，绝不包含正文/隐藏内容。 */
export type SearchResultView = ContractSearchResult & {
  highlight?: string | null;
  board_slug?: string | null;
  board_name?: string | null;
  author_id?: string | null;
  author_name?: string | null;
  reply_count?: number;
  view_count?: number;
};

/** 搜索分页投影（契约 SearchPage 兼容平面返回形状）。 */
export interface SearchPageView {
  items: SearchResultView[];
  query: string;
  next_cursor: string | null;
  has_more: boolean;
}

// ── AI 能力（M09） ───────────────────────────────────────────────────────

/** AI 能力总开关状态（Feature Flag 门控；未启用默认关闭，见 docs/AI.md）。 */
export type AiFeatureState = 'enabled' | 'disabled';

/** Provider 脱敏状态（GET /api/v1/ai/capabilities）。
 *  Secret 只返回 `secret_configured` 布尔；base_url/model 为允许展示的元数据，
 *  绝不携带密钥/完整 API key。 */
export interface AiProviderStatus {
  id: string;
  name?: string | null;
  /** 是否已配置 Secret（只返回布尔，不回显任何明文/片段）。 */
  secret_configured?: boolean;
  /** 该 Provider 当前是否可用（allowlist/健康/预算裁决）。 */
  available?: boolean;
  /** 允许的用途（formatting/seo/tagging/moderation）。 */
  purposes?: string[];
  model?: string | null;
  /** Provider 留存声明（脱敏展示文案）。 */
  retention?: string | null;
  /** Provider 训练使用声明（脱敏展示文案）。 */
  training?: string | null;
  /** Provider 所在区域（脱敏展示文案）。 */
  region?: string | null;
}

/** 用户对某个 purpose 的同意状态（AI.md §5 同意模型）。 */
export interface AiConsentView {
  provider_id: string;
  provider_name?: string | null;
  /** 同意用途（formatting/seo/tagging/moderation）。 */
  purpose: string;
  /** 数据模式（full_with_consent 时才记录同意）。 */
  data_mode: string;
  /** 展示给用户的披露文案版本（同意版本）。 */
  disclosure_version: number;
  /** 当时展示文案的 hash（服务端保存；前端不回显原文）。 */
  disclosure_hash?: string | null;
  granted_at?: number | null;
  revoked_at?: number | null;
}

/** GET /api/v1/ai/capabilities 投影（字段缺失容忍；未启用返回 409/501 → 前端
 *  以 disabled 状态降级）。 */
export interface AiCapabilities {
  /** 站点 AI 能力总开关（Feature Flag；默认 false）。 */
  enabled: boolean;
  /** 默认数据发送策略：disabled/metadata_only/redacted/full_with_consent。 */
  data_mode?: string | null;
  /** 站点开放的用途列表。 */
  purposes?: string[];
  providers?: AiProviderStatus[];
  /** 是否支持同步返回建议（能力声明 synchronous=true）。 */
  synchronous?: boolean;
  /** 当前用户各 purpose 的同意状态。 */
  consents?: AiConsentView[];
  /** 管理员全站/板块强制关闭（管理员策略优先于作者同意）。 */
  admin_forbidden?: boolean;
}

/** POST/DELETE /api/v1/ai/consent body（契约 AiConsentCreate）。 */
export interface AiConsentInput {
  provider_id: string;
  purpose: string;
  /** 契约约束：仅 full_with_consent 模式记录逐次同意。 */
  data_mode: 'full_with_consent';
  disclosure_version: number;
  disclosure_hash: string;
}

// ── AI 任务（M09-TASKS / M09-UI-03） ─────────────────────────────────────

export type AiTaskType = 'formatting' | 'seo' | 'tagging' | 'moderation';

export type AiTaskStatus =
  | 'queued'
  | 'running'
  | 'retry_wait'
  | 'succeeded'
  | 'cancelled'
  | 'dead';

/** 任务投影（GET /api/v1/ai/tasks/{id}）；AI.md §5 Task 响应 schema union。 */
export interface AiTask {
  id: string;
  task_type: AiTaskType;
  status: AiTaskStatus;
  /** 生成时内容 revision（旧 revision 结果不得覆盖新内容）。 */
  source_revision?: number | null;
  policy_version?: number | null;
  /** 目标（draft_id / post_id；用户只见本人任务）。 */
  target_id?: string | null;
  /** 稳定错误码（脱敏；不回显 Provider 响应原文）。 */
  error_code?: string | null;
  error_message?: string | null;
  /** 完成时挂接的建议 id。 */
  suggestion_id?: string | null;
  poll_url?: string | null;
  cancel_url?: string | null;
  created_at: number;
  started_at?: number | null;
  finished_at?: number | null;
}

/** 生成接口响应（默认 202；synchronous=true 短预算内可 200 并携带 suggestion）。 */
export interface AiTaskAccepted {
  task_id: string;
  status: 'queued';
  poll_url: string;
  cancel_url?: string | null;
  source_revision?: number | null;
  policy_version?: number | null;
  suggestion?: AiSuggestion | null;
}

// ── AI 建议（M09-SUGGESTIONS / M09-UI-04/05） ────────────────────────────

export type AiSuggestionType = 'formatting' | 'seo' | 'tagging' | 'moderation';

export type AiSuggestionStatus =
  | 'pending'
  | 'accepted'
  | 'rejected'
  | 'expired'
  | 'superseded';

/** 建议字段（服务端校验后的纯文本；不包含 HTML/脚本）。 */
export interface AiSuggestionField {
  /** 字段名（title/content/summary/tags/…）。 */
  field: string;
  /** 当前值（服务端投影；缺失表示无当前值）。 */
  current?: string | null;
  /** 建议值（安全纯文本，前端按文本插值渲染）。 */
  proposed: string;
  /** 变更理由/摘要（安全纯文本）。 */
  reason?: string | null;
  /** 是否可单独采纳。 */
  selectable?: boolean;
}

/** Suggestion 投影（GET /api/v1/ai/suggestions/{id}）；按 type 独立版本化
 *  schema。审核 Suggestion（moderation）只对有目标审核权限者可见；作者默认
 *  只见公开审核结果而不见内部风险信号（AI.md §5）。 */
export interface AiSuggestion {
  id: string;
  type: AiSuggestionType;
  status: AiSuggestionStatus;
  /** 生成时的目标 base revision（采纳时 expected_base_version/If-Match）。 */
  base_version: number;
  target_id?: string | null;
  fields: AiSuggestionField[];
  /** 安全纯文本 diff 预览（无 HTML；formatting/seo 使用）。 */
  diff?: string | null;
  created_at: number;
  policy_version?: number | null;
  /** moderation 专用：只含公开合规摘要；内部 Prompt/举报信号由后端隐去，
   *  前端绝不渲染任何内部字段。 */
  moderation?: {
    target_type: string;
    summary?: string | null;
  } | null;
}

/** POST /api/v1/ai/suggestions/{id}/accept body（契约 SuggestionAccept）。 */
export interface AiSuggestionAccept {
  expected_base_version: number;
  selected_fields?: string[];
}

// ── 管理端 AI（M09-UI-06） ───────────────────────────────────────────────

/** 管理端 Provider 配置（GET/PATCH /admin/ai/config）。
 *  Secret 只写不读：GET 只返回 secret_configured 布尔。 */
export interface AiAdminProviderConfig {
  id: string;
  name?: string | null;
  api_type?: string | null;
  base_url?: string | null;
  model?: string | null;
  secret_configured?: boolean;
  available?: boolean;
  purposes?: string[];
  retention?: string | null;
  training?: string | null;
  region?: string | null;
  /** 每个 Provider 的策略版本（If-Match 依据之一）。 */
  version?: number;
}

/** AI 站点配置脱敏视图（GET /admin/ai/config）。 */
export interface AiAdminConfig {
  /** AI 能力总开关（Feature Flag）。 */
  enabled: boolean;
  data_mode?: string | null;
  purposes?: string[];
  providers?: AiAdminProviderConfig[];
  budgets?: {
    per_user_daily_tokens?: number | null;
    per_user_daily_usd?: number | null;
    site_daily_tokens?: number | null;
    site_daily_usd?: number | null;
  } | null;
  /** 功能 Flag（formatting/seo/tagging/moderation）。 */
  flags?: Record<string, boolean>;
  /** 默认拒绝 AI 训练爬虫策略（robots/响应头，见 CRAWLER-POLICY.md）。 */
  ai_crawler_policy?: string | null;
  version: number;
  updated_at?: number | null;
}

/** 管理端任务行（GET /admin/ai/tasks）；不能扩大任务内容可见性。 */
export interface AiAdminTaskRow extends AiTask {
  user_id?: string | null;
  provider?: string | null;
  purpose?: string | null;
}

/** POST /admin/ai/providers/test 响应：稳定错误码 + 脱敏诊断（不回显凭证）。 */
export interface AiProviderTestResult {
  ok: boolean;
  message: string;
  code?: string | null;
  elapsed_ms?: number | null;
}

// ═══════════════════════════════════════════════════════════════════════════
// M10：视频嵌入（自建投影类型）
// ═══════════════════════════════════════════════════════════════════════════
//
// 契约（openapi/openapi.yaml）中视频读写响应以 GenericSuccess/GenericRequest
// 兜底（M10-VIDEO/M10-UI 交付时由并行 backend 域 agent 实现具体 schema）；
// 请求体 schema 已由 generated 提供（VideoResolveRequest/VideoEmbedCreate/
// VideoEmbedPatch，见下方 re-export）。本层按 docs/VIDEO-PLUGIN.md §4/§5
// 定义投影字段形状，字段缺失一律容忍；后端收敛后可按契约塌缩为 re-export。
// 时间戳统一为 Unix 毫秒（与 M01-DB-08 一致）。
//
// Security 边界（VIDEO-PLUGIN.md §3/§4）：
//  - 隐藏/审核中/删除/封禁内容：后端**省略** media_url/official_url/
//    poster_url/source_url 等渲染字段——前端绝不猜测或拼接 URL；
//  - Provider Secret、平台签名播放 URL、Cookie/授权头、HLS 密钥永远不进
//    前端投影（lib/video/projection.ts 白名单挑选 + 组件纯渲染后端字段）。

// ── 视频嵌入视图（阅读/管理端） ─────────────────────────────────────────

/** 视频引用类型（VIDEO-PLUGIN.md §4 `video_embeds.provider`）。 */
export type VideoEmbedProvider = 'direct' | 'hls' | 'xigua';

/** 视频引用状态（VIDEO-PLUGIN.md §4 `video_embeds.status`）。 */
export type VideoEmbedStatus = 'pending' | 'ready' | 'blocked' | 'error' | 'removed';

/** 视频挂载目标（帖子或评论，契约 VideoResolveRequest.target_type）。 */
export type VideoTargetType = 'post' | 'comment';

/** 视频嵌入视图（GET /api/v1/video-embeds/{id} 投影）。
 *
 * Security 边界：
 *  - `media_url`（direct/hls 源）/`official_url`（xigua 官方 iframe）/
 *    `source_url`/`poster_url`/`caption_url` 只来自后端白名单投影——受限
 *    内容后端省略这些字段（undefined），前端一律不自行拼接 URL；
 *  - blocked/removed 时渲染层连后端返回的 URL 都不渲染（M10-UI-04 双保险，
 *    见 lib/video/projection.ts 与 VideoEmbedView.svelte）；
 *  - `title`/`poster_url` 为不可信内容，仅作安全纯文本/受限 img 渲染。 */
export interface VideoEmbedView {
  id: string;
  provider: VideoEmbedProvider;
  status: VideoEmbedStatus;
  media_type?: string | null;
  title?: string | null;
  /** Direct/HLS 媒体源（浏览器直连已验证 HTTPS 来源；ready 且可见时返回）。 */
  media_url?: string | null;
  /** 官方 iframe 嵌入 URL（xigua；ready 且可见时返回）。 */
  official_url?: string | null;
  /** 规范化来源 URL（外链卡片；后端控制返回）。 */
  source_url?: string | null;
  /** 来源白名单内的封面 URL（受限内容省略）。 */
  poster_url?: string | null;
  /** 字幕/说明轨 URL（后端提供时用于 `<track kind="captions">`）。 */
  caption_url?: string | null;
  duration_seconds?: number | null;
  /** 生成/校验此投影时生效的 Provider 策略版本。 */
  policy_version?: number;
  last_checked_at?: number | null;
  version: number;
  created_at: number;
  updated_at: number;
}

/** Provider 脱敏状态（resolve 预览用；Secret 只返回布尔，与 AI Provider
 *  投影同策略）。 */
export interface VideoProviderStatusView {
  provider: string;
  /** Provider 是否启用（管理员策略）。 */
  enabled?: boolean;
  /** Provider 当前是否可用（健康/限流/预算裁决）。 */
  available?: boolean;
}

/** POST /api/v1/video-embeds/resolve 响应投影。
 *
 * 只保留展示/创建所需字段；不可嵌入时（无嵌入权限/限流/下架/Provider 故障）
 * `embeddable=false` + `degraded_reason` 稳定码 → 前端降级为安全外链卡片，
 * 不阻塞发帖（VIDEO-PLUGIN.md §3）。Provider Secret/签名播放 URL 不进投影。 */
export interface VideoResolveResult {
  resolution_id: string;
  provider: VideoEmbedProvider | null;
  media_type?: string | null;
  title?: string | null;
  poster_url?: string | null;
  media_url?: string | null;
  official_url?: string | null;
  source_url?: string | null;
  duration_seconds?: number | null;
  /** 当前 Provider 策略版本（创建 embed 时作为 expected_policy_version）。 */
  policy_version?: number;
  /** 是否可嵌入；false → 只渲染安全外链卡片。 */
  embeddable: boolean;
  /** 不可嵌入的稳定原因码（如 no_embed_permission / provider_unavailable）。 */
  degraded_reason?: string | null;
  provider_status?: VideoProviderStatusView | null;
  checked_at?: number | null;
}

// ── 管理端 Provider 策略（M10-UI-06） ───────────────────────────────────

/** Provider 策略视图（GET /api/v1/admin/video/policies）。
 *  字段形状按 docs/VIDEO-PLUGIN.md §4 `video_provider_policies`。 */
export interface VideoProviderPolicyView {
  provider: VideoEmbedProvider;
  enabled: boolean;
  allowed_hosts: string[];
  embed_hosts: string[];
  allowed_media_types: string[];
  max_duration_seconds?: number | null;
  max_bytes?: number | null;
  max_redirects?: number | null;
  hls_max_depth?: number | null;
  hls_max_segments?: number | null;
  hls_max_bytes?: number | null;
  timeout_ms?: number | null;
  /** 审计依据之一：策略版本（PATCH If-Match）。 */
  policy_version: number;
  updated_at?: number | null;
}

/** GET /api/v1/admin/video/policies 投影（字段缺失容忍）。 */
export interface VideoProviderPoliciesView {
  items: VideoProviderPolicyView[];
  /** 站点视频能力总开关（Feature Flag；后端返回时使用）。 */
  enabled?: boolean;
  version?: number;
}

/** PATCH /api/v1/admin/video/policies/{provider} body（If-Match + reason 审计）。 */
export interface VideoProviderPolicyPatch {
  enabled?: boolean;
  allowed_hosts?: string[];
  embed_hosts?: string[];
  allowed_media_types?: string[];
  max_duration_seconds?: number | null;
  max_bytes?: number | null;
  max_redirects?: number | null;
  hls_max_depth?: number | null;
  hls_max_segments?: number | null;
  hls_max_bytes?: number | null;
  timeout_ms?: number | null;
  expected_version: number;
  /** 必填操作原因（写审计）。 */
  reason: string;
}

/** POST /api/v1/admin/video/policies/test 响应：稳定错误码 + 脱敏诊断
 *  （不回显凭证/内部探测详情）。 */
export interface VideoProviderTestResult {
  ok: boolean;
  message: string;
  code?: string | null;
  elapsed_ms?: number | null;
}