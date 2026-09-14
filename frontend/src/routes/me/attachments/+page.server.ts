// 我的附件页（GAP-FIX 个人域：/me/attachments）——前台查看可用附件空间与
// 已上传附件的唯一入口。
//
// - load：GET /api/v1/attachments（本人附件列表 + 当前等级容量摘要；
//   非冻结契约的扩展端点，见 docs/API.md §12 与
//   scripts/check-route-coverage.rb DOCUMENTED_NON_CONTRACT）。
//   401 → /login；其余错误返回错误态（页面展示降级提示，不 500）。
// - delete action：DELETE /api/v1/attachments/{id}（软删除进入保留期，
//   M06-QUOTA-09；容量在保留期后物理清理时释放，与后端口径一致）。
import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedDelete, getAuthed } from '$lib/api/server';
import type { Attachment, AttachmentQuota } from '$lib/api/types';

/** 页面数据（load 投影；列表与容量可独立缺失）。 */
export interface MeAttachmentsPageData {
  items: Attachment[];
  quota: AttachmentQuota | null;
  error: string | null;
}

/** delete action 返回投影（toast 反馈）。 */
export interface MeAttachmentsActionData {
  message?: string;
  messageKind?: 'success' | 'error';
  requestId?: string | null;
}

export const load: PageServerLoad = async ({ cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<{ items?: Attachment[]; quota?: AttachmentQuota | null }>(
    cookies,
    '/api/v1/attachments',
    requestId
  );
  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    return { items: [], quota: null, error: result.message } satisfies MeAttachmentsPageData;
  }
  const data = result.data;
  return {
    items: Array.isArray(data.items) ? data.items : [],
    quota: data.quota ?? null,
    error: null
  } satisfies MeAttachmentsPageData;
};

export const actions: Actions = {
  /** 删除本人附件（软删除；未引用附件进入保留期后物理清理）。 */
  remove: async ({ request, cookies }) => {
    const form = await request.formData();
    const id = String(form.get('id') ?? '').trim();
    if (!id) {
      return fail(422, { message: '缺少附件标识', messageKind: 'error' } satisfies MeAttachmentsActionData);
    }
    try {
      const result = await authedDelete(
        cookies,
        `/api/v1/attachments/${encodeURIComponent(id)}`,
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return {
          message: '附件已删除，进入保留期（到期后物理清理并释放空间）',
          messageKind: 'success'
        } satisfies MeAttachmentsActionData;
      }
      if (result.status === 401) throw redirect(303, '/login');
      if (result.code === 'step_up_required') {
        return fail(403, {
          message: '此操作需要重新验证身份，请重新登录后重试',
          messageKind: 'error',
          requestId: result.requestId
        } satisfies MeAttachmentsActionData);
      }
      return fail(result.status, {
        message: result.message || '删除失败，请稍后重试',
        messageKind: 'error',
        requestId: result.requestId
      } satisfies MeAttachmentsActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, {
        message: '删除失败，请稍后重试',
        messageKind: 'error'
      } satisfies MeAttachmentsActionData);
    }
  }
};
