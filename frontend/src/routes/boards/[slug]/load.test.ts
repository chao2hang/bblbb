// M03-UI-06：板块详情 load 测试——成功取板块+信息流（首页同款
// GET /api/v1/posts?board_id=…）、404（不存在/隐藏）抛 404 不泄漏存在性、
// 排序参数透传、板块内推荐位与关注态。
import { afterEach, describe, expect, it, vi } from 'vitest';
import { load, type BoardDetailData } from './+page.server';
import { getAuthed, getPublic } from '$lib/api/server';

vi.mock('$lib/api/server', () => ({
  getAuthed: vi.fn(),
  getPublic: vi.fn(),
  // GAP-FIX：load 现引用 SESSION_COOKIE 判断登录态（关注板块按钮）
  SESSION_COOKIE: 'bblbb_session'
}));

const getAuthedMock = getAuthed as unknown as ReturnType<typeof vi.fn>;
const getPublicMock = getPublic as unknown as ReturnType<typeof vi.fn>;

function jsonResponse(data: unknown, status = 200): Response {
  return new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json' }
  });
}

function loadEvent(slug: string, opts: { url?: string; authed?: boolean; fetchMock?: typeof fetch } = {}) {
  return {
    params: { slug },
    cookies: { get: vi.fn(() => (opts.authed ? 'sess-1' : null)) },
    request: { headers: new Headers() },
    // M18-BOARD-05：load 读 ?sort=/?after=（分类页与首页同构）并以 event.fetch
    // 走相对 /api/v1/posts（同源 Cookie 透传）——测试注入 fetch mock。
    url: new URL(opts.url ?? `http://local/boards/${slug}`),
    fetch: opts.fetchMock ?? (async () => jsonResponse({ items: [], page: { next_cursor: null, has_more: false } }))
  } as unknown as Parameters<typeof load>[0];
}

afterEach(() => vi.clearAllMocks());

const board = {
  id: 'b1',
  slug: 'tech',
  name: '技术分享',
  description: '技术文章',
  parent_id: null,
  visibility: 'members',
  posting_mode: 'readonly',
  post_count: 3,
  is_active: 1
};

const boardsListRow = { id: 'b1', slug: 'tech', name: '技术分享', post_count: 3 };

const feedPost = {
  id: 'p1',
  title: 'hi',
  author_id: 'u1',
  author_name: 'chaos',
  author_display_name: null,
  board_id: 'b1',
  is_featured: false,
  reply_count: 0,
  view_count: 1,
  like_count: 0,
  pinned: false,
  created_at: 0,
  last_reply_at: null,
  // 作者装扮/上传头像投影（feed.ts 白名单）：未携带时为 null。
  author_presentation_tokens: null,
  author_avatar_attachment_id: null,
  participants: []
};

const popularPost = { ...feedPost, id: 'p2', title: 'hot!', view_count: 99 };

