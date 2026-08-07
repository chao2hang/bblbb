// BBLBB API Client — 统一前后端通信（Cookie 同源 / CSRF / request_id）
//
// DTO 类型唯一来源见 ./types.ts（M00-FRONTEND-03，契约域 generated 接入点）；
// Problem 统一映射见 ../errors.ts（M00-FRONTEND-04）。本模块只做 re-export，
// 页面可直接从 `$lib/api/client` 或 `$lib/api/types` 引用类型。

import type {
  User,
  Board,
  PostSummary,
  PostDetail,
  Comment,
  PageResult,
  Notification,
  NotificationListResult,
  NotificationPreference,
  Tag,
  ReactionResult,
  PublicProfile,
  Draft,
  PostCreateInput,
  CommentCreateInput,
  CommentPatchInput,
  DraftCreateInput,
  DraftPatchInput,
  ReportItem,
  ReportListResult,
  OwnAppeal,
  AppealListResult,
  ModerationCaseItem,
  ModerationCaseDetail,
  ModerationAppealDetail
} from './types';
import type { Problem } from '../errors';

export type {
  User,
  Board,
  PostSummary,
  PostDetail,
  Comment,
  PageResult,
  Notification,
  NotificationListResult,
  NotificationPreference,
  Tag,
  ReactionResult,
  PublicProfile,
  Draft,
  PostCreateInput,
  CommentCreateInput,
  CommentPatchInput,
  DraftCreateInput,
  DraftPatchInput,
  ReportItem,
  ReportListResult,
  OwnAppeal,
  AppealListResult,
  ModerationCaseItem,
  ModerationCaseDetail,
  ModerationAppealDetail
} from './types';
export type { Problem, ProblemFieldError } from '../errors';

const API_BASE = '/api/v1';

/** 幂等键（client_request_id，契约 16-200 字符）：优先 crypto.randomUUID()，
 *  无则退化生成稳定前缀的随机串（与 Field.svelte 同一退化策略）。 */
export function newClientRequestId(): string {
  if (typeof crypto !== 'undefined' && typeof (crypto as Crypto).randomUUID === 'function') {
    return (crypto as Crypto).randomUUID();
  }
  return `bblbb-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 12)}`;
}

interface CsrfResponse {
  token: string;
}

/** 模块级缓存：首次写请求前从 GET /api/v1/auth/csrf 获取，401/403 时清空重取。 */
let csrfToken: string | null = null;

const CSRF_SAFE_METHODS = new Set(['GET', 'HEAD', 'OPTIONS']);

async function ensureCsrf(fetchFn: typeof fetch): Promise<string> {
  if (csrfToken) return csrfToken;
  const response = await fetchFn(`${API_BASE}/auth/csrf`, {
    credentials: 'same-origin',
    headers: { Accept: 'application/json' },
  });
  if (!response.ok) {
    throw new Error(`Failed to fetch CSRF token (status ${response.status})`);
  }
  const data = (await response.json()) as CsrfResponse;
  csrfToken = data.token;
  return csrfToken;
}

