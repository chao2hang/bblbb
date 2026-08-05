// M02-UX-05：/me 页服务端 load 与表单 action
//
// - load：转发浏览器会话 Cookie → GET /api/v1/me（安全投影）与
//   GET /api/v1/auth/sessions（设备列表）；会话无效（401）→ 跳登录；
// - revoke：逐设备撤销（DELETE /api/v1/auth/sessions/{id}，限本人，
//   他人 404），成功后回 /me 刷新列表；撤销的是当前设备时后续 load
//   401 → 自然跳转登录；
// - logoutall：撤销全部设备（DELETE /api/v1/auth/sessions，204 + 清
//   Cookie），成功后跳登录。
//
// 写操作走会话绑定 synchronizer token（M02-SESSION-07，$lib/api/server.ts
// authedDelete：转发会话 Cookie + GET /auth/csrf 取 token + X-CSRF-Token），
// 后端 Set-Cookie（含清 Cookie）逐属性复制到浏览器。

import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedDelete, getAuthed } from '$lib/api/server';
import type { User } from '$lib/api/types';
import type { DeviceSession } from '$lib/api/generated/v1';

export interface MePageData {
  user: User | null;
  sessions: DeviceSession[];
  currentSessionId: string | null;
  error: string | null;
}

export interface MeActionData {
  message?: string;
  requestId?: string | null;
}

function currentSessionId(sessions: DeviceSession[]): string | null {
  if (sessions.length === 0) return null;
  // 后端 resolve_session 每次请求更新 last_seen_at（滑动超时），本次 load
  // 自身刚通过会话认证，故 last_seen_at 最大的即是当前设备。
  return sessions.reduce((a, b) => (b.last_seen_at > a.last_seen_at ? b : a)).id;
}

export const load: PageServerLoad = async ({ cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const meResult = await getAuthed<User>(cookies, '/api/v1/me', requestId);
  if (meResult.ok === false) {
    if (meResult.status === 401) throw redirect(303, '/login');
    return { user: null, sessions: [], currentSessionId: null, error: meResult.message } satisfies MePageData;
  }
  const sessionsResult = await getAuthed<DeviceSession[]>(
    cookies,
    '/api/v1/auth/sessions',
    requestId
  );
  if (sessionsResult.ok === false) {
    if (sessionsResult.status === 401) throw redirect(303, '/login');
    return {
      user: meResult.data,
      sessions: [],
      currentSessionId: null,
      error: sessionsResult.message
    } satisfies MePageData;
  }
  const sessions = sessionsResult.data;
  return {
    user: meResult.data,
    sessions,
    currentSessionId: currentSessionId(sessions),
    error: null
  } satisfies MePageData;
};

export const actions: Actions = {
  revoke: async ({ request, cookies }) => {
    const form = await request.formData();
    const sessionId = String(form.get('session_id') ?? '').trim();
    if (!sessionId) {
      return fail(422, { message: '缺少设备标识' } satisfies MeActionData);
    }
    try {
      const result = await authedDelete(
        cookies,
        `/api/v1/auth/sessions/${encodeURIComponent(sessionId)}`,
        request.headers.get('x-request-id')
      );
      if (result.ok) throw redirect(303, '/me');
      return fail(result.status, {
        message: result.message,
        requestId: result.requestId
      } satisfies MeActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '撤销设备失败，请稍后重试' } satisfies MeActionData);
    }
  },
  logoutall: async ({ request, cookies }) => {
    try {
      const result = await authedDelete(
        cookies,
        '/api/v1/auth/sessions',
        request.headers.get('x-request-id')
      );
      if (result.ok) throw redirect(303, '/login');
      return fail(result.status, {
        message: result.message,
        requestId: result.requestId
      } satisfies MeActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '退出全部设备失败，请稍后重试' } satisfies MeActionData);
    }
  }
};
