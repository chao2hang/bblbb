// M08-UI-01/02：搜索 load 与校验/归一化测试。
//
// - 校验：q 必填、超长截断提示、limit clamp、非法 cursor 回退第一页；
// - load：引导态不请求、校验失败不请求、成功（契约/平面形状）归一化、
//   429 → rateLimited（Retry-After）、challenge 码 → challenge 态、其它错误；
// - 归一化：只保留后端安全投影字段（title/url/excerpt/highlight + 平面行），
//   对抗性输入（body/restricted_html 等）绝不出现在结果投影。
import { afterEach, describe, expect, it, vi } from 'vitest';
import { load, type SearchPageData } from '../../routes/search/+page.server';
import { getAuthed } from '$lib/api/server';
import {
  normalizeSearchPage,
  normalizeSearchQuery,
  SEARCH_QUERY_MAX,
  SEARCH_LIMIT_MAX,
  SEARCH_LIMIT_DEFAULT,
  normalizeCursor
} from '$lib/search';

vi.mock('$lib/api/server', () => ({
  getAuthed: vi.fn()
}));

const getAuthedMock = getAuthed as unknown as ReturnType<typeof vi.fn>;

function loadEvent(params: Record<string, string> = {}, requestId: string | null = null) {
  const url = new URL('http://test.local/search');
  for (const [k, v] of Object.entries(params)) url.searchParams.set(k, v);
  const headers = new Headers();
  if (requestId) headers.set('x-request-id', requestId);
  return {
    url,
    cookies: { get: vi.fn(() => null) },
    request: { headers }
  } as unknown as Parameters<typeof load>[0];
}

afterEach(() => vi.clearAllMocks());

const contractItem = {
  id: 'p1',
  type: 'post',
  title: '搜索测试帖',
  url: '/posts/search-test',
  excerpt: '安全摘要',
  highlight: '测试<em>高亮</em>'
};

describe('M08-UI-01 搜索参数校验', () => {
  it('空 q → 引导态（不请求）', async () => {
    const data = (await load(loadEvent({}))) as SearchPageData;
    expect(data.searched).toBe(false);
    expect(data.results).toEqual([]);
    expect(data.invalid).toBeNull();
    expect(getAuthedMock).not.toHaveBeenCalled();
  });

  it('超长 q → 截断 + 校验提示，且不调用 API', async () => {
    const long = 'a'.repeat(SEARCH_QUERY_MAX + 10);
    const data = (await load(loadEvent({ q: long }))) as SearchPageData;
    expect(data.invalid).toContain('过长');
    expect(data.searched).toBe(true);
    expect(data.q.length).toBeLessThanOrEqual(SEARCH_QUERY_MAX);
    expect(getAuthedMock).not.toHaveBeenCalled();
  });

  it('limit 超上限 → clamp 到 SEARCH_LIMIT_MAX', () => {
    const n = normalizeSearchQuery({ q: 'x', limit: '9999' });
    expect(n.limit).toBe(SEARCH_LIMIT_MAX);
  });

  it('非法/超长 cursor → 回退第一页并提示', () => {
    const c = normalizeCursor('x'.repeat(500));
    expect(c.cursor).toBeNull();
    expect(c.invalid).toContain('分页标记无效');
    const q = normalizeSearchQuery({ q: 'x', after: 'y'.repeat(500) });
    expect(q.after).toBeNull();
    expect(q.invalid).not.toBeNull();
  });
});

