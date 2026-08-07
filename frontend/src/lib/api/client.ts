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
  ModerationAppealDetail,
  // M6/M7：附件、下载、商城、权益、活跃与后台（自建投影）
  ActivityConfig,
  ActivitySummary,
  ActivityTask,
  Attachment,
  AttachmentCreateResult,
  AttachmentListResult,
  AttachmentQuota,
  DownloadAuthorizationView,
  DownloadBillingConfig,
  DownloadPolicyView,
  DownloadTransaction,
  Entitlement,
  LevelQuotaView,
  OrderCreateResult,
  Presentation,
  ProductKind,
  ProductStatus,
  RefundPolicy,
  ShopConfig,
  ShopOrder,
  ShopProduct,
  StorageConfig,
  StorageConfigPatch,
  StorageTestResult,
  // M8/M9：搜索与 AI（自建投影）
  SearchPageView,
  SearchResultView,
  AiCapabilities,
  AiConsentInput,
  AiTask,
  AiTaskAccepted,
  AiSuggestion,
  AiSuggestionAccept,
  AiAdminConfig,
  AiAdminTaskRow,
  AiProviderTestResult,
  // M10：视频嵌入（自建投影 + 契约请求体）
  VideoEmbedProvider,
  VideoEmbedStatus,
  VideoTargetType,
  VideoEmbedView,
  VideoResolveResult,
  VideoProviderPoliciesView,
  VideoProviderPolicyView,
  VideoProviderPolicyPatch,
  VideoProviderTestResult,
  VideoResolveRequest,
  VideoEmbedCreate,
  VideoEmbedPatch
} from './types';
import type { Problem } from '../errors';
import { normalizeSearchPage } from '../search';
import type {
  DownloadResult,
  EntitlementEquip,
  Money,
  ShopOrderCreate
} from './types';

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
  ModerationAppealDetail,
  // M6/M7 自建投影类型
  ActivityConfig,
  ActivitySummary,
  ActivityTask,
  Attachment,
  AttachmentCreateResult,
  AttachmentListResult,
  AttachmentQuota,
  DownloadAuthorizationView,
  DownloadBillingConfig,
  DownloadPolicyView,
  DownloadTransaction,
  Entitlement,
  LevelQuotaView,
  OrderCreateResult,
  Presentation,
  ProductKind,
  ProductStatus,
  RefundPolicy,
  ShopConfig,
  ShopOrder,
  ShopProduct,
  StorageConfig,
  StorageConfigPatch,
  StorageTestResult,
  // M8/M9：搜索与 AI（自建投影）
  SearchPageView,
  SearchResultView,
  AiCapabilities,
  AiConsentInput,
  AiTask,
  AiTaskAccepted,
  AiSuggestion,
  AiSuggestionAccept,
  AiAdminConfig,
  AiAdminTaskRow,
  AiProviderTestResult,
  // M10：视频嵌入（自建投影 + 契约请求体）
  VideoEmbedProvider,
  VideoEmbedStatus,
  VideoTargetType,
  VideoEmbedView,
  VideoResolveResult,
  VideoProviderPoliciesView,
  VideoProviderPolicyView,
  VideoProviderPolicyPatch,
  VideoProviderTestResult,
  VideoResolveRequest,
  VideoEmbedCreate,
  VideoEmbedPatch
} from './types';
export type { Problem, ProblemFieldError } from '../errors';
export type { DownloadResult, EntitlementEquip, Money, ShopOrderCreate } from './types';

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

// ═══════════════════════════════════════════════════════════════════════════
// M6/M7：附件、下载、商城、权益、活跃与后台（自建投影类型见 types.ts）
// ═══════════════════════════════════════════════════════════════════════════

/** 幂等键透传辅助：把 client_request_id 同时作为 Idempotency-Key 头。 */
function idemHeaders(key: string): Record<string, string> {
  return { 'Idempotency-Key': key };
}

// ─── 附件（M06-UPLOAD / M06-QUOTA） ─────────────────────────────────────

/** POST /api/v1/attachments：创建上传（返回 presigned PUT 参数或本地直传
 *  信息 + 配额摘要）。body 见契约 AttachmentCreate。 */
export async function createAttachment(
  fetchFn: typeof fetch,
  input: {
    filename: string;
    size: number;
    declared_media_type: string;
    target_type?: string | null;
    target_id?: string | null;
  }
): Promise<AttachmentCreateResult> {
  return request(fetchFn, '/attachments', {
    method: 'POST',
    body: JSON.stringify(input)
  });
}

/** POST /api/v1/attachments/{id}/complete：完成上传并触发服务端校验（幂等）。 */
export async function completeAttachment(
  fetchFn: typeof fetch,
  id: string,
  clientRequestId: string
): Promise<Attachment> {
  return request(fetchFn, `/attachments/${encodeURIComponent(id)}/complete`, {
    method: 'POST',
    headers: idemHeaders(clientRequestId),
    body: JSON.stringify({ client_request_id: clientRequestId })
  });
}

