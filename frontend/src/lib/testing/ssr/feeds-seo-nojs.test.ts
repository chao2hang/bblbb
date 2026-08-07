// M08-UI-05：Feed/SEO 页面源测试——/rss.xml、/sitemap.xml、/robots.txt 代理。
//
// - 成功：转发后端内容与 content-type，X-Request-ID 透传；
// - 后端故障/未就绪：502 降级（不返回受限内容、不误导抓取器）；
// - robots 只做代理（声明层由后端动态生成；不承诺阻止恶意抓取）。
import { afterEach, describe, expect, it, vi } from 'vitest';
import { GET as rssGet } from '../../../routes/rss.xml/+server';
import { GET as sitemapGet } from '../../../routes/sitemap.xml/+server';
import { GET as robotsGet } from '../../../routes/robots.txt/+server';

afterEach(() => {
  vi.unstubAllGlobals();
});

type AnyFeedEvent = Parameters<typeof rssGet>[0] &
  Parameters<typeof sitemapGet>[0] &
  Parameters<typeof robotsGet>[0];

function event(path: string, requestId: string | null = null): AnyFeedEvent {
  const headers = new Headers();
  if (requestId) headers.set('x-request-id', requestId);
  return {
    request: new Request(`http://test.local${path}`, { headers }),
    url: new URL(`http://test.local${path}`)
  } as unknown as AnyFeedEvent;
}

describe('M08-UI-05 RSS 订阅页面源', () => {
  it('代理后端 /api/v1/rss，透传 content-type 与 X-Request-ID', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response('<?xml version="1.0"?><rss version="2.0"><channel><title>BBLBB</title></channel></rss>', {
        status: 200,
        headers: { 'Content-Type': 'application/rss+xml; charset=utf-8' }
      })
    );
    vi.stubGlobal('fetch', fetchMock);
    const res = await rssGet(event('/rss.xml', 'req-1'));
    expect(res.status).toBe(200);
    expect(res.headers.get('Content-Type')).toContain('application/rss+xml');
    const body = await res.text();
    expect(body).toContain('<rss');
    expect(body).toContain('BBLBB');
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toContain('/api/v1/rss');
    expect((init.headers as Record<string, string>)['X-Request-ID']).toBe('req-1');
  });

  it('后端故障 → 502 降级（不返回受限内容）', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('boom', { status: 500 })));
    const res = await rssGet(event('/rss.xml'));
    expect(res.status).toBe(502);
    expect(await res.text()).toContain('暂不可用');
  });
});

describe('M08-UI-05 sitemap 页面源', () => {
  it('代理后端 /api/v1/sitemap.xml（含分片参数透传）', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response('<?xml version="1.0"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9"></urlset>', {
        status: 200,
        headers: { 'Content-Type': 'application/xml' }
      })
    );
    vi.stubGlobal('fetch', fetchMock);
    const res = await sitemapGet(event('/sitemap.xml?page=2'));
    expect(res.status).toBe(200);
    expect(res.headers.get('Content-Type')).toContain('application/xml');
    expect(await res.text()).toContain('<urlset');
    const [url] = fetchMock.mock.calls[0] as [string];
    expect(url).toContain('/api/v1/sitemap.xml');
    expect(url).toContain('page=2');
  });

  it('后端故障 → 502 降级', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('err', { status: 503 })));
    const res = await sitemapGet(event('/sitemap.xml'));
    expect(res.status).toBe(502);
  });
});

describe('M08-UI-04/05 robots.txt 页面源', () => {
  it('代理后端 /robots.txt（AI 爬虫默认拒绝由后端动态生成）', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response('User-agent: GPTBot\nDisallow: /\nUser-agent: *\nAllow: /', {
        status: 200,
        headers: { 'Content-Type': 'text/plain' }
      })
    );
    vi.stubGlobal('fetch', fetchMock);
    const res = await robotsGet(event('/robots.txt'));
    expect(res.status).toBe(200);
    expect(res.headers.get('Content-Type')).toContain('text/plain');
    const body = await res.text();
    expect(body).toContain('GPTBot');
    expect(body).toContain('Disallow: /');
    const [url] = fetchMock.mock.calls[0] as [string];
    expect(url).toContain('/robots.txt');
  });

  it('后端故障 → 502（不返回会误导抓取器的过期内容）', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('err', { status: 500 })));
    const res = await robotsGet(event('/robots.txt'));
    expect(res.status).toBe(502);
  });
});
