// M03-UI-06：板块详情 SSR——服务端取板块详情与帖子（转发会话 Cookie），
// 不存在/隐藏板块 → 404（不泄漏存在性，M03-BOARDS-03）；前端不直接打 /api。
//
// GAP-FIX（板块页增强）：
// - 排序 tab：?sort=latest|hot 透传给 GET /boards/{slug}/posts（后端已实现
//   sort=hot 浏览量倒序；游标仍为 created_at 键序）；
// - 关注板块：load 取 viewer following 态（GET /me/following boards 列表），
//   follow/unfollow action（authedPost/authedDeleteBody 服务端代理 + 401→/login）；
// - 侧栏：板块信息卡（today_post_count 为 authed 聚合；版主数据后端无投影，
//   不展示）+ 全站热门标签（/tags 无按板块过滤端点，暂以全站热门顶置）。
import { error, fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedDelete, authedPost, getAuthed, SESSION_COOKIE } from '$lib/api/server';
import type { Board, PageResult, PostSummary, Tag } from '$lib/api/types';

/** today_post_count 不在契约 Board 类型里（GAP-FIX 社交域 authed 聚合，
 *  UTC 日界内新帖），局部扩展而不是改 types.ts。 */
type BoardWithToday = Board & { today_post_count?: number };

export interface BoardDetailData {
  board: BoardWithToday | null;
  posts: PostSummary[];
  error: string | null;
  /** 当前排序（latest 默认 | hot | featured | unanswered，M18-BOARD-01）。 */
  sort: 'latest' | 'hot' | 'featured' | 'unanswered';
  /** 作者/标题过滤词（M18-BOARD-01，原型「按作者或标题筛选…」）。 */
  q: string;
  /** viewer 是否已关注该板块（匿名/拉取失败 → false）。 */
  following: boolean;
  /** 请求方是否带会话 Cookie（关注按钮门控；真实鉴权由后端裁决）。 */
  authed: boolean;
  /** 侧栏热门标签（全站 usage 排序前 10；无按板块过滤端点，注释顶置说明）。 */
  tags: Tag[];
}

export interface BoardFollowActionData {
  ok?: boolean;
  message?: string;
}

export const load: PageServerLoad = async ({ params, cookies, request, url }) => {
  const requestId = request.headers.get('x-request-id');
  const slug = params.slug;
  // 关注板块是登录操作：无会话 Cookie → 页面渲染登录引导而非关注表单
  // （真值判断：cookies.get 缺失返回 undefined，非 null，`!== null` 恒真）。
  const authed = Boolean(cookies.get(SESSION_COOKIE));
  // M18-BOARD-01：latest（默认）| hot | featured | unanswered 透传后端。
  const rawSort = url.searchParams.get('sort');
  const sort =
    rawSort === 'hot' || rawSort === 'featured' || rawSort === 'unanswered'
      ? rawSort
      : 'latest';
  // 作者/标题过滤（原型「按作者或标题筛选…」；trim + 截断 100 与契约一致）。
  const q = (url.searchParams.get('q') ?? '').trim().slice(0, 100);

  const boardResult = await getAuthed<BoardWithToday>(
    cookies,
    `/api/v1/boards/${encodeURIComponent(slug)}`,
    requestId
  );
  if (!boardResult.ok) {
    // 404（不存在/hidden 板块）→ 与后端一致不泄漏存在性。
    if (boardResult.status === 404) throw error(404, '板块不存在或不可见');
    return {
      board: null,
      posts: [],
      error: boardResult.message,
      sort,
      q,
      following: false,
      authed,
      tags: []
    } satisfies BoardDetailData;
  }
  // 后端已实现 sort=hot/featured/unanswered（M18-BOARD-01）与 q 过滤，透传即可。
  const postsParams = new URLSearchParams({ sort });
  if (q) postsParams.set('q', q);
  const postsResult = await getAuthed<PageResult<PostSummary>>(
    cookies,
    `/api/v1/boards/${encodeURIComponent(slug)}/posts?${postsParams.toString()}`,
    requestId
  );

  // viewer 关注态：GET /me/following 返回 {users, boards}（slug 列表）。
  // 匿名（无会话 Cookie）直接 false，不打无谓请求。
  // 注意真值判断：cookies.get 缺失返回 undefined（非 null），`!== null` 恒真。
  let following = false;
  if (cookies.get(SESSION_COOKIE)) {
    try {
      const me = await getAuthed<{ boards?: string[] }>(cookies, '/api/v1/me/following', requestId);
      if (me.ok && Array.isArray(me.data.boards)) {
        following = me.data.boards.includes(slug);
      }
    } catch {
      following = false;
    }
  }

  // 侧栏热门标签：/tags 无按板块过滤的端点（BE 未提供 board 维度），
  // 暂以全站热门标签顶置（usage_count 降序前 10），待后端支持后替换。
  let tags: Tag[] = [];
  try {
    const tagsResult = await getAuthed<{ items?: Tag[] }>(cookies, '/api/v1/tags', requestId);
    if (tagsResult.ok && Array.isArray(tagsResult.data.items)) {
      tags = tagsResult.data.items
        .slice()
        .sort((a, b) => (b.usage_count ?? 0) - (a.usage_count ?? 0))
        .slice(0, 10);
    }
  } catch {
    tags = [];
  }

  return {
    board: boardResult.data,
    posts: postsResult.ok ? postsResult.data.items : [],
    error: postsResult.ok ? null : postsResult.message,
    sort,
    q,
    following,
    authed,
    tags
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
