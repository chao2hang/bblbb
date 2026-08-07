// M05-UI-06：申诉详情——申诉人侧投影 + 未审理前撤回。
import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPost, getAuthed } from '$lib/api/server';
import type { OwnAppeal } from '$lib/api/types';

export interface AppealDetailPageData {
  appeal: OwnAppeal | null;
  message?: string | null;
  withdrawn?: boolean;
}

export const load: PageServerLoad = async (
  { params, cookies, request }
): Promise<AppealDetailPageData> => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<OwnAppeal>(
    cookies,
    `/api/v1/appeals/${encodeURIComponent(params.id)}`,
    requestId
  );
  if (!result.ok && result.status === 401) throw redirect(303, '/login');
  if (!result.ok) return { appeal: null, message: result.message } satisfies AppealDetailPageData;
  return { appeal: result.data } satisfies AppealDetailPageData;
};

export const actions: Actions = {
  withdraw: async ({ params, cookies, request }) => {
    try {
      const result = await authedPost<unknown>(
        cookies,
        `/api/v1/appeals/${encodeURIComponent(params.id)}/withdraw`,
        undefined,
        request.headers.get('x-request-id')
      );
      if (result.ok) return { appeal: null, withdrawn: true } satisfies AppealDetailPageData;
      return fail(result.status, { appeal: null, message: result.message } satisfies AppealDetailPageData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { appeal: null, message: '撤回失败，请稍后重试' } satisfies AppealDetailPageData);
    }
  }
};