describe('M08-UI-01 search load', () => {
  it('成功（契约 SearchPage 形状）→ 归一化结果与分页', async () => {
    getAuthedMock.mockResolvedValueOnce({
      ok: true,
      data: { items: [contractItem], page: { next_cursor: 'cursor-1', has_more: true } }
    });
    const data = (await load(loadEvent({ q: '测试', limit: '30' }))) as SearchPageData;
    expect(data.searched).toBe(true);
    expect(data.results).toHaveLength(1);
    expect(data.results[0]).toMatchObject({
      id: 'p1',
      type: 'post',
      title: '搜索测试帖',
      url: '/posts/search-test',
      excerpt: '安全摘要',
      highlight: '测试<em>高亮</em>'
    });
    expect(data.nextCursor).toBe('cursor-1');
    expect(data.hasMore).toBe(true);
    const [cookies, path] = getAuthedMock.mock.calls[0];
    expect(path).toContain('/api/v1/search?');
    expect(path).toContain('q=');
    expect(path).toContain('limit=30');
    expect(cookies.get).toBeTypeOf('function');
  });

  it('成功（平面返回形状）→ next_cursor/has_more 顶层归一化 + url 推导', async () => {
    getAuthedMock.mockResolvedValueOnce({
      ok: true,
      data: {
        query: '测试',
        next_cursor: null,
        has_more: false,
        items: [
          {
            id: 'p2',
            title: '平面行',
            board_slug: 'tech',
            board_name: '技术分享',
            author_name: 'alice',
            reply_count: 3,
            view_count: 10,
            created_at: 123
          }
        ]
      }
    });
    const data = (await load(loadEvent({ q: '测试' }))) as SearchPageData;
    expect(data.results[0]).toMatchObject({
      id: 'p2',
      type: 'post',
      url: '/posts/p2',
      title: '平面行',
      board_name: '技术分享',
      author_name: 'alice'
    });
    expect(data.hasMore).toBe(false);
  });

  it('429 → rateLimited 态（Retry-After 透传）', async () => {
    getAuthedMock.mockResolvedValueOnce({ ok: false, status: 429, message: 'rate', requestId: 'r', retryAfterSecs: 60, code: 'rate_limited' });
    const data = (await load(loadEvent({ q: 'x' }))) as SearchPageData;
    expect(data.rateLimited).toEqual({ retryAfterSecs: 60 });
    expect(data.challenge).toBeNull();
  });

  it('challenge 码 → challenge 态', async () => {
    getAuthedMock.mockResolvedValueOnce({ ok: false, status: 429, message: 'challenge', requestId: 'r', retryAfterSecs: null, code: 'challenge_required' });
    const data = (await load(loadEvent({ q: 'x' }))) as SearchPageData;
    expect(data.challenge).not.toBeNull();
    expect(data.rateLimited).toBeNull();
  });

  it('其它错误 → error 文案', async () => {
    getAuthedMock.mockResolvedValueOnce({ ok: false, status: 503, message: 'unavailable', requestId: 'r', retryAfterSecs: null, code: null });
    const data = (await load(loadEvent({ q: 'x' }))) as SearchPageData;
    expect(data.error).toBe('unavailable');
    expect(data.results).toEqual([]);
  });

  it('转发 X-Request-ID', async () => {
    getAuthedMock.mockResolvedValueOnce({ ok: true, data: { items: [], page: { next_cursor: null, has_more: false } } });
    await load(loadEvent({ q: 'x' }, 'req-99'));
    const [, , requestId] = getAuthedMock.mock.calls[0];
    expect(requestId).toBe('req-99');
  });
});

describe('M08-UI-02 搜索结果归一化隐私守卫', () => {
  it('对抗性条目（隐藏正文/受限 HTML/凭据）不进结果投影', () => {
    const page = normalizeSearchPage(
      {
        items: [
          {
            id: 'p3',
            title: '标题',
            body: '隐藏正文不应泄漏',
            restricted_html: '<div>受限</div>',
            content: '完整正文',
            password_hash: 'HASH',
            excerpt: '安全摘要'
          }
        ],
        next_cursor: null,
        has_more: false
      },
      'q'
    );
    expect(page.items[0]).not.toHaveProperty('body');
    expect(page.items[0]).not.toHaveProperty('content');
    expect(page.items[0]).not.toHaveProperty('restricted_html');
    expect(page.items[0]).not.toHaveProperty('password_hash');
    expect(page.items[0].excerpt).toBe('安全摘要');
  });

  it('非对象/缺字段条目 → 安全空投影', () => {
    const page = normalizeSearchPage({ items: [null, 'str', 42], next_cursor: null, has_more: false }, 'q');
    expect(page.items).toHaveLength(3);
    for (const item of page.items) {
      expect(typeof item.title).toBe('string');
      expect(typeof item.excerpt).toBe('string');
    }
  });
});
