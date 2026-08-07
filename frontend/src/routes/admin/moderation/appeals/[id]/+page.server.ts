// M05-UI-06：复核申诉——决定表单（uphold/partial/reject + reason + 乐观版本）。
import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPatch, getAuthed } from '$lib/api/server';
import type { ModerationAppealDetail } from '$lib/api/types';

export interface AdminAppealPageData {
  appeal: ModerationAppealDetail | null;
  forbidden?: boolean;
  message?: string | null;
  ok?: string;
}

export const load: PageServerLoad = async (
  { params, cookies, request }
): Promise<AdminAppealPageData> => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<ModerationAppealDetail>(
    cookies,
    `/api/v1/admin/moderation/appeals/${encodeURIComponent(params.id)}`,
    requestId
  );
  if (!result.ok && result.status === 401) throw redirect(303, '/login');
  if (!result.ok && result.status === 403) return { appeal: null, forbidden: true, message: result.message } satisfies AdminAppealPageData;
  if (!result.ok) return { appeal: null, message: result.message } satisfies AdminAppealPageData;
  return { appeal: result.data } satisfies AdminAppealPageData;
};

export const actions: Actions = {
  decide: async ({ params, request, cookies }) => {
    const form = await request.formData();
    const decision = String(form.get('decision') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim();
    const expectedVersion = Number(String(form.get('expected_version') ?? ''));
    if (!decision || !reason || !Number.isFinite(expectedVersion)) {
      return fail(422, { appeal: null, message: '决定、理由与版本号均必填' } satisfies AdminAppealPageData);
    }
    try {
      const result = await authedPatch<{ status: string }>(
        cookies,
        `/api/v1/admin/moderation/appeals/${encodeURIComponent(params.id)}`,
        { decision, reason, expected_version: expectedVersion },
        {},
        request.headers.get('x-request-id')
      );
      if (result.ok) return { appeal: null, ok: `申诉已决定：${result.data.status}` } satisfies AdminAppealPageData;
      // M05-UI-05：利益冲突/越权/并发版本冲突由 API 拒绝并稳定呈现。
      return fail(result.status, { appeal: null, message: result.message } satisfies AdminAppealPageData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { appeal: null, message: '操作失败，请稍后重试' } satisfies AdminAppealPageData);
    }
  }
};
