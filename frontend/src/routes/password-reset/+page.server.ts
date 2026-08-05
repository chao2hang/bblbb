// M02-UX-04：忘记密码页服务端表单 action
//
// 提交邮箱 → 代理 POST /api/v1/auth/password-reset。后端统一 202（邮箱
// 不存在/已删除与正常一致，不泄漏是否注册），冷却 60s / 日上限 3 次 /
// IP 每小时 5 次命中返回 429 + Retry-After。均走预认证 CSRF 配对
// （$lib/api/server.ts）。

import { fail } from '@sveltejs/kit';
import type { Actions } from './$types';
import { requestPasswordResetViaServer } from '$lib/api/server';

export interface PasswordResetRequestData {
  sent?: boolean;
  message?: string;
  requestId?: string | null;
  email?: string;
  cooldown?: number;
}

const EMAIL_PATTERN = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

export const actions: Actions = {
  default: async ({ request, cookies }) => {
    const form = await request.formData();
    const email = String(form.get('email') ?? '').trim();
    if (!email) {
      return fail(422, { message: '请输入注册时使用的邮箱' } satisfies PasswordResetRequestData);
    }
    if (!EMAIL_PATTERN.test(email) || email.length > 320) {
      return fail(422, { message: '邮箱格式不正确' } satisfies PasswordResetRequestData);
    }
    try {
      const result = await requestPasswordResetViaServer(
        cookies,
        email,
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { sent: true, email } satisfies PasswordResetRequestData;
      }
      return fail(result.status, {
        message: result.message,
        requestId: result.requestId,
        email,
        cooldown: result.retryAfterSecs ?? undefined
      } satisfies PasswordResetRequestData);
    } catch {
      return fail(503, { message: '找回密码服务暂时不可用，请稍后重试' } satisfies PasswordResetRequestData);
    }
  }
};
