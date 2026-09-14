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
//   /api/v1/users/{username}/follow，follows.rs BE-1 已落地）；load 返回
//   { user, authed }（authed = 请求方是否带会话 Cookie，关注按钮门控用，
//   真实鉴权由后端裁决），社交统计
//   post_count/followers/following/is_following 由后端 PublicProfile 附带。
import { error, fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPost, getAuthed, getPublic, SESSION_COOKIE } from '$lib/api/server';
import { followUser, newClientRequestId, unfollowUser } from '$lib/api/client';
import { problemMessage, type Problem } from '$lib/errors';
import type { PublicProfile } from '$lib/api/types';

export interface UserPageData {
  user: PublicProfile;
  /** 请求方是否带会话 Cookie（匿名 → 页面渲染登录引导而非关注表单）。 */
  authed: boolean;
}

export interface UserFollowActionData {
  ok?: boolean;
  following?: boolean;
  message?: string;
  requestId?: string | null;
}

export interface UserMessageActionData {
  ok?: boolean;
  message?: string;
  requestId?: string | null;
}

export const load: PageServerLoad = async ({ params, cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const username = params.username;
  // 关注是登录操作：无会话 Cookie → 页面渲染登录引导而非关注表单
  // （真值判断：cookies.get 缺失返回 undefined，非 null，`!== null` 恒真）。
  const authed = Boolean(cookies.get(SESSION_COOKIE));
  const profilePath = `/api/v1/users/${encodeURIComponent(username)}`;
  const result = authed
    ? await getAuthed<PublicProfile>(cookies, profilePath, requestId)
    : await getPublic<PublicProfile>(profilePath, requestId);
  if (result.ok) {
    return { user: result.data, authed } satisfies UserPageData;
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
  // 发私信：创建或复用双人会话后跳入 /messages；请求经过会话绑定
  // CSRF，幂等键用于网络重试时保持请求身份稳定。
  message: async ({ params, cookies, request }) => {
    const username = params.username;
    const clientRequestId = newClientRequestId();
    const result = await authedPost<{ id?: string }>(
      cookies,
      '/api/v1/conversations',
      { username, client_request_id: clientRequestId },
      request.headers.get('x-request-id'),
      { 'Idempotency-Key': clientRequestId }
    );
    if (result.ok && result.data?.id) {
      throw redirect(303, `/messages?c=${encodeURIComponent(result.data.id)}`);
    }
    if (!result.ok && result.status === 401) {
      throw redirect(303, `/login?next=${encodeURIComponent(`/users/${username}`)}`);
    }
    if (!result.ok) {
      return fail(result.status, {
        message: result.message,
        requestId: result.requestId
      } satisfies UserMessageActionData);
    }
    return fail(503, {
      message: '会话创建结果无效，请稍后重试'
    } satisfies UserMessageActionData);
  },
  // 关注 / 取关（GAP-FIX follows.rs）：会话过期 → redirect /login（enhance
  // 自动跟随）；成功由页面 toast + invalidateAll 刷新统计与 is_following。
  // 注意 redirect 在 try 外抛出（避免被本 action 的 catch 吞掉）。
  follow: async ({ params, fetch }) => {
    const username = params.username;
    try {
      await followUser(fetch, username);
    } catch (e) {
      const p = asProblem(e);
      if (p?.status === 401) {
        throw redirect(303, `/login?next=${encodeURIComponent(`/users/${username}`)}`);
      }
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
      if (p?.status === 401) {
        throw redirect(303, `/login?next=${encodeURIComponent(`/users/${username}`)}`);
      }
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
