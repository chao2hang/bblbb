// M02-UX-04：重置密码页服务端表单 action
//
// 从邮件链接携带 ?token= 进入；提交新密码 → 代理 POST
// /api/v1/auth/password-reset/confirm。后端单事务：原子消费 30 分钟
// 一次性 token → 更新密码哈希 → 撤销该用户全部 Session（M02-IDENTITY-10），
// 因此成功页提示“其他设备上的会话已撤销”。无效/已消费/过期统一 400。
// 密码规则与注册一致（$lib/validation：8-128 且含字母和数字），后端仍
// 权威复检（8-256）。

import { fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { confirmPasswordResetViaServer } from '$lib/api/server';
import { validateNewPassword } from '$lib/validation';

export interface PasswordResetConfirmData {
  ok?: boolean;
  message?: string;
  requestId?: string | null;
  fieldErrors?: Partial<Record<'password' | 'confirm', string>>;
}

export const load: PageServerLoad = ({ url }) => {
  return { token: url.searchParams.get('token') ?? null };
};

export const actions: Actions = {
  default: async ({ request, cookies }) => {
    const form = await request.formData();
    const token = String(form.get('token') ?? '').trim();
    const password = String(form.get('password') ?? '');
    const confirm = String(form.get('confirm') ?? '');

    if (!token) {
      return fail(422, { message: '重置链接缺少 token，请从邮件中的完整链接进入' } satisfies PasswordResetConfirmData);
    }
    const fieldErrors = validateNewPassword(password, confirm);
    if (Object.keys(fieldErrors).length > 0) {
      return fail(422, {
        message: '请修正表单中标红的字段',
        fieldErrors
      } satisfies PasswordResetConfirmData);
    }

    try {
      const result = await confirmPasswordResetViaServer(
        cookies,
        token,
        password,
        request.headers.get('x-request-id')
      );
      if (result.ok) return { ok: true } satisfies PasswordResetConfirmData;
      return fail(result.status, {
        message: result.message,
        requestId: result.requestId
      } satisfies PasswordResetConfirmData);
    } catch {
      return fail(503, { message: '重置服务暂时不可用，请稍后重试' } satisfies PasswordResetConfirmData);
    }
  }
};
