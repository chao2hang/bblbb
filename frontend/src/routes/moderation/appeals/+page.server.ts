// M05-UI-06：申诉页——列表 + 创建（申诉人侧安全投影，无内部 note）。
import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPost, getAuthed } from '$lib/api/server';
import type { OwnAppeal } from '$lib/api/types';

export interface AppealsPageData {
  items: OwnAppeal[];
  message?: string | null;
  submitted?: OwnAppeal | null;
}

export const load: PageServerLoad = async ({ cookies, request }): Promise<AppealsPageData> => {
  const requestId = request.headers.get('x-request-id');
  const me = await getAuthed<unknown>(cookies, '/api/v1/me', requestId);
  if (!me.ok && me.status === 401) throw redirect(303, '/login');
  const result = await getAuthed<{ items: OwnAppeal[] }>(cookies, '/api/v1/appeals', requestId);
  const items = result.ok ? result.data.items : [];
  return { items, submitted: null } satisfies AppealsPageData;
};

export const actions: Actions = {
  create: async ({ request, cookies }) => {
    const form = await request.formData();
    const sanctionId = String(form.get('sanction_id') ?? '').trim();
    const content = String(form.get('content') ?? '').trim();
    if (!sanctionId || !content) {
      return fail(422, { items: [], message: '处罚 ID 与申诉内容均必填' } satisfies AppealsPageData);
    }
    try {
      const result = await authedPost<OwnAppeal>(
        cookies,
        '/api/v1/appeals',
        { sanction_id: sanctionId, content },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { items: [], submitted: result.data, message: null } satisfies AppealsPageData;
      }
      // M05-UI-05：窗口/重复/越权由 API 拒绝并稳定呈现。
      return fail(result.status, { items: [], message: result.message, submitted: null } satisfies AppealsPageData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { items: [], message: '提交失败，请稍后重试', submitted: null } satisfies AppealsPageData);
    }
  }
};
