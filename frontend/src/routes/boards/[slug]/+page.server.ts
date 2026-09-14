// M03-UI-06：板块详情 SSR——服务端取板块详情与帖子（转发会话 Cookie），
// 不存在/隐藏板块 → 404（不泄漏存在性，M03-BOARDS-03）。
//
// M18-BOARD-05（分类页与首页完全同构）：
// - 信息流切到与首页同一数据源 GET /api/v1/posts?board_id=…（$lib/api/feed），
//   排序 tab 与首页一致（最新|精华|已关注|热门），游标 ?after= 分页，
//   参与者预览 / 右栏推荐位一并提供；
// - 右栏「推荐内容」按板块过滤（sort=popular&board_id=…）——每个分区的
//   推荐内容随分区而不同；
// - 关注板块保留：load 取 viewer following 态（GET /me/following），
//   follow/unfollow action（服务端代理 + 401→/login）；板块导航侧栏沿用。
// 原板块自有端点 GET /boards/{slug}/posts 的 q 过滤与 unanswered 排序随
// 同构化下线（首页无此二能力；如需恢复另行评估）。
import { error, fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedDelete, authedPost, getAuthed, getPublic, SESSION_COOKIE } from '$lib/api/server';
import { fetchPostsPage, type FeedRow } from '$lib/api/feed';
import type { Board, PageResult } from '$lib/api/types';

/** today_post_count 不在契约 Board 类型里（GAP-FIX 社交域 authed 聚合，
 *  UTC 日界内新帖），局部扩展而不是改 types.ts。 */
type BoardWithToday = Board & { today_post_count?: number };

/** 分类页筛选 tab：与首页 HomeSort 一致。 */
export type BoardFeedSort = '' | 'featured' | 'following' | 'popular';

export interface BoardDetailData {
  board: BoardWithToday | null;
  /** 信息流行（首页同款白名单投影，board_id 恒为当前板块）。 */
  posts: FeedRow[];
  error: string | null;
  /** 当前排序（'' 最新默认 | featured 精华 | following 已关注 | popular 热门）。 */
  sort: BoardFeedSort;
  /** viewer 是否已关注该板块（匿名/拉取失败 → false）。 */
  following: boolean;
  /** 请求方是否带会话 Cookie（关注按钮门控；真实鉴权由后端裁决）。 */
  authed: boolean;
  /** 当前 ?after= 游标（无 JS 加载更多回退）。 */
  after: string | null;
  nextCursor: string | null;
  hasMore: boolean;
  /** 右栏推荐位（sort=popular 且按当前板块过滤的前 3，白名单投影）。 */
  popular: FeedRow[];
  /** 侧栏板块导航（GET /boards 全量列表；拉取失败降级为空数组 → 卡片隐藏）。 */
  boards: Board[];
}

export interface BoardFollowActionData {
  ok?: boolean;
  message?: string;
}

export const load: PageServerLoad = async ({ params, cookies, request, url, fetch }) => {
  const requestId = request.headers.get('x-request-id');
  const slug = params.slug;
  // 关注板块是登录操作：无会话 Cookie → 页面渲染登录引导而非关注表单
  // （真值判断：cookies.get 缺失返回 undefined，非 null，`!== null` 恒真）。
  const authed = Boolean(cookies.get(SESSION_COOKIE));
  // M18-BOARD-05：'' （最新默认）| featured | following | popular（与首页一致）。
  const rawSort = url.searchParams.get('sort');
  const sort: BoardFeedSort =
    rawSort === 'featured' || rawSort === 'following' || rawSort === 'popular'
      ? rawSort
      : '';
  const after = url.searchParams.get('after');

  const boardPath = `/api/v1/boards/${encodeURIComponent(slug)}`;
  const boardResult = authed
    ? await getAuthed<BoardWithToday>(cookies, boardPath, requestId)
    : await getPublic<BoardWithToday>(boardPath, requestId);
  if (!boardResult.ok) {
    // 404（不存在/hidden 板块）→ 与后端一致不泄漏存在性。
    if (boardResult.status === 404) throw error(404, '板块不存在或不可见');
    return {
      board: null,
      posts: [],
      error: boardResult.message,
      sort,
      following: false,
      authed,
      after,
      nextCursor: null,
      hasMore: false,
      popular: [],
      boards: []
    } satisfies BoardDetailData;
  }
  const boardId = boardResult.data?.id ?? null;

  // 信息流 + 右栏推荐位：与首页同一取页层（相对 /api/v1/posts，同源 Cookie
  // 透传），按板块过滤——feed 为本板块内容，推荐位为本板块热门前 3。
  // 关注态（authed）与其并发；失败静默降级 false。
  const [postsPage, popularPage, following] = await Promise.all([
    fetchPostsPage(fetch, { sort, limit: 8, after, boardId }),
    fetchPostsPage(fetch, { sort: 'popular', limit: 3, boardId }),
    (async () => {
      if (!cookies.get(SESSION_COOKIE)) return false;
      try {
        const me = await getAuthed<{ boards?: string[] }>(cookies, '/api/v1/me/following', requestId);
        return Boolean(me.ok && Array.isArray(me.data.boards) && me.data.boards.includes(slug));
      } catch {
        return false;
      }
    })()
  ]);

  // 板块导航：GET /boards 全量列表（authed 转发会话 → 计数按请求方可见性
  // 裁剪；匿名投影缺 post_count 时前端不渲染计数）。失败静默降级。
  let boards: Board[] = [];
  try {
    const boardsResult = authed
      ? await getAuthed<PageResult<Board>>(cookies, '/api/v1/boards', requestId)
      : await getPublic<PageResult<Board>>('/api/v1/boards', requestId);
    if (boardsResult.ok && Array.isArray(boardsResult.data.items)) {
      boards = boardsResult.data.items;
    }
  } catch {
    // 静默降级：导航是辅助内容，失败不阻塞板块页
  }

  return {
    board: boardResult.data,
    posts: postsPage.rows,
    error: null,
    sort,
    following,
    authed,
    after,
    nextCursor: postsPage.nextCursor,
    hasMore: postsPage.hasMore,
    popular: popularPage.rows,
    boards
  } satisfies BoardDetailData;
};

export const actions: Actions = {
  // 关注板块（幂等）：成功 → toast + invalidate；401 → /login（enhance 自动跟随）。
  follow: async ({ params, cookies, request }) => {
    const slug = params.slug;
    const result = await authedPost<{ following: boolean }>(
      cookies,
      `/api/v1/boards/${encodeURIComponent(slug)}/follow`,
      {},
      request.headers.get('x-request-id')
    );
    if (result.ok) return { ok: true, message: '已关注该板块' } satisfies BoardFollowActionData;
    if (result.status === 401) {
      throw redirect(303, `/login?next=${encodeURIComponent(`/boards/${slug}`)}`);
    }
    return fail(result.status, {
      ok: false,
      message: result.message
    } satisfies BoardFollowActionData);
  },
  // 取关板块（幂等；DELETE 无 body，与 client.ts unfollowBoard 形状一致）。
  unfollow: async ({ params, cookies, request }) => {
    const slug = params.slug;
    const result = await authedDelete(
      cookies,
      `/api/v1/boards/${encodeURIComponent(slug)}/follow`,
      request.headers.get('x-request-id')
    );
    if (result.ok) return { ok: true, message: '已取消关注' } satisfies BoardFollowActionData;
    if (result.status === 401) {
      throw redirect(303, `/login?next=${encodeURIComponent(`/boards/${slug}`)}`);
    }
    return fail(result.status, {
      ok: false,
      message: result.message
    } satisfies BoardFollowActionData);
  }
};