async function request<T>(
  fetchFn: typeof fetch,
  path: string,
  options: RequestInit = {},
  csrfRetried = false
): Promise<T> {
  const method = (options.method ?? 'GET').toUpperCase();
  const needsCsrf = !CSRF_SAFE_METHODS.has(method);

  const { headers: optionHeaders, ...restOptions } = options;
  const response = await fetchFn(`${API_BASE}${path}`, {
    credentials: 'same-origin',
    headers: {
      'Content-Type': 'application/json',
      Accept: 'application/json',
      ...(needsCsrf ? { 'X-CSRF-Token': await ensureCsrf(fetchFn) } : {}),
      ...optionHeaders,
    },
    ...restOptions,
  });

  // CSRF token 失效（401/403）时清空缓存并重试一次
  if (needsCsrf && !csrfRetried && (response.status === 401 || response.status === 403)) {
    csrfToken = null;
    return request<T>(fetchFn, path, options, true);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  if (!response.ok) {
    let problem: Problem;
    try {
      problem = (await response.json()) as Problem;
    } catch {
      problem = { status: response.status, detail: response.statusText };
    }
    // 429 限流：透传 Retry-After（秒），供 errors.ts retryAfterOf / UI 使用
    if (response.status === 429) {
      const retryAfter = response.headers.get('Retry-After');
      const seconds = retryAfter ? Number.parseInt(retryAfter, 10) : Number.NaN;
      if (Number.isFinite(seconds) && seconds >= 0) {
        problem.retry_after = seconds;
      }
    }
    throw problem;
  }

  return (await response.json()) as T;
}

// ─── Auth ─────────────────────────────────────────────────────────────────

export async function register(
  fetchFn: typeof fetch,
  username: string,
  email: string,
  password: string
): Promise<{ ok: boolean }> {
  return request(fetchFn, '/auth/register', {
    method: 'POST',
    body: JSON.stringify({ username, email, password }),
  });
}

export async function login(
  fetchFn: typeof fetch,
  identifier: string,
  password: string
): Promise<User> {
  return request(fetchFn, '/auth/login', {
    method: 'POST',
    body: JSON.stringify({ identifier, password }),
  });
}

export async function logout(fetchFn: typeof fetch): Promise<void> {
  await request(fetchFn, '/auth/session', { method: 'DELETE' });
}

export async function getMe(fetchFn: typeof fetch): Promise<User | null> {
  try {
    return await request<User>(fetchFn, '/me');
  } catch {
    return null;
  }
}

export async function verifyEmail(
  fetchFn: typeof fetch,
  token: string
): Promise<{ ok: boolean }> {
  return request(fetchFn, '/auth/verify-email', {
    method: 'POST',
    body: JSON.stringify({ token }),
  });
}

export async function requestPasswordReset(
  fetchFn: typeof fetch,
  email: string
): Promise<{ ok: boolean }> {
  return request(fetchFn, '/auth/password-reset', {
    method: 'POST',
    body: JSON.stringify({ email }),
  });
}

export async function confirmPasswordReset(
  fetchFn: typeof fetch,
  token: string,
  password: string
): Promise<{ ok: boolean }> {
  return request(fetchFn, '/auth/password-reset/confirm', {
    method: 'POST',
    body: JSON.stringify({ token, password }),
  });
}

// ─── Boards ───────────────────────────────────────────────────────────────

export async function listBoards(fetchFn: typeof fetch): Promise<PageResult<Board>> {
  return request(fetchFn, '/boards');
}

export async function getBoard(fetchFn: typeof fetch, slug: string): Promise<Board> {
  return request(fetchFn, `/boards/${slug}`);
}

export async function listBoardPosts(
  fetchFn: typeof fetch,
  slug: string
): Promise<PageResult<PostSummary>> {
  return request(fetchFn, `/boards/${slug}/posts`);
}

// ─── Posts ────────────────────────────────────────────────────────────────

export async function getPost(fetchFn: typeof fetch, id: string): Promise<PostDetail> {
  return request(fetchFn, `/posts/${encodeURIComponent(id)}`);
}

/** POST /api/v1/posts（契约 PostCreate）：返回 201 帖子的 {id}（详情跳转用）。 */
export async function createPost(
  fetchFn: typeof fetch,
  input: PostCreateInput
): Promise<{ id: string }> {
  return request(fetchFn, '/posts', {
    method: 'POST',
    body: JSON.stringify(input),
  });
}

// ─── Drafts ────────────────────────────────────────────────────────────────

export async function listDrafts(fetchFn: typeof fetch): Promise<PageResult<Draft>> {
  return request(fetchFn, '/drafts');
}

export async function getDraft(fetchFn: typeof fetch, id: string): Promise<Draft> {
  return request(fetchFn, `/drafts/${encodeURIComponent(id)}`);
}

export async function createDraft(
  fetchFn: typeof fetch,
  input: DraftCreateInput
): Promise<Draft> {
  return request(fetchFn, '/drafts', {
    method: 'POST',
    body: JSON.stringify(input),
  });
}

/** PATCH /api/v1/drafts/{id}（契约 DraftPatch）：If-Match 版本守卫（409 version_conflict）。 */
export async function updateDraft(
  fetchFn: typeof fetch,
  id: string,
  input: DraftPatchInput,
  version: number
): Promise<Draft> {
  return request(fetchFn, `/drafts/${encodeURIComponent(id)}`, {
    method: 'PATCH',
    headers: { 'If-Match': String(version) },
    body: JSON.stringify(input),
  });
}

export async function deleteDraft(fetchFn: typeof fetch, id: string): Promise<void> {
  return request(fetchFn, `/drafts/${encodeURIComponent(id)}`, { method: 'DELETE' });
}

// ─── Comments ─────────────────────────────────────────────────────────────

export async function listComments(
  fetchFn: typeof fetch,
  postId: string
): Promise<PageResult<Comment>> {
  // 契约 CommentPage 为 {items, page{next_cursor,has_more}}；实现曾返回平面
  // {items,next_cursor,has_more}，此处归一化以容忍两种形状。
  const data = await request<
    { items?: Comment[] } & Partial<PageResult<Comment>> & { page?: { next_cursor: string | null; has_more: boolean } }
  >(fetchFn, `/posts/${encodeURIComponent(postId)}/comments`);
  return {
    items: data.items ?? [],
    next_cursor: data.next_cursor ?? data.page?.next_cursor ?? null,
    has_more: data.has_more ?? data.page?.has_more ?? false
  };
}

/** POST /api/v1/posts/{postId}/comments（契约 CommentCreate）。 */
export async function createComment(
  fetchFn: typeof fetch,
  postId: string,
  input: CommentCreateInput
): Promise<Comment> {
  return request(fetchFn, `/posts/${encodeURIComponent(postId)}/comments`, {
    method: 'POST',
    body: JSON.stringify(input),
  });
}

/** PATCH /api/v1/comments/{id}：作者限时编辑（M04-COMMENTS-05），If-Match 版本守卫。 */
export async function updateComment(
  fetchFn: typeof fetch,
  id: string,
  input: CommentPatchInput,
  version: number
): Promise<Comment> {
  return request(fetchFn, `/comments/${encodeURIComponent(id)}`, {
    method: 'PATCH',
    headers: { 'If-Match': String(version) },
    body: JSON.stringify(input),
  });
}

/** DELETE /api/v1/comments/{id}：作者删除（M04-COMMENTS-06）。 */
export async function deleteComment(fetchFn: typeof fetch, id: string): Promise<void> {
  return request(fetchFn, `/comments/${encodeURIComponent(id)}`, { method: 'DELETE' });
}

// ─── Users ────────────────────────────────────────────────────────────────

export async function getUser(fetchFn: typeof fetch, username: string): Promise<PublicProfile> {
  return request(fetchFn, `/users/${username}`);
}

// ─── Search ───────────────────────────────────────────────────────────────

export async function search(
  fetchFn: typeof fetch,
  q: string,
  limit?: number
): Promise<PageResult<PostSummary>> {
  const params = new URLSearchParams({ q });
  if (limit) params.set('limit', String(limit));
  return request(fetchFn, `/search?${params}`);
}

// ─── Notifications ────────────────────────────────────────────────────────

export async function listNotifications(
  fetchFn: typeof fetch,
  unreadOnly?: boolean
): Promise<NotificationListResult> {
  const params = new URLSearchParams();
  if (unreadOnly) params.set('unread_only', 'true');
  const query = params.toString();
  return request(fetchFn, `/notifications${query ? `?${query}` : ''}`);
}

export async function markNotificationRead(
  fetchFn: typeof fetch,
  id: string
): Promise<void> {
  await request(fetchFn, `/notifications/${id}/read`, { method: 'POST' });
}

export async function markAllNotificationsRead(
  fetchFn: typeof fetch
): Promise<{ updated: number }> {
  return request(fetchFn, `/notifications/read-all`, { method: 'POST' });
}

export async function getNotificationPreferences(
  fetchFn: typeof fetch
): Promise<{ items: NotificationPreference[] }> {
  return request(fetchFn, `/notifications/preferences`);
}

export async function setNotificationPreference(
  fetchFn: typeof fetch,
  preference: Pick<NotificationPreference, 'category' | 'email_enabled' | 'in_app_enabled' | 'push_enabled'>
): Promise<{ category: string; updated: boolean }> {
  return request(fetchFn, `/notifications/preferences`, {
    method: 'PUT',
    body: JSON.stringify(preference)
  });
}

// ─── 举报（M05-UI） ─────────────────────────────────────────────────────

export async function createReport(
  fetchFn: typeof fetch,
  input: { target_type: string; target_id: string; reason: string; detail?: string | null }
): Promise<{ id: string; status: string }> {
  return request(fetchFn, `/reports`, { method: 'POST', body: JSON.stringify(input) });
}

export async function listOwnReports(
  fetchFn: typeof fetch
): Promise<ReportListResult> {
  return request(fetchFn, `/reports`);
}

export async function withdrawReport(fetchFn: typeof fetch, id: string): Promise<void> {
  await request(fetchFn, `/reports/${id}/withdraw`, { method: 'POST' });
}

// ─── 申诉（M05-UI） ─────────────────────────────────────────────────────

export async function createAppeal(
  fetchFn: typeof fetch,
  input: { sanction_id: string; content: string }
): Promise<OwnAppeal> {
  return request(fetchFn, `/appeals`, { method: 'POST', body: JSON.stringify(input) });
}

export async function listOwnAppeals(
  fetchFn: typeof fetch
): Promise<AppealListResult> {
  return request(fetchFn, `/appeals`);
}

export async function getOwnAppeal(
  fetchFn: typeof fetch,
  id: string
): Promise<OwnAppeal> {
  return request(fetchFn, `/appeals/${encodeURIComponent(id)}`);
}

export async function withdrawAppeal(fetchFn: typeof fetch, id: string): Promise<void> {
  await request(fetchFn, `/appeals/${encodeURIComponent(id)}/withdraw`, { method: 'POST' });
}

// ─── 管理端案件（M05-UI） ───────────────────────────────────────────────

export async function listModerationCases(
  fetchFn: typeof fetch
): Promise<{ items: ModerationCaseItem[] }> {
  return request(fetchFn, `/admin/moderation/cases`);
}

export async function getModerationCase(
  fetchFn: typeof fetch,
  id: string
): Promise<ModerationCaseDetail> {
  return request(fetchFn, `/admin/moderation/cases/${encodeURIComponent(id)}`);
}

export async function updateModerationCase(
  fetchFn: typeof fetch,
  id: string,
  body: { status: string; resolution?: string | null }
): Promise<{ id: string; status: string }> {
  return request(fetchFn, `/admin/moderation/cases/${encodeURIComponent(id)}`, {
    method: 'PATCH',
    body: JSON.stringify(body)
  });
}

export async function assignModerationCase(
  fetchFn: typeof fetch,
  id: string,
  body: { assignee_id: string; note?: string | null }
): Promise<{ id: string; assigned_to: string }> {
  return request(fetchFn, `/admin/moderation/cases/${encodeURIComponent(id)}/assign`, {
    method: 'POST',
    body: JSON.stringify(body)
  });
}

// ─── 管理端申诉（M05-UI） ───────────────────────────────────────────────

export async function listModerationAppeals(
  fetchFn: typeof fetch
): Promise<{ items: ModerationAppealDetail[] }> {
  return request(fetchFn, `/admin/moderation/appeals`);
}

export async function getModerationAppeal(
  fetchFn: typeof fetch,
  id: string
): Promise<ModerationAppealDetail> {
  return request(fetchFn, `/admin/moderation/appeals/${encodeURIComponent(id)}`);
}

export async function decideModerationAppeal(
  fetchFn: typeof fetch,
  id: string,
  body: { decision: string; reason: string; expected_version: number }
): Promise<{ status: string }> {
  return request(fetchFn, `/admin/moderation/appeals/${encodeURIComponent(id)}`, {
    method: 'PATCH',
    body: JSON.stringify(body)
  });
}

// ─── Tags ────────────────────────────────────────────────────────────────

export async function listTags(fetchFn: typeof fetch): Promise<PageResult<Tag>> {
  return request(fetchFn, '/tags');
}

// ─── Reactions ────────────────────────────────────────────────────────────

export async function togglePostReaction(
  fetchFn: typeof fetch,
  postId: string
): Promise<ReactionResult> {
  return request(fetchFn, `/posts/${postId}/reactions`, { method: 'POST' });
}

// ─── Health ───────────────────────────────────────────────────────────────

export async function fetchHealth(
  fetchFn: typeof fetch = fetch
): Promise<{ status: string; version: string }> {
  const response = await fetchFn('/healthz', {
    credentials: 'same-origin',
    headers: { Accept: 'application/json' },
  });
  if (!response.ok) {
    throw new Error(`Health check failed with status ${response.status}`);
  }
  return response.json();
}
