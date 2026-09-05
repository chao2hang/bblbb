// M08-FEEDS/M08-UI-05：Atom 订阅（/atom.xml）。
//
// 完全照 rss.xml/+server.ts 的代理模式：代理到后端 GET /api/v1/atom
// （M08-FEEDS-02：Atom 1.0 字段/链接/更新时间/XML escaping；ETag/缓存策略
// 由后端裁决）。后端未就绪/故障 → 502 并带简短说明（不返回缓存的受限
// 内容；feeds 只包含安全公开投影）。
import { env } from '$env/dynamic/private';

const INTERNAL_API_ORIGIN: string = env.INTERNAL_API_ORIGIN ?? 'http://127.0.0.1:8080';

export async function GET({ request }): Promise<Response> {
  const requestId = request.headers.get('x-request-id');
  const headers: Record<string, string> = { Accept: 'application/atom+xml' };
  if (requestId) headers['X-Request-ID'] = requestId;
  try {
    const response = await fetch(`${INTERNAL_API_ORIGIN}/api/v1/atom`, { headers });
    if (!response.ok) {
      return new Response('订阅服务暂不可用', { status: 502, headers: { 'Content-Type': 'text/plain; charset=utf-8' } });
    }
    const body = await response.text();
    return new Response(body, {
      headers: {
        'Content-Type': 'application/atom+xml; charset=utf-8',
        'Cache-Control': response.headers.get('Cache-Control') ?? 'public, max-age=300'
      }
    });
  } catch {
    return new Response('订阅服务暂不可用', { status: 502, headers: { 'Content-Type': 'text/plain; charset=utf-8' } });
  }
}
