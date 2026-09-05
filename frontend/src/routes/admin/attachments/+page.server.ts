// GAP-FIX（管理域·附件管理）：/admin/attachments 重写——附件列表 + 删除
// （替换原存储配置脱敏视图；存储配置仍在 /admin/storage）。
// - load：GET /api/v1/admin/attachments（?q= 文件名/上传者模糊 + ?after= 分页）；
// - delete：DELETE /api/v1/admin/attachments/{id} body {reason}（软删除 +
//   reason 写审计；storage.manage 权限门）。
import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedDeleteBody, getAuthed } from '$lib/api/server';
import type { AdminAttachmentItem } from '$lib/api/types';

export type AdminAttachmentsState = 'ok' | 'forbidden' | 'not_implemented' | 'error';

export interface AdminAttachmentsPageData {
  state: AdminAttachmentsState;
  items: AdminAttachmentItem[] | null;
  nextCursor: string | null;
  q: string;
  after: string | null;
  error: string | null;
}

export interface AdminAttachmentsActionData {
  message?: string;
  requestId?: string | null;
}

export const load: PageServerLoad = async ({
  cookies,
  request,
  url
}): Promise<AdminAttachmentsPageData> => {
  const requestId = request.headers.get('x-request-id');
  const q = (url.searchParams.get('q') ?? '').trim();
  const after = url.searchParams.get('after');

  const params = new URLSearchParams({ limit: '30' });
  if (q) params.set('q', q);
  if (after) params.set('after', after);

  const result = await getAuthed<{ items: AdminAttachmentItem[]; next_cursor: string | null }>(
    cookies,
    `/api/v1/admin/attachments?${params.toString()}`,
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

export const actions: Actions = {
  delete: async ({ request, cookies }) => {
    const form = await request.formData();
    const id = String(form.get('id') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim();
    if (!id) return fail(422, { message: '缺少附件 ID' });
    if (!reason) return fail(422, { message: '删除原因必填（写审计）' });
    try {
      const result = await authedDeleteBody<unknown>(
        cookies,
        `/api/v1/admin/attachments/${encodeURIComponent(id)}`,
        { reason },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { message: `附件 ${id} 已删除（软删除，记录保留）` };
      }
      return fail(result.status, { message: result.message, requestId: result.requestId });
    } catch {
      return fail(503, { message: '删除失败，请稍后重试' });
    }
  }
};