/** GET /api/v1/attachments/{id}：附件元数据。 */
export async function getAttachment(fetchFn: typeof fetch, id: string): Promise<Attachment> {
  return request(fetchFn, `/attachments/${encodeURIComponent(id)}`);
}

/** GET /api/v1/attachments：本人附件列表 + 配额摘要（后端扩展接口；后端
 *  未实现或 501 时返回空列表，由调用方展示配额缺失降级）。 */
export async function listMyAttachments(fetchFn: typeof fetch): Promise<AttachmentListResult> {
  try {
    return await request<AttachmentListResult>(fetchFn, '/attachments');
  } catch {
    return { items: [], quota: null };
  }
}

/** DELETE /api/v1/attachments/{id}：删除未引用附件（删除仅解除对象；容量在
 *  物理释放后归还）。 */
export async function deleteAttachment(fetchFn: typeof fetch, id: string): Promise<void> {
  await request(fetchFn, `/attachments/${encodeURIComponent(id)}`, {
    method: 'DELETE',
    body: JSON.stringify({})
  });
}

/** 稳定内容端点（预览/下载字节流）：GET /api/v1/attachments/{id}/content。
 *  返回字节流（本地直传）或 302 到短期签名 URL（S3）；前端直接作为
 *  img/链接 src，不把签名 URL 当永久状态。 */
export function attachmentContentUrl(id: string): string {
  return `${API_BASE}/attachments/${encodeURIComponent(id)}/content`;
}

// ─── 下载（M06-DOWNLOAD） ────────────────────────────────────────────────

/** GET /api/v1/attachments/{id}/download-policy：当前下载价格与授权状态。 */
export async function getDownloadPolicy(
  fetchFn: typeof fetch,
  id: string
): Promise<DownloadPolicyView> {
  return request(fetchFn, `/attachments/${encodeURIComponent(id)}/download-policy`);
}

/** POST /api/v1/attachments/{id}/download：鉴权、必要时原子扣费并签发临时
 *  URL。返回 DownloadResult（契约）；`url_expires_at` 为 ISO date-time。 */
export async function downloadAttachment(
  fetchFn: typeof fetch,
  id: string,
  clientRequestId: string,
  options: { target_type?: 'post' | 'comment' | null; target_id?: string | null } = {}
): Promise<DownloadResult> {
  return request(fetchFn, `/attachments/${encodeURIComponent(id)}/download`, {
    method: 'POST',
    headers: idemHeaders(clientRequestId),
    body: JSON.stringify({ client_request_id: clientRequestId, ...options })
  });
}

/** GET /api/v1/download-authorizations/{id}：查询本人下载授权。 */
export async function getDownloadAuthorization(
  fetchFn: typeof fetch,
  id: string
): Promise<DownloadAuthorizationView> {
  return request(fetchFn, `/download-authorizations/${encodeURIComponent(id)}`);
}

/** POST /api/v1/download-authorizations/{id}/sign-url：有效授权重新签发
 *  URL，不重复扣费（M06-UI-04：URL 过期只重签，不删除附件）。 */
export async function signDownloadUrl(
  fetchFn: typeof fetch,
  id: string,
  clientRequestId: string
): Promise<DownloadResult> {
  return request(fetchFn, `/download-authorizations/${encodeURIComponent(id)}/sign-url`, {
    method: 'POST',
    headers: idemHeaders(clientRequestId),
    body: JSON.stringify({ client_request_id: clientRequestId })
  });
}

/** GET /api/v1/me/download-transactions：我的下载流水。 */
export async function listDownloadTransactions(
  fetchFn: typeof fetch
): Promise<DownloadTransaction[]> {
  const data = await request<{ items?: DownloadTransaction[] } | DownloadTransaction[]>(
    fetchFn,
    '/me/download-transactions'
  );
  return Array.isArray(data) ? data : (data.items ?? []);
}

// ─── 商城（M07-SHOP） ─────────────────────────────────────────────────────

/** GET /api/v1/shop/products：在售商品列表。 */
export async function listShopProducts(fetchFn: typeof fetch): Promise<ShopProduct[]> {
  const data = await request<{ items?: ShopProduct[] } | ShopProduct[]>(fetchFn, '/shop/products');
  return Array.isArray(data) ? data : (data.items ?? []);
}

/** GET /api/v1/shop/products/{id}：商品详情。 */
export async function getShopProduct(fetchFn: typeof fetch, id: string): Promise<ShopProduct> {
  return request(fetchFn, `/shop/products/${encodeURIComponent(id)}`);
}

