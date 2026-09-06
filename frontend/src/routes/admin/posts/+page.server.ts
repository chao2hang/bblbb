// GAP-FIX（管理域·帖子管理）：/admin/posts 重写——真实数据页替换外链占位。
// - load：GET /api/v1/admin/posts（?status= 筛选 tab + ?q= 搜索 + ?after= cursor 分页）；
// - 401 → /login、403 → forbidden 态（照 admin/users 模式）；
// - action：POST /api/v1/admin/posts/{id}/action（approve/reject/hide/restore/
//   feature/unfeature/pin/unpin/lock/unlock/delete + reason 写审计）。
import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPost, getAuthed } from '$lib/api/server';
import type { AdminPostItem } from '$lib/api/types';

/** 帖子管理动作白名单（与后端 POST /admin/posts/{id}/action 契约一致）。 */
const POST_ACTIONS = [
  'approve',
  'reject',
  'hide',
  'restore',
  'feature',
  'unfeature',
  'pin',
  'unpin',
  'lock',
  'unlock',
  'delete'
] as const;

export type AdminPostsState = 'ok' | 'forbidden' | 'not_implemented' | 'error';

/** Tab 计数（GET /admin/posts 的全局状态聚合，M17-GAPFIX-07）。 */
export interface AdminPostsCounts {
  all: number;
  pending_review: number;
  published: number;
  featured: number;
  hidden: number;
  deleted: number;
}

export interface AdminPostsPageData {
  state: AdminPostsState;
  items: AdminPostItem[] | null;
  nextCursor: string | null;
  status: string;
  q: string;
  after: string | null;
  error: string | null;
  counts: AdminPostsCounts | null;
}

export interface AdminPostsActionData {
  message?: string;
  requestId?: string | null;
  conflict?: boolean;
}

export const load: PageServerLoad = async ({
  cookies,
  request,
  url
}): Promise<AdminPostsPageData> => {
  const requestId = request.headers.get('x-request-id');
  const status = url.searchParams.get('status') ?? '';
  const q = (url.searchParams.get('q') ?? '').trim();
  const after = url.searchParams.get('after');

  const params = new URLSearchParams({ limit: '30' });
  if (status) params.set('status', status);
  if (q) params.set('q', q);
  if (after) params.set('after', after);

  const result = await getAuthed<{ items: AdminPostItem[]; next_cursor: string | null }>(
    cookies,
    `/api/v1/admin/posts?${params.toString()}`,
    requestId
  );
  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    if (result.status === 403) {
      return { state: 'forbidden', items: null, nextCursor: null, status, q, after, error: result.message, counts: null };
    }
    if (result.status === 501) {
      return { state: 'not_implemented', items: null, nextCursor: null, status, q, after, error: result.message, counts: null };
    }
    return { state: 'error', items: null, nextCursor: null, status, q, after, error: result.message, counts: null };
  }
  return {
    state: 'ok',
    items: result.data.items,
    nextCursor: result.data.next_cursor,
    counts: (result.data as { counts?: AdminPostsCounts }).counts ?? null,
    status,
    q,
    after,
    error: null
  };
};

export const actions: Actions = {
  moderate: async ({ request, cookies }) => {
    const form = await request.formData();
    const id = String(form.get('id') ?? '').trim();
    const action = String(form.get('action') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim();
    if (!id) return fail(422, { message: '缺少帖子 ID' });
    if (!reason) return fail(422, { message: '操作原因必填（写审计）' });
    if (!(POST_ACTIONS as readonly string[]).includes(action)) {
      return fail(422, { message: `无效动作：${action}` });
    }
    try {
      const result = await authedPost<unknown>(
        cookies,
        `/api/v1/admin/posts/${encodeURIComponent(id)}/action`,
        { action, reason },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { message: `帖子 ${id} 已执行 ${action}` };
      }
      if (result.status === 409) {
        return fail(409, { conflict: true, message: `状态冲突：${result.message}（请刷新后重试）` });
      }
      return fail(result.status, { message: result.message, requestId: result.requestId });
    } catch {
      return fail(503, { message: '操作失败，请稍后重试' });
    }
  }
};
