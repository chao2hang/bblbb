// BBLBB API Client — 统一前后端通信

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

export interface Problem {
  type?: string;
  title?: string;
  status?: number;
  code?: string;
  detail?: string;
  instance?: string;
  request_id?: string;
  errors?: unknown;
}

const API_BASE = '/api/v1';

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
  return request(fetchFn, `/posts/${id}`);
}

export async function createPost(
  fetchFn: typeof fetch,
  boardSlug: string,
  title: string,
  content: string,
  visibility?: string
): Promise<{ id: string }> {
  return request(fetchFn, '/posts', {
    method: 'POST',
    body: JSON.stringify({ board_slug: boardSlug, title, content, visibility }),
  });
}

// ─── Comments ─────────────────────────────────────────────────────────────

export async function listComments(
  fetchFn: typeof fetch,
  postId: string
): Promise<PageResult<Comment>> {
  return request(fetchFn, `/posts/${postId}/comments`);
}

export async function createComment(
  fetchFn: typeof fetch,
  postId: string,
  content: string,
  parentId?: string
): Promise<Comment> {
  return request(fetchFn, `/posts/${postId}/comments`, {
    method: 'POST',
    body: JSON.stringify({ content, parent_id: parentId }),
  });
}

// ─── Users ────────────────────────────────────────────────────────────────

export async function getUser(fetchFn: typeof fetch, username: string): Promise<User> {
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

// ─── Tags ────────────────────────────────────────────────────────────────

export interface Tag {
  id: string;
  name: string;
  usage_count: number;
  created_at: number;
}

export async function listTags(fetchFn: typeof fetch): Promise<PageResult<Tag>> {
  return request(fetchFn, '/tags');
}

// ─── Reactions ────────────────────────────────────────────────────────────

export interface ReactionResult {
  reaction: string;
  active: boolean;
  count: number;
}

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
