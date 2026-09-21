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
  presentation_tokens?: PublicPresentationTokens | null;
  cover_attachment_id?: string | null;
};

/** 公开用户资料（GET /users/{username}）投影：对应后端 PublicProfile DTO
 *  （M03-PROFILE-01），严格公开字段（不含邮箱/状态/Session/IP）。 */
export interface PublicPresentationTokens {
  nickname_color?: string;
  /** 自定义昵称样式（M07-SHOP-UI-10）：样式库名称与结构化样式参数，
   *  服务端校验后投影；仅样式库定义存在（注册枚举色无此字段）。 */
  nickname_color_name?: string;
  nickname_color_style?: CosmeticDefStyle;
  avatar_frame?: string;
  /** 自定义头像框样式（M07-SHOP-UI-10）：样式库名称与结构化参数。 */
  avatar_frame_name?: string;
  avatar_frame_style?: CosmeticDefStyle;
  avatar_frame_attachment_id?: string;
  /** 直接图片 URL 边框（内置 / Steam 实时预览） */
  avatar_frame_url?: string;
  avatar_attachment?: string;
  profile_effect?: string;
  profile_effect_name?: string;
  profile_effect_style?: CosmeticDefStyle;
  post_effect?: string;
  post_effect_name?: string;
  post_effect_style?: CosmeticDefStyle;
  profile_badges?: string[];
  profile_badge_names?: string[];
  profile_badge_styles?: CosmeticDefStyle[];
  title_prefix?: string;
  title_prefix_name?: string;
  title_prefix_style?: CosmeticDefStyle;
}

/** 装扮样式库定义的结构化样式参数（M07-SHOP-UI-10）：字段由服务端 schema
 *  校验（颜色 #rrggbb、动画枚举、时长钳制），前端只渲染白名单字段。 */
export interface CosmeticDefStyle {
  /** nickname_color: solid | gradient | glow；avatar_frame: ring。 */
  mode?: string;
  color?: string;
  /** 渐变 stops（2..=5）。 */
  colors?: string[];
  /** 动画枚举（扩展更多动效：flow | breathe | pulse | wave | shimmer | glitch | rainbow | flicker | fire | bounce | spin | ripple | float | aurora | scanline 等）。 */
  animate?: string;
  durationMs?: number;
  widthPx?: number;
  glowPx?: number;
  shape?: 'circle' | 'rounded' | 'square';
  icon?: string;
  texture?: string;
  baseColor?: string;
  accentColor?: string;
  notification?: 'none' | 'author';
  dailyLimit?: number;
  action?: string;
  quantity?: number;
  cooldownSec?: number;
  /** 边框线型：solid | dashed | dotted | double | groove */
  borderStyle?: 'solid' | 'dashed' | 'dotted' | 'double' | 'groove';
  /** 头像框贴合缩放百分比 (80..=200，默认 125 适配 Steam APNG) */
  frameScale?: number;
  /** Steam / 外部图片资源 URL 或文件名 */
  url?: string;
  image?: string;
  webm?: string;
  mp4?: string;
  /** 光晕扩散大小（px） */
  glowSpread?: number;
  /** 文字阴影/发光大小（px） */
  shadowPx?: number;
  /** 字间距（px） */
  letterSpacing?: number;
  /** 渐变角度（deg 0..=360） */
  angle?: number;
  /** 纹理不透明度（% 10..=100） */
  opacity?: number;
  /** 开放自定义 CSS 代码定义 */
  css?: string;
  /** 开放自定义 JS 脚本定义（动画/画布 hook） */
  js?: string;
}

/** 管理员自定义装扮样式定义（GET /shop/cosmetics 投影）。 */
export interface CosmeticDef {
  id: string;
  kind: string;
  name: string;
  style: CosmeticDefStyle;
  status: string;
  createdAt?: number;
  updatedAt?: number;
}

export type PublicProfile = Omit<ContractPublicUser, 'presentation_tokens'> & {
  display_name: string | null;
  bio: string | null;
  avatar_attachment_id: string | null;
  signature: string | null;
  presentation_tokens?: PublicPresentationTokens | null;
  /** 已装备成就徽章（社交域·成就墙装备槽）：后端恒返回该键（≤3，
   *  服务端裁决）；无装备/封禁降级为空数组。 */
  equipped_achievements: { code: string; name: string }[];
};

