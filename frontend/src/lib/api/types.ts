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

/** 当前用户（GET /me）投影：契约 Me + 实现扩展字段（status/display_name）。
 *  后端补齐 version/created_at/updated_at 后，可塌缩为 `export type User = Me`。 */
export type User = Omit<ContractMe, 'version' | 'created_at' | 'updated_at'> & {
  status: string;
  display_name: string | null;
};

/** 板块（GET /boards）投影：契约 Board + 实现扩展字段（post_count/is_active）。
 *  后端补齐 version/created_at/updated_at 后，可塌缩为
 *  `export type Board = Omit<Board, 'version' | 'created_at' | 'updated_at'>`。 */
export type Board = Omit<ContractBoard, 'version' | 'created_at' | 'updated_at' | 'description'> & {
  description: string | null;
  post_count: number;
  is_active: boolean;
};

// ── 实现投影：契约暂未覆盖的列表/详情浅投影 ──────────────────────────────────
// 对应契约目标类型见注释；后端实现对应 operation 后按注释机械替换，不改页面。

/** 帖子列表行投影（GET /posts、GET /boards/{slug}/posts）；契约目标：Post。 */
export interface PostSummary {
  id: string;
  title: string;
  author_id: string;
  reply_count: number;
  view_count: number;
  pinned: boolean;
  created_at: number;
  last_reply_at: number | null;
}

/** 帖子详情投影（GET /posts/{id}）；契约目标：Post + body_html。 */
export interface PostDetail {
  id: string;
  board_id: string;
  author_id: string;
  author_name: string | null;
  title: string;
  content: string;
  content_format: string;
  status: string;
  visibility: string;
  reply_count: number;
  view_count: number;
  pinned: boolean;
  created_at: number;
  updated_at: number;
  last_reply_at: number | null;
}

/** 评论投影（GET /posts/{id}/comments）；契约目标：Comment + body_html。 */
export interface Comment {
  id: string;
  post_id: string;
  author_id: string;
  author_name: string | null;
  parent_id: string | null;
  content: string;
  content_format: string;
  floor: number;
  created_at: number;
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
}

export interface NotificationListResult extends PageResult<Notification> {
  unread_count: number;
}

/** 标签投影（GET /tags）；契约未定义独立 Tag schema，保留为领域值对象。 */
export interface Tag {
  id: string;
  name: string;
  usage_count: number;
  created_at: number;
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