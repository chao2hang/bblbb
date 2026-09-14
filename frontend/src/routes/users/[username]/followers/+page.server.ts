// M03-UI-03 / 社交域·关注：/users/{username}/followers——粉丝列表 SSR load。
//
// - 服务端转发 GET /api/v1/users/{username}/followers（公开端点，
//   listFollowers ?after= 游标分页；无 JS 回退为同名 ?after= 链接整页翻页）；
// - 404（用户不存在/已注销/匿名化）→ SvelteKit error(404)——与用户主页
//   load 一致，不泄漏存在性；>=500 或非 Problem 异常 → error(500)；
//   其余 4xx → error(status) 透传。
// - 返回仅公开投影行（username/display_name/level/follow created_at）。
import { error } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { listFollowers } from '$lib/api/client';
import type { Problem } from '$lib/errors';
import type { FollowUserItem } from '$lib/components/users/FollowUserList.svelte';

export interface UserFollowersPageData {
  username: string;
  items: FollowUserItem[];
  /** 下一页游标（空串视为无下一页；无 JS 分页回退 ?after= 用）。 */
  nextCursor: string | null;
  /** 当前 ?after= 游标（回显用）。 */
  after: string | null;
}

function asProblem(e: unknown): Problem | null {
  return e && typeof e === 'object' ? (e as Problem) : null;
}

export const load: PageServerLoad = async ({ params, fetch, url }) => {
  const username = params.username;
  const after = url.searchParams.get('after');
  try {
    const page = await listFollowers(fetch, username, after);
    return {
      username,
      items: (page.items ?? []) as FollowUserItem[],
      nextCursor: page.next_cursor ? String(page.next_cursor) : null,
      after
    } satisfies UserFollowersPageData;
  } catch (e) {
    const p = asProblem(e);
    const status = typeof p?.status === 'number' ? p.status : 500;
    if (status === 404) {
      throw error(404, '用户不存在或已注销');
    }
    if (status >= 500) {
      throw error(500, '服务暂时不可用，请稍后重试');
    }
    throw error(status, '获取粉丝列表失败');
  }
};
