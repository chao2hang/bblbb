// M02-MFA-PK：Passkey 注册 begin 代理端点（认证上下文）。
//
// 返回 WebAuthn creation options JSON，供页面 JS 调
// navigator.credentials.create()；随后凭据经 /mfa/passkey/confirm 提交。

import { json } from '@sveltejs/kit';
import type { RequestHandler } from './$types';
import { authedPost } from '$lib/api/server';

export const POST: RequestHandler = async ({ request, cookies }) => {
  const result = await authedPost<unknown>(
    cookies,
    '/api/v1/auth/passkeys',
    {},
    request.headers.get('x-request-id')
  );
  if (!result.ok) {
    return json({ message: result.message, code: result.code }, { status: result.status });
  }
  return json(result.data, {
    headers: { 'Cache-Control': 'private, no-store' }
  });
};