/** GET /api/v1/users/suggest 联想提及用户项 */
export interface MentionSuggestionItem {
  username: string;
  display_name: string | null;
  level: number;
}

/** GET /api/v1/users/suggest 响应体 */
export interface MentionSuggestionsResult {
  items: MentionSuggestionItem[];
}

/** 板块（GET /boards）投影：契约 Board（icon 公开投影即返回；parent_id/
 * visibility/posting_mode/post_count 仅已认证请求方可见，M03-BOARDS-08
 * 防匿名计数/面包屑推断）。
 */
export type Board = Omit<
  ContractBoard,
  | 'version'
  | 'created_at'
  | 'updated_at'
  | 'description'
  | 'icon'
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
  /** 板块图标（lucide 图标名；null = 未设置，回退 slug 视觉映射/默认图标）。 */
  icon: string | null;
  parent_id?: string | null;
  visibility?: 'public' | 'members' | 'restricted' | 'hidden' | null;
  posting_mode?: 'normal' | 'approval' | 'readonly' | 'closed' | null;
  /** 管理端投影（GET/PATCH /admin/boards）返回；展示顺序按其升序。 */
  sort_order?: number;
  /** 管理端投影：板块版主用户名列表（board_role_assignments 聚合）。 */
  moderators?: string[];
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
  avatar_attachment_id?: string | null;
  presentation_tokens?: PublicPresentationTokens | null;
}

/** 帖子列表行投影（GET /posts、GET /boards/{slug}/posts、GET /search）；
 *  契约目标：Post。公开列表投影不含正文（body_html 仅详情接口按可见性返回）。 */
