// M02-UX-SEC：/me/security 账号与安全页——从 /me 拆分的会话设备管理。
//
// - load：转发浏览器会话 Cookie → GET /api/v1/me（安全投影，含 mfa_enabled，
//   供两步验证状态行）与 GET /api/v1/auth/sessions（设备列表）；401 →
//   跳登录并携带 ?next= 回跳本页；
// - revoke / logoutall：Session 设备管理（自 /me/+page.server.ts 平移，
//   M02-UX-05）：逐设备撤销（?/revoke，隐藏 session_id）与退出全部设备
//   （?/logoutall）。写操作走会话绑定 synchronizer token（M02-SESSION-07，
//   $lib/api/server.ts authedDelete：转发会话 Cookie + GET /auth/csrf +
//   X-CSRF-Token），后端 Set-Cookie（含清 Cookie）逐属性复制到浏览器。
//
// 两步验证（TOTP/Passkey）的管理在专属页 /mfa（M18-MFA-01），本页只做
// 状态展示与入口，避免同一功能多处置理。

import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedDelete, getAuthed } from '$lib/api/server';
import type { User } from '$lib/api/types';
import type { DeviceSession } from '$lib/api/generated/v1';

export interface SecurityPageData {
  user: User | null;
  sessions: DeviceSession[];
  currentSessionId: string | null;
  error: string | null;
}

export interface SecurityActionData {
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
    if (meResult.status === 401) {
      throw redirect(303, `/login?next=${encodeURIComponent('/me/security')}`);
    }
    return { user: null, sessions: [], currentSessionId: null, error: meResult.message } satisfies SecurityPageData;
  }
  const sessionsResult = await getAuthed<DeviceSession[]>(
    cookies,
    '/api/v1/auth/sessions',
    requestId
  );
  if (sessionsResult.ok === false) {
    if (sessionsResult.status === 401) {
      throw redirect(303, `/login?next=${encodeURIComponent('/me/security')}`);
    }
    return {
      user: meResult.data,
      sessions: [],
      currentSessionId: null,
      error: sessionsResult.message
    } satisfies SecurityPageData;
  }

  const sessions = sessionsResult.data;
  return {
    user: meResult.data,
    sessions,
    currentSessionId: currentSessionId(sessions),
    error: null
  } satisfies SecurityPageData;
};

export const actions: Actions = {
  revoke: async ({ request, cookies }) => {
    const form = await request.formData();
    const sessionId = String(form.get('session_id') ?? '').trim();
    if (!sessionId) {
      return fail(422, { message: '缺少设备标识' } satisfies SecurityActionData);
    }
    try {
      const result = await authedDelete(
        cookies,
        `/api/v1/auth/sessions/${encodeURIComponent(sessionId)}`,
        request.headers.get('x-request-id')
      );
      if (result.ok) throw redirect(303, '/me/security');
      return fail(result.status, {
        message: result.message,
        requestId: result.requestId
      } satisfies SecurityActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '撤销设备失败，请稍后重试' } satisfies SecurityActionData);
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
      } satisfies SecurityActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '退出全部设备失败，请稍后重试' } satisfies SecurityActionData);
    }
  }
};