/** POST /api/v1/shop/orders（契约 ShopOrderCreate）：服务端重算价格/库存/
 *  等级/限购，同一幂等键重放不重复扣款。响应见 OrderCreateResult。 */
export async function createShopOrder(
  fetchFn: typeof fetch,
  input: ShopOrderCreate
): Promise<OrderCreateResult> {
  return request(fetchFn, '/shop/orders', {
    method: 'POST',
    headers: idemHeaders(input.client_request_id),
    body: JSON.stringify(input)
  });
}

/** GET /api/v1/shop/orders/{id}：订单结果（含 entitlement 发放状态）。 */
export async function getShopOrder(fetchFn: typeof fetch, id: string): Promise<ShopOrder> {
  return request(fetchFn, `/shop/orders/${encodeURIComponent(id)}`);
}

// ─── 权益与展示（M07-SHOP） ──────────────────────────────────────────────

/** GET /api/v1/me/entitlements：我的权益。 */
export async function listEntitlements(fetchFn: typeof fetch): Promise<Entitlement[]> {
  const data = await request<{ items?: Entitlement[] } | Entitlement[]>(
    fetchFn,
    '/me/entitlements'
  );
  return Array.isArray(data) ? data : (data.items ?? []);
}

/** POST /api/v1/me/entitlements/{id}/equip：装备（expected_presentation_version
 *  乐观并发；409 版本冲突/槽位冲突提示刷新）。 */
export async function equipEntitlement(
  fetchFn: typeof fetch,
  id: string,
  body: EntitlementEquip
): Promise<Presentation> {
  return request(fetchFn, `/me/entitlements/${encodeURIComponent(id)}/equip`, {
    method: 'POST',
    headers: idemHeaders(String(body.expected_presentation_version)),
    body: JSON.stringify(body)
  });
}

/** POST /api/v1/me/entitlements/{id}/unequip。 */
export async function unequipEntitlement(
  fetchFn: typeof fetch,
  id: string,
  body: EntitlementEquip
): Promise<Presentation> {
  return request(fetchFn, `/me/entitlements/${encodeURIComponent(id)}/unequip`, {
    method: 'POST',
    headers: idemHeaders(String(body.expected_presentation_version)),
    body: JSON.stringify(body)
  });
}

/** GET /api/v1/me/presentation：服务端编译的安全 Token 展示投影。 */
export async function getPresentation(fetchFn: typeof fetch): Promise<Presentation> {
  return request(fetchFn, '/me/presentation');
}

// ─── 活跃与等级（M07-LEVELS） ────────────────────────────────────────────

/** GET /api/v1/activity/summary：等级/经验/签到/余额安全投影。 */
export async function getActivitySummary(fetchFn: typeof fetch): Promise<ActivitySummary> {
  return request(fetchFn, '/activity/summary');
}

/** POST /api/v1/activity/visit：每日首次有效业务页面访问自动签到（幂等）；
 *  前端也可显式触发领取。返回 ActivityVisitResult（契约）。 */
export async function recordVisit(
  fetchFn: typeof fetch,
  clientRequestId: string
): Promise<{ checked_in_today: boolean; streak_days: number; today_earned: Money[]; point_operation_id?: string }> {
  return request(fetchFn, '/activity/visit', {
    method: 'POST',
    headers: idemHeaders(clientRequestId),
    body: JSON.stringify({ client_request_id: clientRequestId })
  });
}

// ─── 反应（M07-SHOP-08） ─────────────────────────────────────────────────

/** POST /api/v1/posts/{id}/reactions（body 含 reaction，契约 ReactionCreate）。 */
export async function addPostReaction(
  fetchFn: typeof fetch,
  postId: string,
  reaction: string
): Promise<ReactionResult> {
  return request(fetchFn, `/posts/${postId}/reactions`, {
    method: 'POST',
    body: JSON.stringify({ reaction })
  });
}

/** DELETE /api/v1/posts/{id}/reactions/{reaction}：撤销帖子反应。 */
export async function removePostReaction(
  fetchFn: typeof fetch,
  postId: string,
  reaction: string
): Promise<void> {
  await request(fetchFn, `/posts/${postId}/reactions/${encodeURIComponent(reaction)}`, {
    method: 'DELETE',
    body: JSON.stringify({})
  });
}

/** POST /api/v1/comments/{id}/reactions。 */
export async function addCommentReaction(
  fetchFn: typeof fetch,
  commentId: string,
  reaction: string
): Promise<ReactionResult> {
  return request(fetchFn, `/comments/${commentId}/reactions`, {
    method: 'POST',
    body: JSON.stringify({ reaction })
  });
}

