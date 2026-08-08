// M14-SEO-04：公开页面 SEO 行为浏览器/HTTP 级测试。
//
// - SSR 源：meta/canonical/OG/Twitter/JSON-LD 直接出现在 SSR HTML；
// - hydration/预取：__data.json（hydration payload）可访问且含公开投影；
// - 304：ETag + If-None-Match 命中返回 304；
// - 缓存键：根 layout 统一 Cache-Control: private, no-store（会话化页面
//   不得进入共享缓存，M00-FRONTEND-06）；
// - 公开图片 URL：og:image/canonical 只接受绝对 http(s) URL（meta.ts 白名单，
//   由 vitest meta.test.ts 覆盖），这里验证无非法 URL 属性输出。
import { expect, test } from '@playwright/test';

test.describe('SSR 源（meta/canonical/OG/Twitter/JSON-LD）', () => {
  test('主页输出 title/description/canonical/OG/Twitter/JSON-LD', async ({ page }) => {
    const response = await page.goto('/');
    const html = await response!.text();
    expect(html).toContain('<title>社区论坛 — BBLBB</title>');
    expect(html).toMatch(/<meta name="description"/);
    expect(html).toMatch(/<link rel="canonical" href="http:\/\/localhost:4173\/"/);
    expect(html).toContain('property="og:type"');
    expect(html).toContain('name="twitter:card"');
    expect(html).toContain('application/ld+json');
  });

  test('帖子详情输出 Article JSON-LD 与 canonical（公开已发布帖）', async ({ page }) => {
    await page.goto('/boards/general');
    const link = page.locator('a[href^="/posts/"]').first();
    await link.waitFor({ state: 'visible', timeout: 10_000 });
    const href = await link.getAttribute('href');
    const response = await page.goto(href!);
    const html = await response!.text();
    expect(html).toMatch(/<title>[^<]+ — BBLBB<\/title>/);
    expect(html).toMatch(/<link rel="canonical" href="http:\/\/localhost:4173\/posts\//);
    expect(html).toContain('ld+json');
  });

  test('搜索页输出 noindex + canonical（默认不收录）', async ({ page }) => {
    const response = await page.goto('/search?q=Rust');
    const html = await response!.text();
    expect(html).toContain('name="robots" content="noindex, noarchive, nofollow"');
    expect(html).toContain('rel="canonical"');
  });

  test('404 错误页输出 noindex（隐藏内容统一策略）', async ({ page }) => {
    const response = await page.goto('/posts/does-not-exist');
    expect(response?.status()).toBe(404);
    const html = await response!.text();
    expect(html).toContain('noindex');
  });

  test('JSON-LD 不含未转义的 `</script`（注入防护）', async ({ page }) => {
    const response = await page.goto('/');
    const html = await response!.text();
    const ldBlocks = html.match(/<script type="application\/ld\+json">([\s\S]*?)<\/script>/g) ?? [];
    expect(ldBlocks.length).toBeGreaterThan(0);
    for (const block of ldBlocks) {
      // 任何字符串字段都不能以未转义 `</script>` 提前闭合（注入）。
      expect(block).not.toContain('</script><script');
      expect(block).not.toMatch(/<\\\/script>\s*<script/);
    }
  });
});

test.describe('hydration/预取（__data.json）', () => {
  test('页面 hydration payload 可访问且只含公开投影', async ({ page }) => {
    const response = await page.goto('/users/alice');
    const dataResponse = await page.request.get('/users/alice/__data.json');
    expect(dataResponse.status()).toBe(200);
    const data = (await dataResponse.json()) as { type: 'data'; nodes: Array<{ data?: unknown }> };
    const serialized = JSON.stringify(data);
    // 公开投影不含凭据字段。
    expect(serialized).not.toContain('password_hash');
    expect(serialized).not.toContain('email_normalized');
    void response;
  });
});

test.describe('304 与缓存键（ETag / Cache-Control）', () => {
  test('带 If-None-Match 的请求命中 304', async ({ page }) => {
    const first = await page.goto('/');
    const etag = first!.headers()['etag'];
    expect(etag).toBeTruthy();
    // 用页面内 fetch 走真实网络（cache: no-store 绕过 APIRequestContext 的
    // HTTP 缓存，否则条件请求会被 Playwright 缓存层折叠）。
    const status = await page.evaluate(async (etag) => {
      const r = await fetch('/', { cache: 'no-store', headers: { 'If-None-Match': etag } });
      return r.status;
    }, etag);
    expect(status).toBe(304);
  });

  test('根 layout 统一 Cache-Control: private, no-store（会话化页面不入共享缓存）', async ({ page }) => {
    for (const path of ['/', '/boards/tech', '/login']) {
      const response = await page.request.get(path);
      const cacheControl = response.headers()['cache-control'] ?? '';
      expect(cacheControl, `${path} 必须 no-store`).toContain('no-store');
    }
  });
});

test.describe('公开图片 URL 与 meta 属性安全', () => {
  test('meta 属性值无非法 URL 注入（javascript:/data:）', async ({ page }) => {
    const response = await page.goto('/');
    const html = await response!.text();
    const metaAttrs = html.match(/<meta[^>]+content="[^"]*"[^>]*>/g) ?? [];
    for (const meta of metaAttrs) {
      expect(meta.toLowerCase()).not.toMatch(/content="(javascript|data|vbscript):/);
    }
  });

  test('canonical/og:url 为绝对 http(s) URL', async ({ page }) => {
    for (const path of ['/', '/boards', '/tags']) {
      const response = await page.goto(path);
      const html = await response!.text();
      const canonical = html.match(/<link rel="canonical" href="([^"]+)"/);
      expect(canonical, `${path} 应有 canonical`).not.toBeNull();
      expect(canonical![1]).toMatch(/^https?:\/\//);
      expect(canonical![1]).not.toMatch(/^(javascript|data|vbscript):/);
    }
  });
});