describe('M03-UI-06 板块详情 load', () => {
  it('成功（匿名）→ 返回板块与信息流（首页同款端点 + 板块内推荐位）', async () => {
    getPublicMock.mockImplementation(async (path: string) => {
      if (path === '/api/v1/boards') return { ok: true, data: { items: [boardsListRow] } };
      if (path === '/api/v1/boards/tech') return { ok: true, data: board };
      return { ok: false, status: 404, message: 'nf', requestId: null, retryAfterSecs: null, code: null };
    });
    const fetchMock = vi.fn(async (url: string | URL | Request) => {
      const u = String(url);
      expect(u).toContain('/api/v1/posts');
      expect(u).toContain('board_id=b1');
      if (u.includes('sort=popular')) return jsonResponse({ items: [popularPost], page: { next_cursor: null, has_more: false } });
      return jsonResponse({ items: [feedPost], page: { next_cursor: 'c1', has_more: true } });
    });
    const data = (await load(loadEvent('tech', { fetchMock }))) as BoardDetailData;
    expect(data.board).toEqual(board);
    expect(data.posts).toEqual([JSON.parse(JSON.stringify(feedPost))]);
    expect(data.sort).toBe('');
    expect(data.nextCursor).toBe('c1');
    expect(data.hasMore).toBe(true);
    // 右栏推荐位：sort=popular 且按当前板块过滤
    expect(data.popular).toHaveLength(1);
    expect(data.popular[0].id).toBe('p2');
    // 板块导航
    expect(data.boards).toEqual([boardsListRow]);
    expect(getPublicMock.mock.calls.map((c) => c[0])).toEqual(
      expect.arrayContaining(['/api/v1/boards/tech', '/api/v1/boards'])
    );
  });

  it('404（不存在/隐藏板块）→ 抛 404，不泄漏存在性', async () => {
    getPublicMock.mockResolvedValueOnce({ ok: false, status: 404, message: 'board not found', requestId: null, retryAfterSecs: null, code: null });
    try {
      await load(loadEvent('ghost'));
      expect.unreachable('必须抛 404');
    } catch (err) {
      expect((err as { status?: number }).status).toBe(404);
    }
    expect(getPublicMock).toHaveBeenCalledTimes(1); // 不再请求信息流
  });

  it('?sort=featured 透传给信息流（board_id 同步携带）', async () => {
    getPublicMock.mockImplementation(async (path: string) => {
      if (path === '/api/v1/boards') return { ok: true, data: { items: [] } };
      if (path === '/api/v1/boards/tech') return { ok: true, data: board };
      return { ok: false, status: 500, message: 'x', requestId: null, retryAfterSecs: null, code: null };
    });
    const seen: string[] = [];
    const fetchMock = vi.fn(async (url: string | URL | Request) => {
      seen.push(String(url));
      return jsonResponse({ items: [], page: { next_cursor: null, has_more: false } });
    });
    await load(loadEvent('tech', { url: 'http://local/boards/tech?sort=featured', fetchMock }));
    const feedCall = seen.find((u) => !u.includes('sort=popular'));
    expect(feedCall).toContain('sort=featured');
    expect(feedCall).toContain('board_id=b1');
  });

  it('authed：关注态经 /me/following 判定；信息流照常返回', async () => {
    getAuthedMock.mockImplementation(async (cookies: unknown, path: string) => {
      if (path === '/api/v1/boards/tech') return { ok: true, data: board };
      if (path === '/api/v1/me/following') return { ok: true, data: { users: [], boards: ['tech'] } };
      if (path === '/api/v1/boards') return { ok: true, data: { items: [boardsListRow] } };
      return { ok: false, status: 500, message: 'x', requestId: null, retryAfterSecs: null, code: null };
    });
    const fetchMock = vi.fn(async () => jsonResponse({ items: [feedPost], page: { next_cursor: null, has_more: false } }));
    const data = (await load(loadEvent('tech', { authed: true, fetchMock }))) as BoardDetailData;
    expect(data.board).toEqual(board);
    expect(data.following).toBe(true);
    expect(data.posts).toHaveLength(1);
    expect(data.boards).toEqual([boardsListRow]);
  });

  it('信息流接口失败 → 板块照常返回，posts 空数组（静默降级，同首页）', async () => {
    getPublicMock.mockImplementation(async (path: string) => {
      if (path === '/api/v1/boards') return { ok: true, data: { items: [] } };
      if (path === '/api/v1/boards/tech') return { ok: true, data: board };
      return { ok: false, status: 500, message: 'x', requestId: null, retryAfterSecs: null, code: null };
    });
    const fetchMock = vi.fn(async () => jsonResponse({ status: 503 }, 503));
    const data = (await load(loadEvent('tech', { fetchMock }))) as BoardDetailData;
    expect(data.board).toEqual(board);
    expect(data.posts).toEqual([]);
    expect(data.popular).toEqual([]);
    expect(data.error).toBeNull();
  });
});
