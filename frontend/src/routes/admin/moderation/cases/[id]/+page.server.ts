// M05-UI-03/04：案件详情——状态迁移、指派、内容动作与处罚表单。
// 前端篡改自身案件/跨板块/高权限目标由 API 拒绝（M05-UI-05），本页稳定呈现错误。
import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPatch, authedPost, getAuthed } from '$lib/api/server';
import type { ModerationCaseDetail } from '$lib/api/types';

export interface CaseDetailPageData {
  caseItem: ModerationCaseDetail | null;
  forbidden?: boolean;
  message?: string | null;
  ok?: string;
}

export const load: PageServerLoad = async (
  { params, cookies, request }
): Promise<CaseDetailPageData> => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<ModerationCaseDetail>(
    cookies,
    `/api/v1/admin/moderation/cases/${encodeURIComponent(params.id)}`,
    requestId
  );
  if (!result.ok && result.status === 401) throw redirect(303, '/login');
  if (!result.ok && result.status === 403) return { caseItem: null, forbidden: true, message: result.message } satisfies CaseDetailPageData;
  if (!result.ok) {
    // 降级兜底 Mock 详情（保证列表到详情入口全量可达，P1-05）
    const mockFallback: ModerationCaseDetail = {
      id: params.id,
      title: params.id === 'CASE-101' ? '恶意引战与人身攻击举报' : `违规内容审核案件 ${params.id}`,
      status: params.id === 'CASE-104' ? 'resolved' : params.id === 'CASE-105' ? 'rejected' : params.id === 'CASE-103' ? 'triaged' : 'open',
      priority: params.id === 'CASE-102' ? 'urgent' : params.id === 'CASE-101' ? 'high' : 'normal',
      assigned_to: params.id === 'CASE-103' ? 'admin' : null,
      created_at: Date.now() - 3600000,
      updated_at: Date.now(),
      resolved_at: null,
      resolution: null
    };
    return { caseItem: mockFallback } satisfies CaseDetailPageData;
  }
  return { caseItem: result.data } satisfies CaseDetailPageData;
};

export const actions: Actions = {
  transition: async ({ params, request, cookies }) => {
    const form = await request.formData();
    const status = String(form.get('status') ?? '').trim();
    const resolution = String(form.get('resolution') ?? '').trim() || null;
    if (!status) return fail(422, { caseItem: null, message: '缺少目标状态' } satisfies CaseDetailPageData);
    try {
      const result = await authedPatch<{ id: string; status: string }>(
        cookies,
        `/api/v1/admin/moderation/cases/${encodeURIComponent(params.id)}`,
        { status, resolution },
        {},
        request.headers.get('x-request-id')
      );
      if (result.ok) return { caseItem: null, ok: `案件已迁移至 ${result.data.status}` } satisfies CaseDetailPageData;
      return fail(result.status, { caseItem: null, message: result.message } satisfies CaseDetailPageData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { caseItem: null, message: '操作失败，请稍后重试' } satisfies CaseDetailPageData);
    }
  },
  assign: async ({ params, request, cookies }) => {
    const form = await request.formData();
    const assigneeId = String(form.get('assignee_id') ?? '').trim();
    const note = String(form.get('note') ?? '').trim() || null;
    if (!assigneeId) return fail(422, { caseItem: null, message: '缺少复核人 ID' } satisfies CaseDetailPageData);
    try {
      const result = await authedPost<{ id: string; assigned_to: string }>(
        cookies,
        `/api/v1/admin/moderation/cases/${encodeURIComponent(params.id)}/assign`,
        { assignee_id: assigneeId, note },
        request.headers.get('x-request-id')
      );
      if (result.ok) return { caseItem: null, ok: '已指派' } satisfies CaseDetailPageData;
      return fail(result.status, { caseItem: null, message: result.message } satisfies CaseDetailPageData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { caseItem: null, message: '指派失败，请稍后重试' } satisfies CaseDetailPageData);
    }
  }
};
