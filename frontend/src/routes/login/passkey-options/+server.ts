// M02-MFA-PK：登录第二步 Passkey request options 代理端点。
//
// 浏览器 JS 调 navigator.credentials.get() 前需要先拿到 WebAuthn options；
// options 获取需要预认证 CSRF 配对（与 login/login/mfa 同上下文），因此经
// 本服务端端点代理（$lib/api/server.ts 统一 Cookie/X-Request-ID 转发与
// Set-Cookie 复制），浏览器不直接触达内部后端。

import { json } from '@sveltejs/kit';
import type { RequestHandler } from './$types';
import { passkeyLoginOptionsViaServer } from '$lib/api/server';

export const POST: RequestHandler = async ({ request, cookies }) => {
  const body = (await request.json().catch(() => null)) as { challenge_token?: string } | null;
  const challengeToken = String(body?.challenge_token ?? '').trim();
  if (!challengeToken) {
    return json({ message: '登录状态已失效，请重新登录' }, { status: 422 });
  }
  const result = await passkeyLoginOptionsViaServer(
    cookies,
    challengeToken,
    request.headers.get('x-request-id')
  );
  if (!result.ok) {
    return json({ message: result.message, code: result.code }, { status: result.status });
  }
  return json(result.data, {
    headers: { 'Cache-Control': 'private, no-store' }
  });
};
