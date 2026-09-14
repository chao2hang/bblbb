// M02-MFA-PK：Passkey 注册 confirm 代理端点（认证上下文）。
//
// 请求体：{ name?: string, credential: PublicKeyCredentialJSON }——credential
// 为浏览器原始凭据 JSON（base64url 字段），透传给后端 webauthn-rs 校验。

import { json } from '@sveltejs/kit';
import type { RequestHandler } from './$types';
import { authedPost } from '$lib/api/server';

export const POST: RequestHandler = async ({ request, cookies }) => {
  const body = (await request.json().catch(() => null)) as
    | { name?: string; credential?: unknown }
    | null;
  if (!body || !body.credential || typeof body.credential !== 'object') {
    return json({ message: '凭据数据无效' }, { status: 422 });
  }
  const result = await authedPost<{ passkey?: unknown }>(
    cookies,
    '/api/v1/auth/passkeys/confirm',
    { name: body.name, credential: body.credential },
    request.headers.get('x-request-id')
  );
  if (!result.ok) {
    return json({ message: result.message, code: result.code }, { status: result.status });
  }
  return json(result.data ?? { ok: true });
};
