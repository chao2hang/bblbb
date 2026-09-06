// GAP-FIX（管理域·审计日志）：/admin/audit 重写——真实日志列表替换外链占位。
// - load：GET /api/v1/admin/audit-logs（?q= 过滤 + ?after= cursor 分页，
//   created_at DESC keyset）；
// - 401 → /login、403 → forbidden 态（照 admin/users 模式）；只读页无 action。
import { redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { getAuthed } from '$lib/api/server';
import type { AuditLogItem } from '$lib/api/types';

export type AdminAuditState = 'ok' | 'forbidden' | 'not_implemented' | 'error';

export interface AdminAuditPageData {
  state: AdminAuditState;
  items: AuditLogItem[] | null;
  nextCursor: string | null;
  q: string;
  after: string | null;
  error: string | null;
}

export const load: PageServerLoad = async ({
  cookies,
  request,
  url
}): Promise<AdminAuditPageData> => {
  const requestId = request.headers.get('x-request-id');
  const q = (url.searchParams.get('q') ?? '').trim();
  const after = url.searchParams.get('after');

  const params = new URLSearchParams({ limit: '50' });
  if (q) params.set('q', q);
  if (after) params.set('after', after);

  const result = await getAuthed<{ items: AuditLogItem[]; next_cursor: string | null }>(
    cookies,
    `/api/v1/admin/audit-logs?${params.toString()}`,
    requestId
  );
  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    if (result.status === 403) {
      return { state: 'forbidden', items: null, nextCursor: null, q, after, error: result.message };
    }
    if (result.status === 501) {
      return { state: 'not_implemented', items: null, nextCursor: null, q, after, error: result.message };
    }
    return { state: 'error', items: null, nextCursor: null, q, after, error: result.message };
  }
  return {
    state: 'ok',
    items: result.data.items,
    nextCursor: result.data.next_cursor,
    q,
    after,
    error: null
  };
};
