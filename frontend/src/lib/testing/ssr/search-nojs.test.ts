// M08-UI-05/06：公开搜索页 SSR 快照 + 无 JS 浏览基线。
//
// - head：canonical、noindex（搜索页默认不被收录）、OpenGraph、Twitter Card、
//   JSON-LD（WebSite + SearchAction）；
// - 表单为原生 GET → /search（无 JS 可提交）；
// - 结果只渲染后端安全投影（excerpt/highlight），对抗性隐藏正文/凭据不进入
//   SSR HTML；
// - 空状态、429/挑战（ChallengeGate）、分页链接（稳定 cursor）与引导态。
import { describe, expect, it, vi } from 'vitest';
import { render } from 'svelte/server';
import SearchPage from '../../../routes/search/+page.svelte';
import type { SearchPageData } from '../../../routes/search/+page.server';

vi.mock('$app/state', () => ({
  page: { url: new URL('http://test.local/search') }
}));

const baseData = (overrides: Partial<SearchPageData> = {}): SearchPageData => ({
  q: '',
  limit: 20,
  after: null,
  invalid: null,
  searched: false,
  results: [],
  nextCursor: null,
  hasMore: false,
  error: null,
  rateLimited: null,
  challenge: null,
  ...overrides
});

describe('M08-UI-05 搜索页 SSR head', () => {
  it('输出 canonical、noindex、OG/Twitter 与 JSON-LD（WebSite + SearchAction）', () => {
    const { body, head } = render(SearchPage, { props: { data: baseData({ q: '测试', searched: true }) } });
    expect(head).toContain('<link rel="canonical"');
    expect(head).toContain(`href="http://test.local/search?q=${encodeURIComponent('测试')}"`);
    expect(head).toContain('name="robots" content="noindex, noarchive, nofollow"');
    expect(head).toContain('property="og:type" content="website"');
    expect(head).toContain('property="og:title"');
    expect(head).toContain(`property="og:url" content="http://test.local/search?q=${encodeURIComponent('测试')}"`);
    expect(head).toContain('name="twitter:card" content="summary"');
    // JSON-LD 经 Seo 统一生成器输出：script 位于正文树（schema.org 允许
    // JSON-LD 出现在 DOM 任意位置；M14-SEO-01 统一 head + JSON-LD 注入）。
    expect(head + body).toContain('application/ld+json');
    expect(head + body).toContain('"@type":"SearchAction"');
    expect(head + body).toContain('urlTemplate');
    // 正文区不含 head 内容（独立输出）。
    expect(body).not.toContain('<link rel="canonical"');
  });

  it('未搜索 → canonical 指向 /search 且无结果区', () => {
    const { body, head } = render(SearchPage, { props: { data: baseData() } });
    expect(head).toContain('href="http://test.local/search"');
    expect(body).toContain('输入关键词搜索公开帖子');
  });
});

describe('M08-UI-01/02 搜索页 SSR 结果', () => {
  it('原生 GET 表单（无 JS 可提交）+ 结果仅安全投影字段', () => {
    const { body } = render(SearchPage, {
      props: {
        data: baseData({
          q: '测试',
          searched: true,
          results: [
            {
              id: 'p1',
              type: 'post',
              title: '搜索测试帖',
              url: '/posts/search-test',
              excerpt: '这是安全摘要',
              highlight: '安全高亮片段'
            }
          ]
        })
      }
    });
    expect(body).toMatch(/<form[^>]*method="get"[^>]*action="\/search"/);
    expect(body).toContain('name="q"');
    expect(body).toContain('搜索测试帖');
    expect(body).toContain('这是安全摘要');
    expect(body).toContain('安全高亮片段');
    expect(body).toContain('href="/posts/search-test"');
  });

  it('对抗性结果（隐藏正文/受限 HTML/凭据）不进入 SSR HTML', () => {
    const adversarial = {
      id: 'p2',
      type: 'post',
      title: '标题',
      url: '/posts/p2',
      excerpt: '摘要',
      body_html: '<div>隐藏正文 HIDDEN-BODY</div>',
      content: '完整正文 FULL-CONTENT',
      password_hash: 'SEARCH-SSR-HASH',
      session_token: 'SEARCH-SSR-TOKEN'
    } as unknown as SearchPageData['results'][number];
    const { body } = render(SearchPage, {
      props: { data: baseData({ q: 'x', searched: true, results: [adversarial] }) }
    });
    expect(body).not.toContain('HIDDEN-BODY');
    expect(body).not.toContain('FULL-CONTENT');
    expect(body).not.toContain('SEARCH-SSR-HASH');
    expect(body).not.toContain('SEARCH-SSR-TOKEN');
  });

  it('空状态文案', () => {
    const { body } = render(SearchPage, {
      props: { data: baseData({ q: '不存在', searched: true, results: [] }) }
    });
    expect(body).toContain('没有找到相关内容');
    expect(body).toContain('换个关键词试试');
  });

  it('分页：下一页（稳定 cursor）与返回第一页链接（无 JS 可点）', () => {
    const { body } = render(SearchPage, {
      props: {
        data: baseData({
          q: '测试',
          searched: true,
          after: 'cursor-0',
          results: [{ id: 'p1', type: 'post', title: 't', url: '/posts/p1', excerpt: '' }],
          nextCursor: 'cursor-1',
          hasMore: true
        })
      }
    });
    expect(body).toContain(`href="/search?q=${encodeURIComponent('测试')}&amp;after=cursor-1"`);
    expect(body).toContain('下一页');
    expect(body).toContain('返回第一页');
    expect(body).toContain('aria-label="搜索结果分页"');
  });

  it('校验失败 → 提示且保留重搜入口', () => {
    const { body } = render(SearchPage, {
      props: { data: baseData({ q: '长', searched: true, invalid: '搜索关键词过长（最多 200 字符）' }) }
    });
    expect(body).toContain('搜索关键词过长');
    expect(body).toContain('href="/search"');
  });

  it('load 错误 → 错误横幅与重试链接', () => {
    const { body } = render(SearchPage, {
      props: { data: baseData({ q: 'x', searched: true, error: '服务暂不可用' }) }
    });
    expect(body).toContain('服务暂不可用');
    expect(body).toContain('重试搜索');
  });
});

describe('M08-UI-06 429/挑战 SSR 回退', () => {
  it('429 → ChallengeGate：role=alert、重试按钮与返回搜索首页链接', () => {
    const { body } = render(SearchPage, {
      props: { data: baseData({ q: 'x', searched: true, rateLimited: { retryAfterSecs: 30 } }) }
    });
    expect(body).toContain('role="alert"');
    expect(body).toContain('搜索过于频繁');
    expect(body).toContain('href="/search"');
    expect(body).toContain('重新搜索');
  });

  it('challenge → 挑战入口为普通链接 + 无障碍回退文案', () => {
    const { body } = render(SearchPage, {
      props: { data: baseData({ q: 'x', searched: true, challenge: { challengeUrl: '/challenge/abc' } }) }
    });
    expect(body).toContain('需要完成验证');
    expect(body).toContain('href="/challenge/abc"');
    expect(body).toContain('返回搜索首页');
  });
});
