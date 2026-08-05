// M02-UX-02：邮箱验证页服务端表单 action
//
// - `verify`：提交一次性验证 token → 代理 POST /api/v1/auth/verify-email；
// - `resend`：提交邮箱 → 代理 POST /api/v1/auth/resend-verification
//   （统一 202 不泄漏；429 返回 retryAfterSecs 供冷却倒计时）。
// 均为预认证写路径（x-csrf-context: preauth），由 $lib/api/server.ts
// 完成 CSRF 配对 + Cookie/X-Request-ID 转发 + Set-Cookie 复制。

import { fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { RESEND_COOLDOWN_SECS, resendVerificationViaServer, verifyEmailViaServer } from '$lib/api/server';

export interface VerifyEmailActionData {
  ok?: boolean;
  sent?: boolean;
  message?: string;
  requestId?: string | null;
  cooldown?: number;
  email?: string;
}

export const load: PageServerLoad = ({ url }) => {
  return { token: url.searchParams.get('token') ?? null };
};

export const actions: Actions = {
  verify: async ({ request, cookies }) => {
    const form = await request.formData();
    const token = String(form.get('token') ?? '').trim();
    if (!token) {
      return fail(422, { message: '验证链接缺少 token，请从邮件中的完整链接进入' } satisfies VerifyEmailActionData);
    }
    try {
      const result = await verifyEmailViaServer(cookies, token, request.headers.get('x-request-id'));
      if (result.ok) return { ok: true } satisfies VerifyEmailActionData;
      return fail(result.status, {
        message: result.message,
        requestId: result.requestId
      } satisfies VerifyEmailActionData);
    } catch {
      return fail(503, { message: '验证服务暂时不可用，请稍后重试' } satisfies VerifyEmailActionData);
    }
  },
  resend: async ({ request, cookies }) => {
    const form = await request.formData();
    const email = String(form.get('email') ?? '').trim();
    if (!email) {
      return fail(422, { message: '请输入注册时使用的邮箱' } satisfies VerifyEmailActionData);
    }
    try {
      const result = await resendVerificationViaServer(
        cookies,
        email,
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { sent: true, cooldown: RESEND_COOLDOWN_SECS, email } satisfies VerifyEmailActionData;
      }
      return fail(result.status, {
        message: result.message,
        requestId: result.requestId,
        cooldown: result.retryAfterSecs ?? undefined,
        email
      } satisfies VerifyEmailActionData);
    } catch {
      return fail(503, { message: '重发服务暂时不可用，请稍后重试' } satisfies VerifyEmailActionData);
    }
  }
};
