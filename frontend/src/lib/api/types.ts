// BBLBB DTO 类型唯一来源（M00-FRONTEND-03）
//
// 契约域 agent 将生成 `frontend/src/lib/api/generated/`（openapi-typescript 产物，
// 见 docs/FRONTEND.md §1），本文件是接入点：待 generated 稳定并含
// User / Board / PostSummary 等核心接口后，把下方手写接口整体替换为
//
//   export type {
//     User, Board, PostSummary, PostDetail, Comment, PageResult,
//     Notification, NotificationListResult, Tag, ReactionResult
//   } from './generated';
//
// client.ts 与所有页面/组件继续从 `$lib/api/client`（或 `$lib/api/types`）引用，
// 无需再改。当前 generated 尚未落盘，以下类型与 openapi/openapi.yaml 对齐，
// 作为过渡期唯一来源；任何契约变更先反映在此处。

export interface User {
  id: string;
  username: string;
  email: string;
  email_verified: boolean;
  status: string;
  display_name: string | null;
  level: number;
  roles: string[];
}

export interface Board {
  id: string;
  slug: string;
  name: string;
  description: string | null;
  post_count: number;
  is_active: boolean;
}

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

export interface PageResult<T> {
  items: T[];
  next_cursor: string | null;
  has_more: boolean;
}

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

export interface Tag {
  id: string;
  name: string;
  usage_count: number;
  created_at: number;
}

export interface ReactionResult {
  reaction: string;
  active: boolean;
  count: number;
}
