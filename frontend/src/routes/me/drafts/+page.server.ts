// M04-UI-03：草稿列表 SSR——服务端转发会话 Cookie 取回本人草稿
// （listDrafts，post.read_own）；401 → 登录。删除在页面内客户端完成。
import { redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { getAuthed } from '$lib/api/server';
import type { Draft } from '$lib/api/types';

export interface DraftsPageData {
  drafts: Draft[];
  error: string | null;
}

export const load: PageServerLoad = async ({ cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<{ items: Draft[] }>(cookies, '/api/v1/drafts', requestId);
  if (result.ok === false) {
    if (result.status === 401) throw redirect(303, '/login');
    return { drafts: [], error: result.message } satisfies DraftsPageData;
  }
  return { drafts: result.data.items ?? [], error: null } satisfies DraftsPageData;
};
