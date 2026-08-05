// M03-UI-07：管理标签页——列表（后端裁决）+ 新建标签表单。
import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPost, getAuthed } from '$lib/api/server';
import { adminListState, type AdminLoadState } from '$lib/admin';
import type { Tag } from '$lib/api/types';

export interface AdminTagsPageData {
  loadState: AdminLoadState<Tag>;
  created?: boolean;
  message?: string;
  requestId?: string | null;
}

export const load: PageServerLoad = async ({ cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<{ items: Tag[] }>(cookies, '/api/v1/admin/tags', requestId);
  if (!result.ok && result.status === 401) throw redirect(303, '/login');
  return { loadState: adminListState(result) } satisfies AdminTagsPageData;
};

export const actions: Actions = {
  create: async ({ request, cookies }) => {
    const form = await request.formData();
    const reason = String(form.get('reason') ?? '').trim();
    const name = String(form.get('name') ?? '').trim();
    if (!reason || !name) {
      return fail(422, { loadState: { state: 'error', message: '名称与操作原因均必填' } } satisfies AdminTagsPageData);
    }
    try {
      const result = await authedPost<unknown>(
        cookies,
        '/api/v1/admin/tags',
        { name, reason },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { loadState: { state: 'ok', items: [] }, created: true } satisfies AdminTagsPageData;
      }
      if (result.status === 403) {
        return fail(403, { loadState: { state: 'forbidden', message: result.message } } satisfies AdminTagsPageData);
      }
      return fail(result.status, { loadState: { state: 'error', message: result.message } } satisfies AdminTagsPageData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { loadState: { state: 'error', message: '保存失败，请稍后重试' } } satisfies AdminTagsPageData);
    }
  }
};
