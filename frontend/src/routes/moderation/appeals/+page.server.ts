// M05-UI-06 & M18-APPEAL：申诉中心（处罚案件 + 举报记录 + 申诉列表与创建，对齐原型）。
import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPost, getAuthed } from '$lib/api/server';
import type { OwnAppeal, SanctionItem } from '$lib/api/types';

export interface OwnReportItem {
  id: string;
  target_type: string;
  target_id: string;
  reason_code: string;
  status: string;
  created_at: number;
}

export interface AppealsPageData {
  items: OwnAppeal[];
  sanctions: SanctionItem[];
  reports: OwnReportItem[];
  message?: string | null;
  submitted?: OwnAppeal | null;
}

export interface AppealsActionData {
  message?: string | null;
  submitted?: OwnAppeal | null;
}

export const load: PageServerLoad = async ({ cookies, request }): Promise<AppealsPageData> => {
  const requestId = request.headers.get('x-request-id');
  const me = await getAuthed<unknown>(cookies, '/api/v1/me', requestId);
  if (!me.ok && me.status === 401) throw redirect(303, '/login');

  const [appealsRes, sanctionsRes, reportsRes] = await Promise.all([
    getAuthed<{ items: OwnAppeal[] }>(cookies, '/api/v1/appeals', requestId),
    getAuthed<{ items: SanctionItem[] }>(cookies, '/api/v1/me/sanctions', requestId),
    getAuthed<{ items: OwnReportItem[] }>(cookies, '/api/v1/reports', requestId)
  ]);

  const items = appealsRes.ok ? appealsRes.data.items : [];
  const sanctions = sanctionsRes.ok ? sanctionsRes.data.items : [];
  const reports = reportsRes.ok ? reportsRes.data.items : [];

  return { items, sanctions, reports, submitted: null } satisfies AppealsPageData;
};

export const actions: Actions = {
  create: async ({ request, cookies }) => {
    const form = await request.formData();
    const sanctionId = String(form.get('sanction_id') ?? '').trim();
    const content = String(form.get('content') ?? '').trim();
    if (!sanctionId || !content) {
      return fail(422, { message: '处罚 ID 与申诉内容均必填' } satisfies AppealsActionData);
    }
    try {
      const result = await authedPost<OwnAppeal>(
        cookies,
        '/api/v1/appeals',
        { sanction_id: sanctionId, content },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { submitted: result.data, message: null } satisfies AppealsActionData;
      }
      // M05-UI-05：窗口/重复/越权由 API 拒绝并稳定呈现。
      return fail(result.status, { message: result.message, submitted: null } satisfies AppealsActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '提交失败，请稍后重试', submitted: null } satisfies AppealsActionData);
    }
  }
};
