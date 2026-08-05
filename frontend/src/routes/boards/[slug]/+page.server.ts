// M03-UI-06：板块详情 SSR——服务端取板块详情与帖子（转发会话 Cookie），
// 不存在/隐藏板块 → 404（不泄漏存在性，M03-BOARDS-03）；前端不直接打 /api。
import { error } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { getAuthed } from '$lib/api/server';
import type { Board, PageResult, PostSummary } from '$lib/api/types';

export interface BoardDetailData {
  board: Board | null;
  posts: PostSummary[];
  error: string | null;
}

export const load: PageServerLoad = async ({ params, cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const slug = params.slug;
  const boardResult = await getAuthed<Board>(
    cookies,
    `/api/v1/boards/${encodeURIComponent(slug)}`,
    requestId
  );
  if (!boardResult.ok) {
    // 404（不存在/hidden 板块）→ 与后端一致不泄漏存在性。
    if (boardResult.status === 404) throw error(404, '板块不存在或不可见');
    return { board: null, posts: [], error: boardResult.message } satisfies BoardDetailData;
  }
  const postsResult = await getAuthed<PageResult<PostSummary>>(
    cookies,
    `/api/v1/boards/${encodeURIComponent(slug)}/posts`,
    requestId
  );
  return {
    board: boardResult.data,
    posts: postsResult.ok ? postsResult.data.items : [],
    error: postsResult.ok ? null : postsResult.message
  } satisfies BoardDetailData;
};
