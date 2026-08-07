// M08-FEEDS/M08-UI-05：RSS 订阅（/rss.xml）。
//
// 代理到后端 GET /api/v1/rss（M08-FEEDS-01：稳定 cursor/发布时间排序、
// ETag/缓存策略由后端裁决）。后端未就绪/故障 → 502 并带简短说明（不返回
// 缓存的受限内容；feeds 只包含安全公开投影）。
import { env } from '$env/dynamic/private';

const INTERNAL_API_ORIGIN: string = env.INTERNAL_API_ORIGIN ?? 'http://127.0.0.1:8080';

export async function GET({ request }): Promise<Response> {
  const requestId = request.headers.get('x-request-id');
  const headers: Record<string, string> = { Accept: 'application/rss+xml' };
  if (requestId) headers['X-Request-ID'] = requestId;
  try {
    const response = await fetch(`${INTERNAL_API_ORIGIN}/api/v1/rss`, { headers });
    if (!response.ok) {
      return new Response('订阅服务暂不可用', { status: 502, headers: { 'Content-Type': 'text/plain; charset=utf-8' } });
    }
    const body = await response.text();
    return new Response(body, {
      headers: {
        'Content-Type': 'application/rss+xml; charset=utf-8',
        'Cache-Control': response.headers.get('Cache-Control') ?? 'public, max-age=300'
      }
    });
  } catch {
    return new Response('订阅服务暂不可用', { status: 502, headers: { 'Content-Type': 'text/plain; charset=utf-8' } });
  }
}
