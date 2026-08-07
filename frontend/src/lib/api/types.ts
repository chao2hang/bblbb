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
}

/** PATCH /api/v1/drafts/{id} body（契约 DraftPatch，部分更新）。 */
export interface DraftPatchInput {
  title?: string;
  markdown?: string;
  board_id?: string | null;
  visibility_level?: number;
  access_policy?: AccessPolicy;
  scheduled_at?: number | null;
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