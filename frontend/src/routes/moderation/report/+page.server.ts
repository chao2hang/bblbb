// M05-UI-01/05/09：举报页——原因/详情/成功统一状态/撤回入口。
// 服务端 form action（无 JS 退化）+ 后端裁决：跨板块/自身等非法目标
// 由 API 拒绝并呈现稳定错误（M05-UI-05）。
import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPost, getAuthed } from '$lib/api/server';
import type { ReportItem } from '$lib/api/types';

export interface ReportPageData {
  items: ReportItem[];
  message?: string | null;
  requestId?: string | null;
  submitted?: { id: string; status: string } | null;
}

export const load: PageServerLoad = async ({ cookies, request }): Promise<ReportPageData> => {
  const requestId = request.headers.get('x-request-id');
  const me = await getAuthed<unknown>(cookies, '/api/v1/me', requestId);
  if (!me.ok && me.status === 401) throw redirect(303, '/login');
  const result = await getAuthed<{ items: ReportItem[] }>(cookies, '/api/v1/reports', requestId);
  const items = result.ok ? result.data.items : [];
  return { items, submitted: null } satisfies ReportPageData;
};

export const actions: Actions = {
  report: async ({ request, cookies }) => {
    const form = await request.formData();
    const targetType = String(form.get('target_type') ?? '').trim();
    const targetId = String(form.get('target_id') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim();
    const detail = String(form.get('detail') ?? '').trim() || null;
    if (!targetType || !targetId || !reason) {
      return fail(422, { items: [], message: '目标类型、目标 ID 与原因均必填' } satisfies ReportPageData);
    }
    try {
      const result = await authedPost<{ id: string; status: string }>(
        cookies,
        '/api/v1/reports',
        { target_type: targetType, target_id: targetId, reason, detail },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { items: [], submitted: { id: result.data.id, status: result.data.status }, message: null } satisfies ReportPageData;
      }
      // M05-UI-05：自身/跨板块/非法目标等被 API 拒绝 → 稳定错误，不猜测原因。
      return fail(result.status, { items: [], message: result.message, requestId: result.requestId, submitted: null } satisfies ReportPageData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { items: [], message: '提交失败，请稍后重试', submitted: null } satisfies ReportPageData);
    }
  },
  withdraw: async ({ request, cookies }) => {
    const form = await request.formData();
    const id = String(form.get('report_id') ?? '').trim();
    if (!id) return fail(422, { items: [], message: '缺少举报 ID' } satisfies ReportPageData);
    try {
      const result = await authedPost<unknown>(
        cookies,
        `/api/v1/reports/${encodeURIComponent(id)}/withdraw`,
        undefined,
        request.headers.get('x-request-id')
      );
      if (result.ok) throw redirect(303, '/moderation/report');
      return fail(result.status, { items: [], message: result.message } satisfies ReportPageData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { items: [], message: '撤回失败，请稍后重试' } satisfies ReportPageData);
    }
  }
};