/** DELETE /api/v1/comments/{id}/reactions/{reaction}：撤销评论反应。 */
export async function removeCommentReaction(
  fetchFn: typeof fetch,
  commentId: string,
  reaction: string
): Promise<void> {
  await request(fetchFn, `/comments/${commentId}/reactions/${encodeURIComponent(reaction)}`, {
    method: 'DELETE',
    body: JSON.stringify({})
  });
}

// ─── 管理端：存储（M06-UI-06/07） ────────────────────────────────────────

/** GET /api/v1/admin/storage/config：脱敏配置（Secret 只给布尔）。 */
export async function getStorageConfig(fetchFn: typeof fetch): Promise<StorageConfig> {
  return request(fetchFn, '/admin/storage/config');
}

/** PATCH /api/v1/admin/storage/config：If-Match 版本守卫；空 Secret 保持原值。 */
export async function updateStorageConfig(
  fetchFn: typeof fetch,
  patch: StorageConfigPatch
): Promise<StorageConfig> {
  return request(fetchFn, '/admin/storage/config', {
    method: 'PATCH',
    headers: { 'If-Match': String(patch.expected_version) },
    body: JSON.stringify(patch)
  });
}

/** POST /api/v1/admin/storage/test：测试候选/当前配置（脱敏诊断）。 */
export async function testStorageConfig(
  fetchFn: typeof fetch,
  candidate: Record<string, unknown>,
  clientRequestId: string
): Promise<StorageTestResult> {
  return request(fetchFn, '/admin/storage/test', {
    method: 'POST',
    headers: idemHeaders(clientRequestId),
    body: JSON.stringify(candidate)
  });
}

/** GET /api/v1/admin/levels/{id}/attachment-quota：等级附件配额。 */
export async function getLevelAttachmentQuota(
  fetchFn: typeof fetch,
  levelId: string
): Promise<LevelQuotaView> {
  return request(fetchFn, `/admin/levels/${encodeURIComponent(levelId)}/attachment-quota`);
}

/** PATCH /api/v1/admin/levels/{id}/attachment-quota：修改后立即影响新上传。 */
export async function updateLevelAttachmentQuota(
  fetchFn: typeof fetch,
  levelId: string,
  body: { max_file_bytes: number; total_bytes: number; expected_version: number; reason: string }
): Promise<LevelQuotaView> {
  return request(fetchFn, `/admin/levels/${encodeURIComponent(levelId)}/attachment-quota`, {
    method: 'PATCH',
    headers: { 'If-Match': String(body.expected_version) },
    body: JSON.stringify(body)
  });
}

/** GET /api/v1/admin/download-billing/config：下载计费站点配置。 */
export async function getDownloadBillingConfig(fetchFn: typeof fetch): Promise<DownloadBillingConfig> {
  return request(fetchFn, '/admin/download-billing/config');
}

/** PATCH /api/v1/admin/download-billing/config（If-Match + reason）。 */
export async function updateDownloadBillingConfig(
  fetchFn: typeof fetch,
  body: { expected_version: number; reason: string; changes: Record<string, unknown> }
): Promise<DownloadBillingConfig> {
  return request(fetchFn, '/admin/download-billing/config', {
    method: 'PATCH',
    headers: { 'If-Match': String(body.expected_version) },
    body: JSON.stringify(body)
  });
}

// ─── 管理端：商城（M07-UI-08） ───────────────────────────────────────────

/** GET /api/v1/admin/shop/config。 */
export async function getShopConfig(fetchFn: typeof fetch): Promise<ShopConfig> {
  return request(fetchFn, '/admin/shop/config');
}

/** PATCH /api/v1/admin/shop/config。 */
export async function updateShopConfig(
  fetchFn: typeof fetch,
  body: { expected_version: number; reason: string; changes: Record<string, unknown> }
): Promise<ShopConfig> {
  return request(fetchFn, '/admin/shop/config', {
    method: 'PATCH',
    headers: { 'If-Match': String(body.expected_version) },
    body: JSON.stringify(body)
  });
}

/** GET /api/v1/admin/shop/products：全部商品（含草稿/下架）。 */
export async function listAdminShopProducts(fetchFn: typeof fetch): Promise<ShopProduct[]> {
  const data = await request<{ items?: ShopProduct[] } | ShopProduct[]>(
    fetchFn,
    '/admin/shop/products'
  );
  return Array.isArray(data) ? data : (data.items ?? []);
}

/** POST /api/v1/admin/shop/products：新建商品。 */
export async function createAdminShopProduct(
  fetchFn: typeof fetch,
  body: Record<string, unknown>
): Promise<ShopProduct> {
  return request(fetchFn, '/admin/shop/products', { method: 'POST', body: JSON.stringify(body) });
}

