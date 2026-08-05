// M02-UX-01：注册页服务端表单 action
//
// 浏览器（含无 JS）把注册表单 POST 到本 action → 代理到 Rust
// /api/v1/auth/register（预认证 CSRF 配对 + Cookie/X-Request-ID 转发 +
// Set-Cookie 复制，见 $lib/api/server.ts，遵循 docs/FRONTEND.md）。
//
// 字段校验在 action 内完成（与后端规则一致，$lib/validation.ts；后端仍
// 权威复检）。后端对“用户名/邮箱已存在”返回与成功一致的 201 {ok:true}
// （防枚举，M02-IDENTITY-05），因此冲突时前端同样只显示成功——统一
// 账号冲突提示，不泄漏账号是否已存在。

import { fail } from '@sveltejs/kit';
import type { Actions } from './$types';
import { registerViaServer } from '$lib/api/server';
import { validateRegistration } from '$lib/validation';

export interface RegisterActionData {
  ok?: boolean;
  message?: string;
  requestId?: string | null;
  fieldErrors?: Partial<Record<'username' | 'email' | 'password' | 'confirm', string>>;
  values?: { username: string; email: string };
}

export const actions: Actions = {
  default: async ({ request, cookies }) => {
    const form = await request.formData();
    const username = String(form.get('username') ?? '').trim();
    const email = String(form.get('email') ?? '').trim();
    const password = String(form.get('password') ?? '');
    const confirm = String(form.get('confirm') ?? '');

    const fieldErrors = validateRegistration({ username, email, password, confirm });
    if (Object.keys(fieldErrors).length > 0) {
      return fail(422, {
        message: '请修正表单中标红的字段',
        fieldErrors,
        values: { username, email }
      } satisfies RegisterActionData);
    }

    try {
      const result = await registerViaServer(
        cookies,
        { username, email, password },
        request.headers.get('x-request-id')
      );
      if (result.ok) return { ok: true } satisfies RegisterActionData;
      return fail(result.status, {
        message: result.message,
        requestId: result.requestId,
        values: { username, email }
      } satisfies RegisterActionData);
    } catch {
      return fail(503, {
        message: '注册服务暂时不可用，请稍后重试'
      } satisfies RegisterActionData);
    }
  }
};
