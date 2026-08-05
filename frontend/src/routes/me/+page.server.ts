// M02-UX-05/06：/me 页服务端 load 与表单 action
//
// - load：转发浏览器会话 Cookie → GET /api/v1/me（安全投影，含
//   mfa_enabled）与 GET /api/v1/auth/sessions（设备列表）；401 → 跳登录；
// - revoke / logoutall：Session 设备管理（M02-UX-05）；
// - mfa-enroll / mfa-confirm / mfa-cancel：TOTP enrollment 三步
//   （M02-UX-06）；
// - mfa-recovery：生成新一组恢复码（只展示一次），403 step_up_required
//   时返回 step-up 态；mfa-disable：停用 MFA，同样要求 step-up；
// - re-auth：step-up 重认证（M02-MFA-07），成功返回 reauth-done 态，
//   页面引导重试原操作。
//
// 写操作走会话绑定 synchronizer token（M02-SESSION-07，$lib/api/server.ts
// authedDelete/authedPost：转发会话 Cookie + GET /auth/csrf + X-CSRF-Token），
// 后端 Set-Cookie（含清 Cookie）逐属性复制到浏览器。

import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedDelete, authedPost, getAuthed } from '$lib/api/server';
import type { User } from '$lib/api/types';
import type { DeviceSession } from '$lib/api/generated/v1';

export interface MePageData {
  user: User | null;
  sessions: DeviceSession[];
  currentSessionId: string | null;
  error: string | null;
}

export type MfaStep =
  | { kind: 'enroll-challenge'; otpauth_uri: string; secret_base32: string }
  | { kind: 'enroll-confirmed' }
  | { kind: 'recovery-codes'; codes: string[] }
  | { kind: 'disabled' }
  | { kind: 'step-up'; intent: 'recovery' | 'disable' }
  | { kind: 'reauth-done'; intent: 'recovery' | 'disable' };

export interface MeActionData {
  message?: string;
  requestId?: string | null;
  mfa?: MfaStep;
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

/** 403 step_up_required → 返回 step-up 态；其余失败透传 fail。 */
function stepUpOrFail(
  result: { ok: boolean; status: number; message: string; requestId: string | null; code: string | null },
  intent: 'recovery' | 'disable'
): ReturnType<typeof fail> | { mfa: { kind: 'step-up'; intent: 'recovery' | 'disable' } } {
  if (!result.ok && result.code === 'step_up_required') {
    return { mfa: { kind: 'step-up', intent } } satisfies MeActionData;
  }
  return fail(result.status, {
    message: result.message,
    requestId: result.requestId
  } satisfies MeActionData);
}

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
  },
  'mfa-enroll': async ({ request, cookies }) => {
    try {
      const result = await authedPost<{ otpauth_uri: string; secret_base32: string }>(
        cookies,
        '/api/v1/auth/mfa/enroll',
        undefined,
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return {
          mfa: {
            kind: 'enroll-challenge',
            otpauth_uri: result.data.otpauth_uri,
            secret_base32: result.data.secret_base32
          }
        } satisfies MeActionData;
      }
      return fail(result.status, {
        message: result.message,
        requestId: result.requestId
      } satisfies MeActionData);
    } catch {
      return fail(503, { message: '两步验证服务暂不可用，请稍后重试' } satisfies MeActionData);
    }
  },
  'mfa-confirm': async ({ request, cookies }) => {
    const form = await request.formData();
    const code = String(form.get('code') ?? '').trim();
    if (!/^[0-9]{6}$/.test(code)) {
      return fail(422, { message: '请输入 6 位验证码' } satisfies MeActionData);
    }
    try {
      const result = await authedPost(
        cookies,
        '/api/v1/auth/mfa/confirm',
        { code },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { mfa: { kind: 'enroll-confirmed' } } satisfies MeActionData;
      }
      return fail(result.status, {
        message: result.message,
        requestId: result.requestId
      } satisfies MeActionData);
    } catch {
      return fail(503, { message: '两步验证服务暂不可用，请稍后重试' } satisfies MeActionData);
    }
  },
  'mfa-cancel': async ({ request, cookies }) => {
    try {
      const result = await authedDelete(
        cookies,
        '/api/v1/auth/mfa/enrollment',
        request.headers.get('x-request-id')
      );
      if (result.ok) return { mfa: { kind: 'disabled' } } satisfies MeActionData;
      return fail(result.status, {
        message: result.message,
        requestId: result.requestId
      } satisfies MeActionData);
    } catch {
      return fail(503, { message: '两步验证服务暂不可用，请稍后重试' } satisfies MeActionData);
    }
  },
  'mfa-recovery': async ({ request, cookies }) => {
    try {
      const result = await authedPost<{ codes: string[] }>(
        cookies,
        '/api/v1/auth/mfa/recovery-codes',
        undefined,
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { mfa: { kind: 'recovery-codes', codes: result.data.codes } } satisfies MeActionData;
      }
      return stepUpOrFail(result, 'recovery');
    } catch {
      return fail(503, { message: '两步验证服务暂不可用，请稍后重试' } satisfies MeActionData);
    }
  },
  'mfa-disable': async ({ request, cookies }) => {
    try {
      const result = await authedDelete(
        cookies,
        '/api/v1/auth/mfa',
        request.headers.get('x-request-id')
      );
      if (result.ok) return { mfa: { kind: 'disabled' } } satisfies MeActionData;
      return stepUpOrFail(result, 'disable');
    } catch {
      return fail(503, { message: '两步验证服务暂不可用，请稍后重试' } satisfies MeActionData);
    }
  },
  're-auth': async ({ request, cookies }) => {
    const form = await request.formData();
    const password = String(form.get('password') ?? '');
    const intent = String(form.get('intent') ?? 'disable') === 'recovery' ? 'recovery' : 'disable';
    if (!password) {
      return fail(422, { message: '请输入密码' } satisfies MeActionData);
    }
    try {
      const result = await authedPost(
        cookies,
        '/api/v1/auth/re-auth',
        { password },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { mfa: { kind: 'reauth-done', intent } } satisfies MeActionData;
      }
      return fail(result.status, {
        message: result.message,
        requestId: result.requestId
      } satisfies MeActionData);
    } catch {
      return fail(503, { message: '重认证服务暂不可用，请稍后重试' } satisfies MeActionData);
    }
  }
};