/** PATCH /api/v1/admin/shop/products/{id}：更新商品（If-Match 版本守卫）。 */
export async function updateAdminShopProduct(
  fetchFn: typeof fetch,
  id: string,
  body: Record<string, unknown>,
  version: number
): Promise<ShopProduct> {
  return request(fetchFn, `/admin/shop/products/${encodeURIComponent(id)}`, {
    method: 'PATCH',
    headers: { 'If-Match': String(version) },
    body: JSON.stringify(body)
  });
}

/** POST /api/v1/admin/shop/products/{id}/publish。 */
export async function publishAdminShopProduct(fetchFn: typeof fetch, id: string): Promise<void> {
  await request(fetchFn, `/admin/shop/products/${encodeURIComponent(id)}/publish`, {
    method: 'POST',
    body: JSON.stringify({})
  });
}

/** POST /api/v1/admin/shop/products/{id}/disable。 */
export async function disableAdminShopProduct(fetchFn: typeof fetch, id: string): Promise<void> {
  await request(fetchFn, `/admin/shop/products/${encodeURIComponent(id)}/disable`, {
    method: 'POST',
    body: JSON.stringify({})
  });
}

/** GET /api/v1/admin/shop/orders：订单列表。 */
export async function listAdminShopOrders(fetchFn: typeof fetch): Promise<ShopOrder[]> {
  const data = await request<{ items?: ShopOrder[] } | ShopOrder[]>(
    fetchFn,
    '/admin/shop/orders'
  );
  return Array.isArray(data) ? data : (data.items ?? []);
}

/** POST /api/v1/admin/shop/orders/{id}/refund：补偿退款（reason 必填）。 */
export async function refundAdminShopOrder(
  fetchFn: typeof fetch,
  id: string,
  body: { reason_code: string; reason: string; amount?: Money | null }
): Promise<ShopOrder> {
  return request(fetchFn, `/admin/shop/orders/${encodeURIComponent(id)}/refund`, {
    method: 'POST',
    body: JSON.stringify(body)
  });
}

// ─── 管理端：活跃（M07-UI-08） ───────────────────────────────────────────

/** GET /api/v1/admin/activity/config。 */
export async function getActivityConfig(fetchFn: typeof fetch): Promise<ActivityConfig> {
  return request(fetchFn, '/admin/activity/config');
}

/** PATCH /api/v1/admin/activity/config。 */
export async function updateActivityConfig(
  fetchFn: typeof fetch,
  body: { expected_version: number; reason: string; changes: Record<string, unknown> }
): Promise<ActivityConfig> {
  return request(fetchFn, '/admin/activity/config', {
    method: 'PATCH',
    headers: { 'If-Match': String(body.expected_version) },
    body: JSON.stringify(body)
  });
}

/** GET /api/v1/admin/activity/tasks。 */
export async function listAdminActivityTasks(fetchFn: typeof fetch): Promise<ActivityTask[]> {
  const data = await request<{ items?: ActivityTask[] } | ActivityTask[]>(
    fetchFn,
    '/admin/activity/tasks'
  );
  return Array.isArray(data) ? data : (data.items ?? []);
}

/** POST /api/v1/admin/activity/tasks：新建任务。 */
export async function createAdminActivityTask(
  fetchFn: typeof fetch,
  body: Record<string, unknown>
): Promise<ActivityTask> {
  return request(fetchFn, '/admin/activity/tasks', { method: 'POST', body: JSON.stringify(body) });
}

/** PATCH /api/v1/admin/activity/tasks/{id}：更新任务（If-Match）。 */
export async function updateAdminActivityTask(
  fetchFn: typeof fetch,
  id: string,
  body: Record<string, unknown>,
  version: number
): Promise<ActivityTask> {
  return request(fetchFn, `/admin/activity/tasks/${encodeURIComponent(id)}`, {
    method: 'PATCH',
    headers: { 'If-Match': String(version) },
    body: JSON.stringify(body)
  });
}

/** 商品类型 → 中文名（页面展示）。 */
export function productKindLabel(kind: ProductKind | undefined): string {
  const map: Record<string, string> = {
    cosmetic_nickname: '昵称装扮',
    cosmetic_avatar: '头像框',
    cosmetic_avatar_attachment: '头像挂件',
    cosmetic_badge: '徽章',
    profile_effect: '主页装饰',
    post_effect: '帖子装饰',
    reaction_pack: '反应包',
    title_prefix: '昵称前缀',
    utility: '道具'
  };
  return kind ? (map[kind] ?? kind) : '商品';
}

/** 商品状态 → 中文标签。 */
export function productStatusLabel(status: ProductStatus | undefined): string {
  const map: Record<string, string> = {
    draft: '草稿',
    pending_review: '待审核',
    published: '在售',
    disabled: '已停售',
    retired: '已下架'
  };
  return status ? (map[status] ?? status) : '';
}

