// M03-UI-06：板块总览 SSR——服务端取板块列表（转发会话 Cookie 使可见性
// 按请求方裁剪），渲染板块树（parent_id 层级）；前端不再直接打 /api。
import type { PageServerLoad } from './$types';
import { getAuthed } from '$lib/api/server';
import type { Board, PageResult } from '$lib/api/types';

export interface BoardsPageData {
  boards: Board[];
  error: string | null;
}

export const load: PageServerLoad = async ({ cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<PageResult<Board>>(cookies, '/api/v1/boards', requestId);
  if (!result.ok) {
    return { boards: [], error: result.message } satisfies BoardsPageData;
  }
  return { boards: result.data.items, error: null } satisfies BoardsPageData;
};