export interface PostSummary {
  id: string;
  board_id?: string | null;
  board_slug?: string | null;
  board_name?: string | null;
  post_type?: 'article' | 'discussion';
  title: string;
  /** 列表行作者投影（GET /posts、GET /boards/{slug}/posts）。
   *  display_name 为作者昵称（缺省回退 username；列表 chip 优先显示昵称）。
   *  presentation_tokens 为服务端编译的公开装扮投影（列表「参与者」列楼主
   *  头像渲染已装备头像框；未携带/无装配时缺省）。
   *  avatar_attachment_id 为用户上传头像的公开附件引用（列表头像直接渲染
   *  图片；内容经 /attachments/{id} 稳定端点按附件可见性下发）。 */
  author?: {
    id: string;
    username?: string | null;
    display_name?: string | null;
    avatar_attachment_id?: string | null;
    presentation_tokens?: PublicPresentationTokens | null;
  } | null;
  /** 搜索接口的平面投影（GET /search）。 */
  author_id?: string;
  author_name?: string | null;
  /** 作者昵称平面投影（与 author.display_name 同源；嵌套缺省时回退）。 */
  author_display_name?: string | null;
  status?: string;
  reply_count: number;
  view_count: number;
  /** 点赞计数（M18-HOME-01，原型帖子卡底部 ♥ 展示）。 */
  like_count?: number;
  pinned?: boolean;
  pinned_at?: number | null;
  /** 精选标记（GET /posts 列表投影；featured_at IS NOT NULL）。 */
  is_featured?: boolean;
  featured_at?: number | null;
  /** 作者手写摘要（≤300 字符；列表卡片展示）。 */
  summary?: string | null;
  /** 参与者预览（GET /posts、GET /boards/{slug}/posts、推荐流）：该帖已发布
   *  回复的不同作者（不含楼主），按首评楼层排序，每帖最多 2 个。
   *  仅公开字段（id/username/display_name/avatar_attachment_id 与服务端
   *  编译的 presentation_tokens），缺省时列表回退为楼主头像。 */
  participants?: Array<{
    id?: string;
    username?: string | null;
    display_name?: string | null;
    avatar_attachment_id?: string | null;
    presentation_tokens?: PublicPresentationTokens | null;
  }> | null;
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
  version?: number;
  /** 锁帖时间（M04-POSTS-09 治理：closed_at 置位即锁帖，M04-UI-06 锁定横幅）。 */
  closed_at?: number | null;
  /** 未授权时缺失（undefined）；公开/已解锁时后端渲染的清洗 HTML。 */
  body_html?: string | null;
  /** 编辑器反显所需的原文 Markdown（契约扩展字段，仅作者/管理员可见）。 */
  markdown?: string | null;
  /** 所在板块 ID。 */
  board_id?: string;
  /** 关联标签。 */
  tags?: string[];
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
  /** 标签列表。 */
  tags?: string[];
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
  /** 标签（slug 或名称，≤8 个、每个 1-32 字符）。写入 post_tags 关联。 */
  tags?: string[];
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
  /** 标签列表。 */
  tags?: string[];
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
  /** 标签列表。 */
  tags?: string[];
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

/** 反应切换结果投影（POST /posts/{id}/reactions）；契约目标：ReactionResult。
 *  后端实际返回完整汇总：操作后该目标的全部 counts/total，
 *  以及单激活切换时被移除的反应名列表 removed。 */
export interface ReactionResult {
  reaction: string;
  active: boolean;
  count: number;
  /** 操作后该目标的全部反应计数（key 为反应名/emoji）。 */
  counts?: Record<string, number>;
  /** 操作后该目标的总表态数。 */
  total?: number;
  /** 单激活切换（一人只激活一个）时被移除的反应名列表。 */
  removed?: string[];
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
  /** 引用数（attachment_links 计数；后端投影携带，缺省视为 0）。 */
  ref_count?: number;
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
  /** 站点上传类型策略放行的媒体类型（管理后台可配置；缺省 = 未提供，按全量白名单处理）。 */
  allowed_media_types?: string[];
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
  /** 管理员上传并通过安全扫描的公开 PNG 资源。 */
  asset_attachment_id?: string | null;
  slot?: string | null;
  /** 货币 ID（后端 shop 实现字段名为 currency_id；UI 创建默认传 'coin'）。 */
  currency_id: string;
  /** 结算货币 code/name（后端 LEFT JOIN currencies 投影；悬空引用缺失容忍）。
   *  展示一律走 currencyLabel()，禁止直接渲染 currency_id（可能为 UUID）。 */
  currency_code?: string | null;
  currency_name?: string | null;
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
  /** 货币 ID（后端 order_json 实现字段名为 currency_id）。 */
  currency_id: string;
  /** 结算货币 code/name（订单详情投影；缺失容忍）。 */
  currency_code?: string | null;
  currency_name?: string | null;
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
  /** 头像框/挂件商品的 PNG 素材附件（ready 公共附件 id；其余商品为 null）。 */
  asset_attachment_id?: string | null;
  presentation_tokens?: string[] | null;
  created_at: number;
}

/** 我的展示投影（GET /me/presentation）：服务端编译的安全 Token 集合。
 *  `presentation_tokens` 的 key/value 全部来自后端白名单枚举，前端只渲染
 *  自身 allowlist（见 lib/components/wardrobe/tokens.ts），绝不解释任意样式。 */
export interface Presentation {
  version: number;
  nickname_color_id?: string | null;
  avatar_frame_id?: string | null;
  profile_effect_id?: string | null;
  post_effect_id?: string | null;
  title_prefix_id?: string | null;
  profile_badge_ids?: string[] | null;
  presentation_tokens?: Record<string, string | string[] | null>;
  updated_at: number;
}

// ── 活跃与等级（M07-LEVELS） ───────────────────────────────────────────────

/** 活动摘要（GET /activity/summary）。只描述签到和 B 币余额；等级由
 * `/me/trust-level` 单独提供，避免把经济活动投影误当作等级来源。 */
export interface ActivitySummary {
  /** 签到功能是否开启。 */
  check_in_enabled?: boolean;
  /** 登录/访问自动签到是否开启。 */
  auto_check_in_enabled?: boolean;
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

/** 取活动摘要的 coin 余额（balances 按 code 投影；缺失容忍 → null）。
 *  后端历史投影无 balances 字段时返回 null，调用方按“—”降级。 */
export function activityCoinBalance(summary: Pick<ActivitySummary, 'balances'> | null | undefined): Money | null {
  const balances = summary?.balances ?? [];
  return balances.find((b) => b.currency === 'coin') ?? null;
}

/** 社区信任等级元数据定义（TL0–TL4 体系） */
export interface TrustLevelMeta {
  level: number;
  code: string;
  name: string;
  summary: string;
  promotion: string;
  perks: string[];
}

/** 全站统一社区信任等级阶梯（TL0–TL4） */
export const LINUXDO_TRUST_LEVELS: TrustLevelMeta[] = [
  {
    level: 0,
    code: 'TL0',
    name: '新用户',
    summary: '注册默认等级；拥有基础浏览与发帖交流权限，受反垃圾与频次保护。',
    promotion: '账号注册成功后默认达到',
    perks: ['浏览公开话题与讨论楼层', '发表主题帖与基础回复', '享有基础附件存储与上传配额']
  },
  {
    level: 1,
    code: 'TL1',
    name: '基本用户',
    summary: '愿意阅读即可达到的正式用户；解除新用户发帖频次限制与编辑时间限制。',
    promotion: '累计进入 5 个话题、阅读 30 楼、阅读时长达 10 分钟',
    perks: ['解除新用户发帖频次限制', '解锁完整个人资料卡与签名展示', '自由参与所有公开板块互动']
  },
  {
    level: 2,
    code: 'TL2',
    name: '成员',
    summary: '持续活跃并积极参与讨论的社区成员；享有更高的附件配额与更长编辑窗口。',
    promotion: '累计访问 15 天、送出赞与收到赞各 ≥1、回复 3 个话题、进入 20 个话题、阅读 100 楼与 1 小时',
    perks: ['附件空间扩容至进阶配额档位', '单文件上传大小上限提升', '享有更长的主题与回复编辑窗口']
  },
  {
    level: 3,
    code: 'TL3',
    name: '活跃用户',
    summary: '社区核心活跃骨干；基于近 100 天滚动窗口考核，享有专属板块访问与社区自治特权。',
    promotion: '100天滚动窗口：访问 50% 天数、回复 10 个话题、阅读 25% 新楼、点赞多样性达标且近 6 个月无禁言',
    perks: ['顶格附件存储与每日上传配额', '解锁 TL3 专属板块与私密讨论', '协助整理社区话题（改名与分类）', '获得专属活跃用户高亮标识']
  },
  {
    level: 4,
    code: 'TL4',
    name: '领导者',
    summary: '社区杰出管理者与精神领袖；由工作人员人工审核授予，深度参与社区规则治理。',
    promotion: '仅由社区工作人员人工审核手动授予',
    perks: ['领导者专属金色尊享徽章', '协助日常管理与社区治理', '置顶/关闭/归档社区话题特权', '永久免除常规频次限制']
  }
];

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
  /** 管理方（`deployment` = 环境变量管理，在线修改只做校验与审计）。 */
  managed_by?: string | null;
  /** 站点上传类型策略（管理后台可配置的类目开关）。 */
  allowed_upload_types?: {
    /** 启用的类目（image/pdf/text/office/av）。 */
    categories: string[];
    /** 策略放行的完整媒体类型列表（能力白名单子集）。 */
    media_types: string[];
  };
  /** 保存（校验）后的说明（如 apply after restart via deployment environment）。 */
  note?: string | null;
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
  /** 实际探测的后端（local/s3）。 */
  backend?: 'local' | 's3' | null;
  /** 脱敏诊断详情（与 message 同源）。 */
  detail?: string | null;
  /** 错误分类：ok/invalid/auth/forbidden/network/rate_limited/upstream/not_found/verification/internal。 */
  error_class?: string | null;
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
  /** 站点基准时区。 */
  site_timezone?: string;
  /** 签到功能总开关。 */
  check_in_enabled: boolean;
  /** 登录/访问自动签到是否开启。 */
  auto_check_in_enabled?: boolean;
  /** 每日新一天起始时间（0..=23）。 */
  day_reset_hour?: number;
  /** 签到奖励（B币）。 */
  check_in_reward?: Money;
  /** 签到奖励币种 */
  check_in_currency?: string;
  /** 每日签到上限。 */
  check_in_daily_limit?: number;
  /** 全站奖励开关。 */
  rewards_enabled?: boolean;
  check_in?: {
    enabled?: boolean;
    auto_enabled?: boolean;
    day_reset_hour?: number;
    amount?: number;
    currency?: string;
    daily_limit?: number;
  };
  /** 连续签到奖励规则（JSON 简化展示）。 */
  streak_bonus_enabled?: boolean;
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
  status?: string | null;
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
// ── M12 第三方 Marketplace 投影（docs/MARKETPLACE.md）──────────────────────

/** 托管 Checkout 确认页视图（GET /api/v1/marketplace/checkout-intents/{id}）。
 *  金额/货币/余额全部来自服务端快照；页面不提交可篡改的价格/用户/余额。 */
export interface MarketplaceCheckoutView {
  intent_id: string;
  interaction_id: string;
  version: number;
  client_id: string;
  merchant_name: string;
  terms_url: string;
  privacy_url: string;
  offer_id: string;
  offer_title: string;
  offer_description?: string | null;
  offer_version: number;
  quantity: number;
  amount: number;
  currency_id: string;
  fee_bps: number;
  fee_refundable: boolean;
  scopes: string[];
  balance: number;
  frozen_balance: number;
  balance_after: number;
  expires_at: number;
  status: string;
  created_at: number;
}

/** 市场应用（管理端投影）。 */
export interface MarketplaceClientView {
  id: string;
  client_id: string;
  owner_user_id: string;
  name: string;
  status: string;
  terms_url: string;
  privacy_url: string;
  webhook_url?: string | null;
  webhook_secret_version: number;
  redirect_uris: string[];
  fee_bps: number;
  version: number;
  approval_history?: unknown[];
  created_at: number;
  updated_at: number;
  scopes?: MarketplaceScopeView[];
  balance?: MarketplaceBalanceView | null;
}

export interface MarketplaceScopeView {
  scope: string;
  status: string;
  limits: Record<string, number>;
  version: number;
  effective_at: number;
  approved_at?: number | null;
}

export interface MarketplaceBalanceView {
  client_id: string;
  currency_id: string;
  available_balance: number;
  pending_balance: number;
  frozen_balance: number;
  total: number;
  status: string;
  version: number;
}

export interface MarketplaceOfferView {
  id: string;
  client_id: string;
  external_offer_id: string;
  title: string;
  description_safe?: string | null;
  currency_id: string;
  unit_amount: number;
  quantity_min: number;
  quantity_max: number;
  stock_policy: string;
  stock_remaining?: number | null;
  status: string;
  fee_bps: number;
  version: number;
  created_at: number;
  updated_at: number;
}

export interface MarketplaceRefundView {
  id: string;
  purchase_id: string;
  client_id: string;
  amount: number;
  status: string;
  reason_code: string;
  reason?: string | null;
  merchant_refund_id: string;
  reversal_operation_id?: string | null;
  refunded_by: string;
  refunded_by_type: string;
  created_at: number;
  processed_at?: number | null;
}

export interface MarketplacePurchaseView {
  id: string;
  intent_id: string;
  client_id: string;
  user_id: string;
  offer_id: string;
  offer_version: number;
  quantity: number;
  amount: number;
  fee_amount: number;
  merchant_net: number;
  currency_id: string;
  status: string;
  refunded_amount: number;
  merchant_order_id: string;
  created_at: number;
  updated_at: number;
  refunds?: MarketplaceRefundView[];
}

export interface MarketplaceDeliveryView {
  id: string;
  event_id: string;
  client_id: string;
  event_type: string;
  payload: Record<string, unknown>;
  status: string;
  attempts: number;
  next_retry_at: number;
  last_status_code?: number | null;
  last_error?: string | null;
  delivered_at?: number | null;
  created_at: number;
}

// ═══════════════════════════════════════════════════════════════════════════
// GAP-FIX：社交域 + 管理域补齐投影（规格见 GAP-FIX-SPEC 一、二节）
// ═══════════════════════════════════════════════════════════════════════════

// ─── 社交域：私信（conversations.rs） ─────────────────────────────────────

/** 会话列表行（GET /conversations；双人会话，other=对方）。 */
export interface ConversationItem {
  id: string;
  other: { username: string; display_name: string | null; level: number };
  /** 最近一条消息（后端 LEFT JOIN：无消息的会话为 null）。 */
  last_message: { body: string; created_at: number; sender_username: string } | null;
  unread_count: number;
  updated_at: number;
}

/** 私信消息（GET/POST /conversations/{id}/messages；(created_at,id) ASC）。 */
export interface ConversationMessage {
  id: string;
  sender_username: string;
  body: string;
  created_at: number;
}

// ─── 社交域：成就（achievements.rs） ──────────────────────────────────────

/** 成就定义（GET /achievements；隐藏成就 description 脱敏为「隐藏成就」）。 */
export interface AchievementDef {
  code: string;
  name: string;
  description: string;
  category: string;
  reward_coin: number;
  is_hidden: boolean;
  sort_order: number;
  /** 成就图标（后台上传，本地磁盘存储不走 S3）；null = 未上传，回退内置图标。 */
  icon_url: string | null;
}

/** 我的成就进度行（GET /me/achievements；未解锁返回 progress/target）。 */
export interface MyAchievementItem {
  code: string;
  unlocked_at: number | null;
  progress: number;
  target: number;
  equipped: boolean;
}

// ─── 社交域：API Key（apikeys.rs） ────────────────────────────────────────

/** API Key 列表行（GET /me/api-keys；revoked_at 非空即已吊销）。 */
export interface ApiKeyItem {
  id: string;
  name: string;
  prefix: string;
  scopes: string[];
  created_at: number;
  last_used_at: number | null;
  revoked_at: number | null;
}

/** 创建 API Key 响应（POST /me/api-keys；key 明文仅此一次返回）。 */
export interface ApiKeyCreated {
  id: string;
  name: string;
  prefix: string;
  scopes: string[];
  created_at: number;
  key: string;
}

// ─── 管理域：仪表盘 / BI / 审计 / 系统设置 ────────────────────────────────

/** 管理仪表盘统计（GET /admin/stats；recent_admin_actions ≤8 条）。 */
export interface AdminStats {
  members: number;
  members_delta_7d: number;
  posts_today: number;
  posts_today_delta: number;
  posts_yesterday: number;
  reports_pending: number;
  active_today: number;
  recent_admin_actions: { action: string; actor_username: string; created_at: number }[];
}

/** 运营趋势桶（GET /admin/stats/trend；start/end 为毫秒，标签前端本地渲染）。 */
export interface AdminStatsTrendBucket {
  start: number;
  end: number;
  /** published 帖子数。 */
  posts: number;
  /** published 评论数。 */
  comments: number;
  /** 桶内 posts+comments 去重作者数。 */
  active_users: number;
  /** 新增举报数。 */
  reports: number;
}

/** 运营趋势（GET /admin/stats/trend?period=day|week|month|year；8 桶）。 */
export interface AdminStatsTrend {
  period: 'day' | 'week' | 'month' | 'year';
  bucket_ms: number;
  buckets: AdminStatsTrendBucket[];
}

/** 管理 BI 指标（GET /admin/bi/metrics?period=）。 */
export interface AdminBiMetrics {
  period: 'day' | 'week' | 'month' | 'year';
  generated_at: number;
  metrics: { key: string; label: string; value: number; target: number; delta_pct: number }[];
}

/** 审计日志行（GET /admin/audit-logs；actor_username 来自 users 左联）。 */
export interface AuditLogItem {
  id: string;
  actor_id: string;
  actor_username: string | null;
  action: string;
  object_type: string;
  object_id: string;
  detail: string | null;
  created_at: number;
}

/** 系统设置（GET/PATCH /admin/settings；version 供 If-Match 乐观并发）。 */
export interface AdminSettingsResult {
  settings: {
    open_registration: boolean;
    email_verification: boolean;
    anonymous_replies: boolean;
    public_rss: boolean;
    maintenance_mode: boolean;
    site_name: string;
    default_lang: string;
    public_source: string;
    api_rate_limit: number;
    /** 站内核心货币名称/单位（code='coin'；例如“金币”、“B币”）。 */
    currency_name?: string;
    smtp_enabled?: boolean;
    smtp_host?: string;
    smtp_port?: number;
    smtp_user?: string;
    smtp_pass?: string;
    smtp_pass_configured?: boolean;
    smtp_from_email?: string;
    smtp_from_name?: string;
    smtp_encryption?: string;
    /** 站点文案（0065；空串 = 前端内置通用文案兜底）。 */
    site_description?: string;
    login_eyebrow?: string;
    login_title?: string;
    login_subtitle?: string;
    register_eyebrow?: string;
    register_title?: string;
    register_subtitle?: string;
    /** 第三方 OAuth 登录配置（0066）。 */
    google_auth_enabled?: boolean;
    google_client_id?: string;
    google_client_secret?: string;
    google_client_secret_configured?: boolean;
    github_auth_enabled?: boolean;
    github_client_id?: string;
    github_client_secret?: string;
    github_client_secret_configured?: boolean;
  };
  version: number;
}

/** 站点公开信息（GET /api/v1/site；匿名可读，0065 全站文案统一）。 */
export interface SitePublicResult {
  site_name: string;
  site_description: string;
  login_eyebrow: string;
  login_title: string;
  login_subtitle: string;
  register_eyebrow: string;
  register_title: string;
  register_subtitle: string;
  maintenance_mode: boolean;
  google_login_enabled?: boolean;
  github_login_enabled?: boolean;
  version: number;
}

// ─── 管理域：帖子 / 积分 / 等级 ────────────────────────────────────────────

/** 管理端帖子行（GET /admin/posts）。 */
export interface AdminPostItem {
  id: string;
  title: string;
  author_username: string;
  board_slug: string;
  board_name: string | null;
  status: string;
  review_status: string;
  is_featured: boolean;
  is_pinned: boolean;
  is_locked: boolean;
  view_count: number;
  created_at: number;
}

/** 管理端帖子动作结果（POST /admin/posts/{id}/action）。 */
export interface AdminPostActionResult {
  id: string;
  status: string;
  review_status: string;
  is_featured: boolean;
  is_pinned: boolean;
  is_locked: boolean;
}

/** 积分流水行（GET /admin/points/ledger；point_transactions + users 联查）。 */
export interface PointsLedgerItem {
  id: string;
  username: string;
  kind: string;
  currency: string;
  amount: number;
  balance_after: number;
  source_type: string;
  memo: string | null;
  created_at: number;
}

/** 本人积分流水行（GET /me/point-transactions；同上但无 username）。 */
export interface PointTransactionItem {
  id: string;
  kind: string;
  currency: string;
  amount: number;
  balance_after: number;
  memo: string | null;
  created_at: number;
}

// 2026-09 等级合并单轨：AdminLevelItem（0062 level_rules 存档投影）随
// GET/PATCH /admin/levels 端点移除一并删除；信任等级规则行见
// routes/admin/levels/+page.server.ts 的 AdminTrustLevelItem。

// ─── 用户侧：处罚 / OAuth 授权 ───────────────────────────────────────────

/** 本人处罚记录行（GET /me/sanctions；moderation_actions 投影）。 */
export interface SanctionItem {
  id: string;
  kind: string;
  reason: string;
  created_at: number;
  expires_at: number | null;
  case_id?: string | null;
}

/** OAuth 授权记录行（GET /me/oauth-grants）。 */
export interface OAuthGrantItem {
  client_id: string;
  client_name: string;
  scopes: string[];
  granted_at: number;
  last_used_at: number | null;
}

// ─── 管理域：附件 / 下载账单 / 通知广播 ──────────────────────────────────

/** 管理端附件行（GET /admin/attachments）。 */
export interface AdminAttachmentItem {
  id: string;
  filename: string;
  uploader_username: string;
  size_bytes: number;
  created_at: number;
}

/** 下载计费流水行（GET /admin/download-billing/transactions）。 */
export interface AdminDownloadTxItem {
  id: string;
  username: string;
  filename: string;
  amount: number;
  created_at: number;
}

/** 通知广播行（GET /admin/notifications/outbox；notification_broadcasts）。 */
export interface BroadcastItem {
  id: string;
  title: string;
  body: string;
  target_count: number;
  target_type?: string;
  sender_username?: string;
  recalled?: boolean;
  recalled_at?: number | null;
  created_at: number;
}

// ─── 信任等级（M20-TRUST，LinuxDo 式 TL0–TL4；见 docs/TRUST-LEVELS.md）───

/** 下一级逐项进度条目。 */
export interface TrustLevelRequirement {
  key: string;
  label: string;
  current: number;
  required: number;
  met: boolean;
}

/** 下一级摘要（manual_only = 仅可手动授予，如 TL4）。 */
export interface TrustLevelNext {
  level: number;
  name: string;
  summary?: string | null;
  manual_only: boolean;
  eligible: boolean;
  requirements: TrustLevelRequirement[];
}

/** GET /me/trust-level 响应：当前等级 + 下一级进度。 */
export interface TrustLevelProgress {
  level: number;
  name: string;
  summary?: string | null;
  updated_at?: number | null;
  /** TL3 降级宽限期截止（Unix 毫秒）；仅 level==3 时返回。 */
  grace_until?: number | null;
  window?: { days: number; since: number } | null;
  next_level?: TrustLevelNext | null;
}

/** POST /me/trust-level/read-time 响应。 */
export interface TrustReadTimeResult {
  credited_seconds: number;
  day_total_seconds: number;
}