// ═══════════════════════════════════════════════════════════════════════════
// M8/M9：搜索与 AI（M08-UI / M09-UI）
// ═══════════════════════════════════════════════════════════════════════════

/** GET /api/v1/search：公开搜索（M08-UI-01）。
 *
 * 归一化契约 SearchPage（{items,page{next_cursor,has_more}}）与后端平面
 * 返回（{items,query,next_cursor,has_more}，backend/src/routes/search.rs 当前
 * 实现）；items 兼容契约 SearchResult（title/url/excerpt）与平面 post 行。
 * 任何受限字段（隐藏正文等）后端绝不返回；归一化只做形状整理，不拼接正文。 */
export async function searchPublic(
  fetchFn: typeof fetch,
  q: string,
  opts: { limit?: number; after?: string | null } = {}
): Promise<SearchPageView> {
  const params = new URLSearchParams({ q });
  if (opts.limit) params.set('limit', String(opts.limit));
  if (opts.after) params.set('after', opts.after);
  const data = await request<unknown>(fetchFn, `/search?${params.toString()}`);
  return normalizeSearchPage(data, q);
}

// ─── AI：能力（M09-UI-01） ────────────────────────────────────────────────

/** GET /api/v1/ai/capabilities：能力声明（未启用返回 409 feature_disabled，
 *  由调用方降级为 disabled 态；失败返回 null 以便 UI 给出关闭/不可用说明）。 */
export async function getAiCapabilities(fetchFn: typeof fetch): Promise<AiCapabilities | null> {
  try {
    return await request<AiCapabilities>(fetchFn, '/ai/capabilities');
  } catch {
    return null;
  }
}

// ─── AI：同意（M09-UI-02/03） ────────────────────────────────────────────

/** POST /api/v1/ai/consent：授予同意（每次正文外发前展示完整披露并确认）。
 *  Idempotency-Key 来自 disclosure 快照，重放不重复记录。 */
export async function grantAiConsent(
  fetchFn: typeof fetch,
  input: AiConsentInput
): Promise<{ ok: boolean }> {
  return request(fetchFn, '/ai/consent', {
    method: 'POST',
    headers: idemHeaders(`ai-consent-${input.provider_id}-${input.purpose}-${input.disclosure_version}`),
    body: JSON.stringify(input)
  });
}

/** DELETE /api/v1/ai/consent：按 purpose 撤回同意（撤回后停止新任务）。 */
export async function revokeAiConsent(
  fetchFn: typeof fetch,
  input: AiConsentInput
): Promise<{ ok: boolean }> {
  return request(fetchFn, '/ai/consent', {
    method: 'DELETE',
    headers: idemHeaders(`ai-revoke-${input.provider_id}-${input.purpose}-${input.disclosure_version}`),
    body: JSON.stringify(input)
  });
}

// ─── AI：任务（M09-UI-03） ────────────────────────────────────────────────

/** GET /api/v1/ai/tasks/{id}：本人任务查询（轮询用）。 */
export async function getAiTask(fetchFn: typeof fetch, id: string): Promise<AiTask> {
  return request(fetchFn, `/ai/tasks/${encodeURIComponent(id)}`);
}

/** POST /api/v1/ai/tasks/{id}/cancel：取消尚未结束任务。 */
export async function cancelAiTask(
  fetchFn: typeof fetch,
  id: string,
  clientRequestId: string
): Promise<{ ok: boolean }> {
  return request(fetchFn, `/ai/tasks/${encodeURIComponent(id)}/cancel`, {
    method: 'POST',
    headers: idemHeaders(clientRequestId),
    body: JSON.stringify({})
  });
}

/** POST /api/v1/ai/drafts/{draft_id}/format：主动格式化（202 任务或 200 同步
 *  建议，统一 AiTaskAccepted）。 */
export async function requestDraftFormat(
  fetchFn: typeof fetch,
  draftId: string,
  clientRequestId: string
): Promise<AiTaskAccepted> {
  return request(fetchFn, `/ai/drafts/${encodeURIComponent(draftId)}/format`, {
    method: 'POST',
    headers: idemHeaders(clientRequestId),
    body: JSON.stringify({})
  });
}

// ─── AI：建议（M09-UI-04/05） ────────────────────────────────────────────

/** GET /api/v1/ai/suggestions/{id}：建议详情（moderation 只对审核人员返回；
 *  内部 Prompt/举报信号由后端隐去）。 */
export async function getAiSuggestion(fetchFn: typeof fetch, id: string): Promise<AiSuggestion> {
  return request(fetchFn, `/ai/suggestions/${encodeURIComponent(id)}`);
}

/** POST /api/v1/ai/suggestions/{id}/accept：字段级采纳（expected_base_version
 *  + If-Match 防覆盖新编辑；409 version_conflict → 提示重载）。 */
