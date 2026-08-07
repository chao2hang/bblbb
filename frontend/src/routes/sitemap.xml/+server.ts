// M08-FEEDS/M08-UI-05：sitemap（/sitemap.xml）。
//
// 代理到后端 GET /api/v1/sitemap.xml（M08-FEEDS-03：只列入允许索引的公开
// canonical URL，支持分片）。后端未就绪/故障 → 502（不缓存受限内容）。
import { env } from '$env/dynamic/private';

const INTERNAL_API_ORIGIN: string = env.INTERNAL_API_ORIGIN ?? 'http://127.0.0.1:8080';

export async function GET({ request, url }): Promise<Response> {
  const requestId = request.headers.get('x-request-id');
  const page = url.searchParams.get('page');
  const headers: Record<string, string> = { Accept: 'application/xml' };
  if (requestId) headers['X-Request-ID'] = requestId;
  const suffix = page ? `?page=${encodeURIComponent(page)}` : '';
  try {
    const response = await fetch(`${INTERNAL_API_ORIGIN}/api/v1/sitemap.xml${suffix}`, { headers });
    if (!response.ok) {
      return new Response('站点地图暂不可用', { status: 502, headers: { 'Content-Type': 'text/plain; charset=utf-8' } });
    }
    const body = await response.text();
    return new Response(body, {
      headers: {
        'Content-Type': 'application/xml; charset=utf-8',
        'Cache-Control': response.headers.get('Cache-Control') ?? 'public, max-age=3600'
      }
    });
  } catch {
    return new Response('站点地图暂不可用', { status: 502, headers: { 'Content-Type': 'text/plain; charset=utf-8' } });
  }
}
