// M18-MFA-01：独立两步验证页（对齐原型 #mfa 路由）。
// 复用后端既有 MFA 端点：enroll / confirm / cancel / recovery-codes / disable。
// M02-MFA-PK：Passkey 与 TOTP 共存（登录第二步任一通过即可）——本页提供
// Passkey 列表 / 注册（经 /mfa/passkey/begin+confirm 端点，浏览器凭据在
// 客户端 JS 生成）/ 撤销（step-up 与停用 TOTP 同级）。
import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedDelete, authedPost, getAuthed, SESSION_COOKIE } from '$lib/api/server';
import { otpauthQrDataUrl } from '$lib/mfa/otpauth-qr';
import type { PasskeyInfo } from '$lib/mfa/passkey-types';
import type { User } from '$lib/api/types';

export type MfaStep =
  | { kind: 'enroll-challenge'; otpauth_uri: string; secret_base32: string; qr_data_url: string | null }
  | { kind: 'enroll-confirmed' }
  | { kind: 'recovery-codes'; codes: string[] }
  | { kind: 'disabled' };

export interface MfaPageData {
  user: User | null;
  error: string | null;
  /** 服务端是否配置了 Passkey（未配置时整块隐藏） */
  passkeyEnabled: boolean;
  passkeys: PasskeyInfo[];
  passkeysError: string | null;
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
    return { user: null, error: meResult.message, passkeyEnabled: false, passkeys: [], passkeysError: null } satisfies MfaPageData;
  }

  // Passkey 列表（M02-MFA-PK）：passkey_not_configured → 整块隐藏；
  // 其他错误不阻断 TOTP 设置（列表区显示错误提示）。
  let passkeyEnabled = false;
  let passkeys: PasskeyInfo[] = [];
  let passkeysError: string | null = null;
  const passkeyResult = await getAuthed<{ passkeys?: PasskeyInfo[] }>(
    cookies,
    '/api/v1/auth/passkeys',
    requestId
  );
  if (passkeyResult.ok) {
    passkeyEnabled = true;
    passkeys = passkeyResult.data.passkeys ?? [];
  } else if (passkeyResult.code !== 'passkey_not_configured') {
    passkeysError = passkeyResult.message;
    if (passkeyResult.status < 500) passkeyEnabled = true;
  }
  return {
    user: meResult.data,
    error: null,
    passkeyEnabled,
    passkeys,
    passkeysError
  } satisfies MfaPageData;
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
  },
  passkeyRevoke: async ({ request, cookies }) => {
    const form = await request.formData();
    const id = String(form.get('id') ?? '').trim();
    if (!id) {
      return fail(422, { message: '缺少 Passkey 标识' } satisfies MfaActionData);
    }
    try {
      const result = await authedDelete(
        cookies,
        `/api/v1/auth/passkeys/${encodeURIComponent(id)}`,
        request.headers.get('x-request-id')
      );
      // 成功：不返回 mfa 状态（避免误改 TOTP 启用态显示）；use:enhance 会
      // 自动 invalidateAll 重新拉取列表。
      if (result.ok) return {} satisfies MfaActionData;
      return fail(result.status, {
        message: result.message,
        requestId: result.requestId
      } satisfies MfaActionData);
    } catch {
      return fail(503, { message: '撤销 Passkey 失败，请稍后重试' } satisfies MfaActionData);
    }
  }
};