export async function acceptAiSuggestion(
  fetchFn: typeof fetch,
  id: string,
  body: AiSuggestionAccept
): Promise<AiSuggestion> {
  return request(fetchFn, `/ai/suggestions/${encodeURIComponent(id)}/accept`, {
    method: 'POST',
    headers: { 'If-Match': String(body.expected_base_version), ...idemHeaders(`ai-accept-${id}`) },
    body: JSON.stringify(body)
  });
}

// ─── AI：管理端（M09-UI-06） ─────────────────────────────────────────────

/** GET /api/v1/admin/ai/config：脱敏配置（Secret 只给布尔）。 */
export async function getAdminAiConfig(fetchFn: typeof fetch): Promise<AiAdminConfig> {
  return request(fetchFn, '/admin/ai/config');
}

/** PATCH /api/v1/admin/ai/config：If-Match 版本守卫 + reason（审计）。 */
export async function updateAdminAiConfig(
  fetchFn: typeof fetch,
  body: { expected_version: number; reason: string; changes: Record<string, unknown> }
): Promise<AiAdminConfig> {
  return request(fetchFn, '/admin/ai/config', {
    method: 'PATCH',
    headers: { 'If-Match': String(body.expected_version) },
    body: JSON.stringify(body)
  });
}

/** POST /api/v1/admin/ai/providers/test：测试 Provider（固定脱敏探针，不接受
 *  用户正文）。 */
export async function testAdminAiProvider(
  fetchFn: typeof fetch,
  candidate: Record<string, unknown>,
  clientRequestId: string
): Promise<AiProviderTestResult> {
  return request(fetchFn, '/admin/ai/providers/test', {
    method: 'POST',
    headers: idemHeaders(clientRequestId),
    body: JSON.stringify(candidate)
  });
}

/** GET /api/v1/admin/ai/tasks：全部任务（不扩大内容可见性）。 */
export async function listAdminAiTasks(fetchFn: typeof fetch): Promise<AiAdminTaskRow[]> {
  const data = await request<{ items?: AiAdminTaskRow[] } | AiAdminTaskRow[]>(
    fetchFn,
    '/admin/ai/tasks'
  );
  return Array.isArray(data) ? data : (data.items ?? []);
}

/** POST /api/v1/admin/ai/tasks/{id}/retry：重试 dead/retry_wait 任务。 */
export async function retryAdminAiTask(
  fetchFn: typeof fetch,
  id: string,
  clientRequestId: string
): Promise<{ ok: boolean }> {
  return request(fetchFn, `/admin/ai/tasks/${encodeURIComponent(id)}/retry`, {
    method: 'POST',
    headers: idemHeaders(clientRequestId),
    body: JSON.stringify({})
  });
}

/** POST /api/v1/admin/ai/tasks/{id}/cancel：取消未结束任务。 */
export async function cancelAdminAiTask(
  fetchFn: typeof fetch,
  id: string,
  clientRequestId: string
): Promise<{ ok: boolean }> {
  return request(fetchFn, `/admin/ai/tasks/${encodeURIComponent(id)}/cancel`, {
    method: 'POST',
    headers: idemHeaders(clientRequestId),
    body: JSON.stringify({})
  });
}

/** AI 任务状态 → 中文标签。 */
export function aiTaskStatusLabel(status: AiTask['status'] | undefined): string {
  const map: Record<AiTask['status'], string> = {
    queued: '排队中',
    running: '处理中',
    retry_wait: '等待重试',
    succeeded: '已完成',
    cancelled: '已取消',
    dead: '失败'
  };
  return status ? (map[status] ?? status) : '';
}

/** AI 用途 → 中文标签。 */
export function aiPurposeLabel(purpose: string | undefined): string {
  const map: Record<string, string> = {
    formatting: '格式化',
    seo: 'SEO 优化',
    tagging: '标签建议',
    moderation: '内容审核'
  };
  return purpose ? (map[purpose] ?? purpose) : '';
}

/** 数据模式 → 中文说明。 */
export function aiDataModeLabel(mode: string | null | undefined): string {
  const map: Record<string, string> = {
    disabled: '不发送任何数据',
    metadata_only: '仅发送元数据',
    redacted: '发送脱敏内容',
    full_with_consent: '征得同意后发送完整内容'
  };
  return mode ? (map[mode] ?? mode) : '不发送任何数据';
}

// ═══════════════════════════════════════════════════════════════════════════
// M10：视频嵌入（M10-UI）
// ═══════════════════════════════════════════════════════════════════════════
//
// 端点要求登录 + CSRF + Idempotency-Key（openapi/openapi.yaml：resolve/
// create/refresh 带 IdempotencyKey 参数；本模块复用 request() 的 CSRF 配对
// 与 idemHeaders 幂等透传）。请求体只提交 resolution_id 与允许字段
// （M10-UI-02），Provider Secret/Key 从不进入浏览器 Bundle 或请求体。

