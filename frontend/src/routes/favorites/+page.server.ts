// GAP-FIX（社交域·收藏，favorites.rs）：/favorites——我收藏的帖子列表。
//
// - load：未登录 → /login；listMyFavorites（?after= 游标分页，无 JS 可用）；
// - unfavorite action：DELETE favorite（unfavoritePost；幂等）→ toast +
//   invalidateAll 刷新列表；
// - 失败态：返回 Problem（ProblemState 渲染），不抛 500。
import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { listMyFavorites, unfavoritePost } from '$lib/api/client';
import type { PostSummary } from '$lib/api/types';
import { problemMessage, type Problem } from '$lib/errors';

export interface FavoritesPageData {
  items: PostSummary[];
  nextCursor: string | null;
  hasMore: boolean;
  /** 当前 ?after= 游标（无 JS 分页回退用）。 */
  after: string | null;
  problem: Problem | null;
  error: string | null;
}

export interface FavoritesActionData {
  ok?: boolean;
  message?: string;
}

function asProblem(e: unknown): Problem | null {
  return e && typeof e === 'object' ? (e as Problem) : null;
}

export const load: PageServerLoad = async ({ fetch, url }) => {
  const after = url.searchParams.get('after');
  try {
    const page = await listMyFavorites(fetch, after);
    return {
      items: page.items ?? [],
      nextCursor: page.next_cursor ?? null,
      hasMore: page.has_more === true,
      after,
      problem: null,
      error: null
    } satisfies FavoritesPageData;
  } catch (e) {
    const p = asProblem(e);
    if (p?.status === 401) throw redirect(303, '/login');
    return {
      items: [],
      nextCursor: null,
      hasMore: false,
      after,
      problem: p,
      error: problemMessage(p)
    } satisfies FavoritesPageData;
  }
};

export const actions: Actions = {
  unfavorite: async ({ request, fetch }) => {
    const form = await request.formData();
    const postId = String(form.get('post_id') ?? '').trim();
    if (!postId) {
      return fail(422, { message: '缺少帖子标识，请刷新后重试' } satisfies FavoritesActionData);
    }
    try {
      await unfavoritePost(fetch, postId);
      return { ok: true, message: '已取消收藏' } satisfies FavoritesActionData;
    } catch (e) {
      const p = asProblem(e);
      if (p?.status === 401) {
        return fail(401, { message: '登录已过期，请重新登录' } satisfies FavoritesActionData);
      }
      const status = typeof p?.status === 'number' && p.status >= 400 && p.status <= 599 ? p.status : 503;
      return fail(status, { message: problemMessage(p) } satisfies FavoritesActionData);
    }
  }
};
