// M03-UI-03 / 社交域·关注：/users/{username}/following——正在关注列表 SSR load。
//
// 与 followers/+page.server.ts 同构：公开端点
// GET /api/v1/users/{username}/following（listFollowing ?after= 游标）；
// 404 → error(404)（不泄漏存在性）；>=500 或非 Problem 异常 → error(500)；
// 其余 4xx → error(status) 透传。
import { error } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { listFollowing } from '$lib/api/client';
import type { Problem } from '$lib/errors';
import type { FollowUserItem } from '$lib/components/users/FollowUserList.svelte';

export interface UserFollowingPageData {
  username: string;
  items: FollowUserItem[];
  nextCursor: string | null;
  after: string | null;
}

function asProblem(e: unknown): Problem | null {
  return e && typeof e === 'object' ? (e as Problem) : null;
}

export const load: PageServerLoad = async ({ params, fetch, url }) => {
  const username = params.username;
  const after = url.searchParams.get('after');
  try {
    const page = await listFollowing(fetch, username, after);
    return {
      username,
      items: (page.items ?? []) as FollowUserItem[],
      nextCursor: page.next_cursor ? String(page.next_cursor) : null,
      after
    } satisfies UserFollowingPageData;
  } catch (e) {
    const p = asProblem(e);
    const status = typeof p?.status === 'number' ? p.status : 500;
    if (status === 404) {
      throw error(404, '用户不存在或已注销');
    }
    if (status >= 500) {
      throw error(500, '服务暂时不可用，请稍后重试');
    }
    throw error(status, '获取关注列表失败');
  }
};
