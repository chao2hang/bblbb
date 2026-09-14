import type { HandleFetch } from '@sveltejs/kit';
import { env } from '$env/dynamic/private';

// docs/FRONTEND.md §2: SSR 服务端代码访问 Rust 后端的统一内部基址
const INTERNAL_API_ORIGIN: string = env.INTERNAL_API_ORIGIN ?? 'http://127.0.0.1:8080';

/**
 * SSR 阶段 internal fetch 拦截重定向（M00-FRONTEND-08 / GAP-FIX）：
 * 1. 将前端内部调用（/api/*、/healthz、/readyz）直接重定向到 INTERNAL_API_ORIGIN，
 *    避免在 HTTPS 开发模式下通过本地回环请求自签名证书失败（DEPTH_ZERO_SELF_SIGNED_CERT）；
 * 2. 自动透传客户端 Cookie 与 X-Request-ID 到后端。
 * 3. 剥离浏览器来源语义（origin/referer）：后端 M02-SESSION-09 会校验
 *    Origin（缺则 Referer）与请求 Host 同主机或命中 allowed_origins，
 *    而重定向后的 Host 是内部 API 主机（默认 127.0.0.1:8080），与前端
 *    站点 origin（如 http://localhost:5173）主机名不同，会导致所有经
 *    event.fetch 的 SSR 写请求（如装备成就 PUT /me/achievements/{code}/equip）
 *    400 origin_not_allowed。内部服务间调用与 lib/api/server.ts 的全局
 *    fetch 行为对齐：不带来源头，后端按「非浏览器客户端」放行；写安全
 *    仍由会话绑定的 X-CSRF-Token（M02-SESSION-07）保证。
 */
export const handleFetch: HandleFetch = async ({ request, fetch: kitFetch, event }) => {
  const url = new URL(request.url);
  if (
    url.pathname.startsWith('/api/') ||
    url.pathname.startsWith('/healthz') ||
    url.pathname.startsWith('/readyz')
  ) {
    const targetUrl = `${INTERNAL_API_ORIGIN}${url.pathname}${url.search}`;
    const headers = new Headers(request.headers);
    // SvelteKit event.fetch 会给非 GET 请求合成 Origin（= 前端站点 origin，
    // 仅 GET/HEAD 同源时移除）；无论上游是否已注入，统一剥离。
    headers.delete('origin');
    headers.delete('referer');
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
    // 必须用全局 fetch 发送：SvelteKit 注入的 kitFetch 在请求缺 origin 头时会
    // 重新注入前端 origin（@sveltejs/kit runtime/server/fetch.js），导致剥离失效。
    // 全局（undici）fetch 不会自动添加 origin/referer。
    return fetch(rewritten);
  }
  return kitFetch(request);
};
