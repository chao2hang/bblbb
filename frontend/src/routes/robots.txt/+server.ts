// M08-FEEDS-04/M08-UI-04：robots.txt（/robots.txt）。
//
// 代理到后端 GET /robots.txt（动态生成：AI 训练爬虫默认拒绝、普通搜索引擎
// 只允许明确公开内容、按配置输出 Disallow）。robots 是协作性声明而非安全
// 边界——服务端授权/限流才是真正防线（CRAWLER-POLICY.md）。后端未就绪 →
// 502（不返回会误导抓取器的过期内容）。
import { env } from '$env/dynamic/private';

const INTERNAL_API_ORIGIN: string = env.INTERNAL_API_ORIGIN ?? 'http://127.0.0.1:8080';

export async function GET({ request }): Promise<Response> {
  const requestId = request.headers.get('x-request-id');
  const headers: Record<string, string> = { Accept: 'text/plain' };
  if (requestId) headers['X-Request-ID'] = requestId;
  try {
    const response = await fetch(`${INTERNAL_API_ORIGIN}/robots.txt`, { headers });
    if (!response.ok) {
      return new Response('robots.txt 暂不可用', { status: 502, headers: { 'Content-Type': 'text/plain; charset=utf-8' } });
    }
    const body = await response.text();
    return new Response(body, {
      headers: {
        'Content-Type': 'text/plain; charset=utf-8',
        'Cache-Control': response.headers.get('Cache-Control') ?? 'public, max-age=600'
      }
    });
  } catch {
    return new Response('robots.txt 暂不可用', { status: 502, headers: { 'Content-Type': 'text/plain; charset=utf-8' } });
  }
}
