// M03-UI-01：用户主页 SSR load——公开资料安全投影
//
// - 服务端转发 GET /api/v1/users/{username}（getAuthed：转发会话 Cookie 与
//   X-Request-ID，回传 Set-Cookie 属性，解析 RFC 7807 Problem）；
// - 404（不存在/已注销/匿名化）→ SvelteKit error(404)——与后端一致不泄漏
//   存在性；5xx → error(500)；其余 → error(status)；
// - banned/pending_delete → 后端 200 安全降级投影（bio/signature/头像/Cover
//   置空，不泄漏状态），页面按公开投影渲染降级态；
// - 返回类型仅 PUBLIC_PROFILE allowlist 九字段（$lib/api/types PublicProfile）。
// - GAP-FIX 社交域：follow / unfollow actions（POST/DELETE
//   /api/v1/users/{username}/follow，follows.rs BE-1 已落地）；load 本身
//   返回形状保持 { user }（load.test.ts 契约），社交统计
//   post_count/followers/following/is_following 由后端 PublicProfile 附带。
import { error, fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { getAuthed } from '$lib/api/server';
import { followUser, unfollowUser } from '$lib/api/client';
import { problemMessage, type Problem } from '$lib/errors';
import type { PublicProfile } from '$lib/api/types';

export interface UserPageData {
  user: PublicProfile;
}

export interface UserFollowActionData {
  ok?: boolean;
  following?: boolean;
  message?: string;
  requestId?: string | null;
}

export const load: PageServerLoad = async ({ params, cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const username = params.username;
  const result = await getAuthed<PublicProfile>(
    cookies,
    `/api/v1/users/${encodeURIComponent(username)}`,
    requestId
  );
  if (result.ok) {
    return { user: result.data } satisfies UserPageData;
  }
  if (result.status === 404) {
    throw error(404, '用户不存在或已注销');
  }
  if (result.status >= 500) {
    throw error(500, '服务暂时不可用，请稍后重试');
  }
  throw error(result.status, result.message || '获取用户资料失败');
};

function asProblem(e: unknown): Problem | null {
  return e && typeof e === 'object' ? (e as Problem) : null;
}

export const actions: Actions = {
  // 关注 / 取关（GAP-FIX follows.rs）：会话过期 → redirect /login（enhance
  // 自动跟随）；成功由页面 toast + invalidateAll 刷新统计与 is_following。
  // 注意 redirect 在 try 外抛出（避免被本 action 的 catch 吞掉）。
  follow: async ({ params, fetch }) => {
    const username = params.username;
    try {
      await followUser(fetch, username);
    } catch (e) {
      const p = asProblem(e);
      if (p?.status === 401) throw redirect(303, '/login');
      const status = typeof p?.status === 'number' && p.status >= 400 && p.status <= 599 ? p.status : 503;
      return fail(status, {
        following: false,
        message: problemMessage(p),
        requestId: p?.request_id ?? null
      } satisfies UserFollowActionData);
    }
    return {
      ok: true,
      following: true,
      message: `已关注 ${username}`
    } satisfies UserFollowActionData;
  },
  unfollow: async ({ params, fetch }) => {
    const username = params.username;
    try {
      await unfollowUser(fetch, username);
    } catch (e) {
      const p = asProblem(e);
      if (p?.status === 401) throw redirect(303, '/login');
      const status = typeof p?.status === 'number' && p.status >= 400 && p.status <= 599 ? p.status : 503;
      return fail(status, {
        following: true,
        message: problemMessage(p),
        requestId: p?.request_id ?? null
      } satisfies UserFollowActionData);
    }
    return {
      ok: true,
      following: false,
      message: `已取消关注 ${username}`
    } satisfies UserFollowActionData;
  }
};
