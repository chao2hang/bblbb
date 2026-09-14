// M02-UX-03：登录页服务端表单 action（两步登录 MFA）
//
// - `login`（default）：密码步——代理 POST /api/v1/auth/login；启用 TOTP
//   的账号返回 MFA challenge（不签发会话，前端进入第二步）；否则签发会话
//   并 redirect 到 `?next=` 或 `/`；
// - `mfa`：第二步——challenge + TOTP code 或恢复码，代理
//   POST /api/v1/auth/login/mfa，成功签发会话并 redirect。
//
// 均走预认证 CSRF 配对 + Set-Cookie 逐属性复制（$lib/api/server.ts，
// FRONTEND.md：form action 代理登录必须显式复制所有 Set-Cookie 属性）。

import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions } from './$types';
import { loginMfaViaServer, loginViaServer } from '$lib/api/server';

export interface LoginActionData {
  mfa_required?: boolean;
  challenge_token?: string;
  /** 该账号注册过 Passkey：第二步展示「使用 Passkey 登录」（M02-MFA-PK） */
  passkey_available?: boolean;
  message?: string;
  requestId?: string | null;
}

// `next` 优先取表单隐藏字段：表单 action="?/login" 会替换整个查询串，
// POST 时 URL 上的 ?next= 丢失（隐藏 input 由页面从当前 URL 读出并回填）。
// 校验保持开放重定向安全：仅本站相对路径（/ 开头且非 //）。
function nextUrl(url: URL, formData?: FormData): string {
  const fromForm = formData ? String(formData.get('next') ?? '').trim() : '';
  const next = fromForm || url.searchParams.get('next') || '';
  return next.startsWith('/') && !next.startsWith('//') ? next : '/';
}

export const actions: Actions = {
  login: async ({ request, cookies, url }) => {
    const form = await request.formData();
    const identifier = String(form.get('identifier') ?? '').trim();
    const password = String(form.get('password') ?? '');
    const remember = form.get('remember') === 'on';
    if (!identifier || !password) {
      return fail(422, { message: '请输入用户名/邮箱和密码' } satisfies LoginActionData);
    }
    try {
      const result = await loginViaServer(
        cookies,
        { identifier, password, remember },
        request.headers.get('x-request-id')
      );
      if (result.kind === 'ok') throw redirect(303, nextUrl(url, form));
      if (result.kind === 'mfa') {
        return {
          mfa_required: true,
          challenge_token: result.challengeToken,
          passkey_available: result.passkeyAvailable
        } satisfies LoginActionData;
      }
      return fail(result.status, {
        message: result.message,
        requestId: result.requestId
      } satisfies LoginActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '登录服务暂时不可用，请稍后重试' } satisfies LoginActionData);
    }
  },
  mfa: async ({ request, cookies, url }) => {
    const form = await request.formData();
    const challengeToken = String(form.get('challenge_token') ?? '').trim();
    const totpCode = String(form.get('totp_code') ?? '').trim();
    const recoveryCode = String(form.get('recovery_code') ?? '').trim();
    // Passkey 断言：客户端 JS 把浏览器凭据 JSON 写入隐藏字段（M02-MFA-PK）
    const passkeyRaw = String(form.get('passkey_assertion') ?? '').trim();
    let passkeyAssertion: unknown;
    if (passkeyRaw) {
      try {
        passkeyAssertion = JSON.parse(passkeyRaw);
      } catch {
        return fail(422, {
          mfa_required: true,
          challenge_token: challengeToken,
          message: 'Passkey 凭据无效，请重试或改用验证码'
        } satisfies LoginActionData);
      }
    }
    if (!challengeToken) {
      return fail(422, { message: '登录状态已失效，请重新登录' } satisfies LoginActionData);
    }
    const methodCount = [totpCode, recoveryCode, passkeyRaw].filter((v) => v).length;
    if (methodCount !== 1) {
      return fail(422, {
        mfa_required: true,
        challenge_token: challengeToken,
        passkey_available: form.get('passkey_available') === '1',
        message: '验证码、恢复码或 Passkey 请任选一种'
      } satisfies LoginActionData);
    }
    try {
      const result = await loginMfaViaServer(
        cookies,
        {
          challenge_token: challengeToken,
          totp_code: totpCode || undefined,
          recovery_code: recoveryCode || undefined,
          passkey: passkeyAssertion
        },
        request.headers.get('x-request-id')
      );
      if (result.ok) throw redirect(303, nextUrl(url, form));
      return fail(result.status, {
        mfa_required: true,
        challenge_token: challengeToken,
        passkey_available: form.get('passkey_available') === '1',
        message: result.message,
        requestId: result.requestId
      } satisfies LoginActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, {
        mfa_required: true,
        challenge_token: challengeToken,
        passkey_available: form.get('passkey_available') === '1',
        message: '登录服务暂时不可用，请稍后重试'
      } satisfies LoginActionData);
    }
  }
};
