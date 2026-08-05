// M03-UI-07：管理板块页——列表（后端裁决）+ 新建板块表单。
// 列表接口当前为 501（M13-ADMIN 落地）；创建接口已实现（board.manage
// 权限门 + reason 审计，M03-BOARDS-05），表单在权限通过时可用。
import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPost, getAuthed } from '$lib/api/server';
import { adminListState, type AdminLoadState } from '$lib/admin';
import type { Board } from '$lib/api/types';

export interface AdminBoardsPageData {
  loadState: AdminLoadState<Board>;
  created?: boolean;
  message?: string;
  requestId?: string | null;
}

export const load: PageServerLoad = async ({ cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<{ items: Board[] }>(cookies, '/api/v1/admin/boards', requestId);
  if (!result.ok && result.status === 401) throw redirect(303, '/login');
  return { loadState: adminListState(result) } satisfies AdminBoardsPageData;
};

export const actions: Actions = {
  create: async ({ request, cookies }) => {
    const form = await request.formData();
    const reason = String(form.get('reason') ?? '').trim();
    const name = String(form.get('name') ?? '').trim();
    const slug = String(form.get('slug') ?? '').trim();
    if (!reason || !name || !slug) {
      return fail(422, { loadState: { state: 'error', message: '名称、slug 与操作原因均必填' } } satisfies AdminBoardsPageData);
    }
    try {
      const result = await authedPost<unknown>(
        cookies,
        '/api/v1/admin/boards',
        {
          name,
          slug,
          description: String(form.get('description') ?? '').trim() || null,
          visibility: String(form.get('visibility') ?? 'public'),
          posting_mode: String(form.get('posting_mode') ?? 'normal'),
          reason
        },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { loadState: { state: 'ok', items: [] }, created: true } satisfies AdminBoardsPageData;
      }
      if (result.status === 403) {
        return fail(403, { loadState: { state: 'forbidden', message: result.message } } satisfies AdminBoardsPageData);
      }
      return fail(result.status, { loadState: { state: 'error', message: result.message } } satisfies AdminBoardsPageData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { loadState: { state: 'error', message: '保存失败，请稍后重试' } } satisfies AdminBoardsPageData);
    }
  }
};
