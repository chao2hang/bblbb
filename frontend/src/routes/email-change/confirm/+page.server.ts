// GA 邮箱换绑确认页服务端 action
//
// - `confirm`：提交一次性换绑 token → 代理 POST /api/v1/auth/email-change/confirm；
//   422 token 无效/过期/已消费；409 新邮箱已被占用（确认前被他人绑定）。
// 预认证写路径（x-csrf-context: preauth），由 $lib/api/server.ts 完成
// CSRF 配对 + Cookie/X-Request-ID 转发。

import { fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { confirmEmailChangeViaServer } from '$lib/api/server';

export interface EmailChangeConfirmActionData {
  ok?: boolean;
  message?: string;
  requestId?: string | null;
}

export const load: PageServerLoad = ({ url }) => {
  return { token: url.searchParams.get('token') ?? null };
};

export const actions: Actions = {
  confirm: async ({ request, cookies }) => {
    const form = await request.formData();
    const token = String(form.get('token') ?? '').trim();
    if (!token) {
      return fail(422, {
        confirm: { ok: false, message: '缺少换绑令牌，请从邮件中的完整链接进入本页' }
      } satisfies { confirm: EmailChangeConfirmActionData });
    }
    const result = await confirmEmailChangeViaServer(
      cookies,
      token,
      request.headers.get('x-request-id')
    );
    if (result.ok) {
      return {
        confirm: { ok: true, message: '邮箱换绑成功！新邮箱已完成验证，现在是你的登录邮箱。' }
      } satisfies { confirm: EmailChangeConfirmActionData };
    }
    return fail(result.status, {
      confirm: {
        ok: false,
        message:
          result.status === 409
            ? '该邮箱已被其他账号使用，换绑未完成'
            : result.status === 422
              ? '换绑链接无效或已过期，请在设置页重新申请换绑'
              : result.message || '换绑失败，请稍后重试',
        requestId: result.requestId
      }
    } satisfies { confirm: EmailChangeConfirmActionData });
  }
};
