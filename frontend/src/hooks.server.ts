import type { HandleFetch } from '@sveltejs/kit';
import { env } from '$env/dynamic/private';

// docs/FRONTEND.md §2: SSR 服务端代码访问 Rust 后端的统一内部基址
const INTERNAL_API_ORIGIN: string = env.INTERNAL_API_ORIGIN ?? 'http://127.0.0.1:8080';

/**
 * SSR 阶段 internal fetch 拦截重定向（M00-FRONTEND-08 / GAP-FIX）：
 * 1. 将前端内部调用（/api/*、/healthz、/readyz）直接重定向到 INTERNAL_API_ORIGIN，
 *    避免在 HTTPS 开发模式下通过本地回环请求自签名证书失败（DEPTH_ZERO_SELF_SIGNED_CERT）；
 * 2. 自动透传客户端 Cookie 与 X-Request-ID 到后端。
 */
export const handleFetch: HandleFetch = async ({ request, fetch, event }) => {
  const url = new URL(request.url);
  if (
    url.pathname.startsWith('/api/') ||
    url.pathname.startsWith('/healthz') ||
    url.pathname.startsWith('/readyz')
  ) {
    const targetUrl = `${INTERNAL_API_ORIGIN}${url.pathname}${url.search}`;
    const headers = new Headers(request.headers);
    const cookie = event.request.headers.get('cookie');
    if (cookie && !headers.has('cookie')) {
      headers.set('cookie', cookie);
    }
    const requestId = event.request.headers.get('x-request-id');
    if (requestId && !headers.has('x-request-id')) {
      headers.set('x-request-id', requestId);
    }
    const rewritten = new Request(targetUrl, {
      method: request.method,
      headers,
      body: request.body,
      // @ts-expect-error duplex required by node fetch when body is stream
      duplex: request.body ? 'half' : undefined
    });
    return fetch(rewritten);
  }
  return fetch(request);
};
