// M18-MFA-01：独立两步验证页（对齐原型 #mfa 路由）。
// 复用后端既有 MFA 端点：enroll / confirm / cancel / recovery-codes / disable。
import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedDelete, authedPost, getAuthed, SESSION_COOKIE } from '$lib/api/server';
import { otpauthQrDataUrl } from '$lib/mfa/otpauth-qr';
import type { User } from '$lib/api/types';

export type MfaStep =
  | { kind: 'enroll-challenge'; otpauth_uri: string; secret_base32: string; qr_data_url: string | null }
  | { kind: 'enroll-confirmed' }
  | { kind: 'recovery-codes'; codes: string[] }
  | { kind: 'disabled' };

export interface MfaPageData {
  user: User | null;
  error: string | null;
}

export interface MfaActionData {
  mfa?: MfaStep;
  message?: string;
  requestId?: string | null;
}

export const load: PageServerLoad = async ({ cookies, request }) => {
  const session = cookies.get(SESSION_COOKIE);
  // login action 只识别 ?next=（returnTo 为无效参数，此前跳登录后无法回跳）。
  if (!session) throw redirect(303, `/login?next=${encodeURIComponent('/mfa')}`);

  const requestId = request.headers.get('x-request-id');
  const meResult = await getAuthed<User>(cookies, '/api/v1/me', requestId);
  if (!meResult.ok) {
    if (meResult.status === 401) throw redirect(303, `/login?next=${encodeURIComponent('/mfa')}`);
    return { user: null, error: meResult.message } satisfies MfaPageData;
  }
  return { user: meResult.data, error: null } satisfies MfaPageData;
};

export const actions: Actions = {
  enroll: async ({ request, cookies }) => {
    try {
      const result = await authedPost<{ otpauth_uri: string; secret_base32: string }>(
        cookies,
        '/api/v1/auth/mfa/enroll',
        undefined,
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        // M18-MFA-01：注册二维码由服务端从 otpauth_uri 生成（SVG data URL，
        // 页面 <img> 渲染，SSR/无 JS 可直接扫码）；失败 null → 页面手工录入降级。
        const qrDataUrl = await otpauthQrDataUrl(result.data.otpauth_uri);
        return {
          mfa: {
            kind: 'enroll-challenge',
            otpauth_uri: result.data.otpauth_uri,
            secret_base32: result.data.secret_base32,
            qr_data_url: qrDataUrl
          }
        } satisfies MfaActionData;
      }
      return fail(result.status, {
        message: result.message,
        requestId: result.requestId
      } satisfies MfaActionData);
    } catch {
      return fail(503, { message: '两步验证服务暂不可用，请稍后重试' } satisfies MfaActionData);
    }
  },
  confirm: async ({ request, cookies }) => {
    const form = await request.formData();
    const code = String(form.get('code') ?? '').trim();
    if (!/^[0-9]{6}$/.test(code)) {
      return fail(422, { message: '请输入 6 位验证码' } satisfies MfaActionData);
    }
    try {
      const result = await authedPost(
        cookies,
        '/api/v1/auth/mfa/confirm',
        { code },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { mfa: { kind: 'enroll-confirmed' } } satisfies MfaActionData;
      }
      return fail(result.status, {
        message: result.message,
        requestId: result.requestId
      } satisfies MfaActionData);
    } catch {
      return fail(503, { message: '两步验证服务暂不可用，请稍后重试' } satisfies MfaActionData);
    }
  },
  cancel: async ({ request, cookies }) => {
    try {
      const result = await authedDelete(
        cookies,
        '/api/v1/auth/mfa/enrollment',
        request.headers.get('x-request-id')
      );
      if (result.ok) return { mfa: { kind: 'disabled' } } satisfies MfaActionData;
      return fail(result.status, {
        message: result.message,
        requestId: result.requestId
      } satisfies MfaActionData);
    } catch {
      return fail(503, { message: '两步验证服务暂不可用，请稍后重试' } satisfies MfaActionData);
    }
  },
  recovery: async ({ request, cookies }) => {
    try {
      const result = await authedPost<{ codes: string[] }>(
        cookies,
        '/api/v1/auth/mfa/recovery-codes',
        undefined,
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { mfa: { kind: 'recovery-codes', codes: result.data.codes } } satisfies MfaActionData;
      }
      return fail(result.status, {
        message: result.message,
        requestId: result.requestId
      } satisfies MfaActionData);
    } catch {
      return fail(503, { message: '两步验证服务暂不可用，请稍后重试' } satisfies MfaActionData);
    }
  },
  disable: async ({ request, cookies }) => {
    try {
      const result = await authedDelete(
        cookies,
        '/api/v1/auth/mfa',
        request.headers.get('x-request-id')
      );
      if (result.ok) return { mfa: { kind: 'disabled' } } satisfies MfaActionData;
      return fail(result.status, {
        message: result.message,
        requestId: result.requestId
      } satisfies MfaActionData);
    } catch {
      return fail(503, { message: '停用两步验证失败，请稍后重试' } satisfies MfaActionData);
    }
  }
};