/** 稳定短 hash（FNV-1a 32bit → 8 位 hex）：为 URL 生成确定性幂等键
 *  （契约 Idempotency-Key 16-200 字符；不依赖随机数，重试/重放稳定）。 */
export function stableShortHash(input: string): string {
  let hash = 0x811c9dc5;
  for (let i = 0; i < input.length; i++) {
    hash ^= input.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0).toString(16).padStart(8, '0');
}

/** POST /api/v1/video-embeds/resolve：解析 URL，返回类型与安全元数据
 *  （M10-UI-01）。返回后端投影原始 JSON；调用方经 pickVideoResolve 白名单
 *  挑选后再渲染/存储（Provider Secret 绝不入状态）。 */
export async function resolveVideoEmbed(
  fetchFn: typeof fetch,
  sourceUrl: string,
  targetType: VideoTargetType = 'post'
): Promise<VideoResolveResult> {
  return request(fetchFn, '/video-embeds/resolve', {
    method: 'POST',
    headers: idemHeaders(`video-resolve-${stableShortHash(sourceUrl.trim().toLowerCase())}`),
    body: JSON.stringify({ source_url: sourceUrl, target_type: targetType })
  });
}

/** POST /api/v1/video-embeds：创建结构化视频引用（M10-UI-02）。
 *  body 只含契约 VideoEmbedCreate 允许字段（resolution_id/target_type/
 *  target_id/expected_policy_version）。 */
export async function createVideoEmbed(
  fetchFn: typeof fetch,
  input: VideoEmbedCreate
): Promise<VideoEmbedView> {
  return request(fetchFn, '/video-embeds', {
    method: 'POST',
    headers: idemHeaders(`video-embed-${input.resolution_id}`),
    body: JSON.stringify(input)
  });
}

/** GET /api/v1/video-embeds/{id}：当前请求方可见投影（受限内容后端省略
 *  URL 字段；前端经 pickVideoEmbedView 再次挑选）。 */
export async function getVideoEmbed(fetchFn: typeof fetch, id: string): Promise<VideoEmbedView> {
  return request(fetchFn, `/video-embeds/${encodeURIComponent(id)}`);
}

/** POST /api/v1/video-embeds/{id}/refresh：按当前策略异步重新解析元数据
 *  （返回 202 {task_id,status,poll_url}；失败保留安全外链）。 */
export async function refreshVideoEmbed(
  fetchFn: typeof fetch,
  id: string,
  clientRequestId: string
): Promise<{ task_id: string; status: string; poll_url?: string | null }> {
  return request(fetchFn, `/video-embeds/${encodeURIComponent(id)}/refresh`, {
    method: 'POST',
    headers: idemHeaders(`video-refresh-${id}`),
    body: JSON.stringify({ client_request_id: clientRequestId })
  });
}

/** DELETE /api/v1/video-embeds/{id}：删除未引用视频引用。 */
export async function deleteVideoEmbed(fetchFn: typeof fetch, id: string): Promise<void> {
  await request(fetchFn, `/video-embeds/${encodeURIComponent(id)}`, {
    method: 'DELETE',
    body: JSON.stringify({})
  });
}

// ─── 管理端：Provider 策略（M10-UI-06） ─────────────────────────────────

/** GET /api/v1/admin/video/policies：全部 Provider 策略（脱敏视图；
 *  server load 经 pickVideoPolicies 白名单挑选）。 */
export async function listVideoPolicies(fetchFn: typeof fetch): Promise<VideoProviderPoliciesView> {
  return request(fetchFn, '/admin/video/policies');
}

/** PATCH /api/v1/admin/video/policies/{provider}：If-Match 版本守卫 +
 *  reason（审计）。 */
export async function updateVideoPolicy(
  fetchFn: typeof fetch,
  provider: VideoEmbedProvider,
  patch: VideoProviderPolicyPatch
): Promise<VideoProviderPolicyView> {
  return request(fetchFn, `/admin/video/policies/${encodeURIComponent(provider)}`, {
    method: 'PATCH',
    headers: { 'If-Match': String(patch.expected_version) },
    body: JSON.stringify(patch)
  });
}

/** POST /api/v1/admin/video/policies/test：测试 Provider 候选/当前配置
 *  （脱敏诊断，不回显凭证）。 */
export async function testVideoPolicy(
  fetchFn: typeof fetch,
  candidate: Record<string, unknown>,
  clientRequestId: string
): Promise<VideoProviderTestResult> {
  return request(fetchFn, '/admin/video/policies/test', {
    method: 'POST',
    headers: idemHeaders(clientRequestId),
    body: JSON.stringify(candidate)
  });
}
