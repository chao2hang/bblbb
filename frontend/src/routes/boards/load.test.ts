// M03-UI-06：板块总览 load 测试——SSR 阶段取回板块（转发会话）、
// 后端错误降级为空数组。
import { afterEach, describe, expect, it, vi } from 'vitest';
import { load, type BoardsPageData } from './+page.server';
import { getAuthed } from '$lib/api/server';

vi.mock('$lib/api/server', () => ({
  getAuthed: vi.fn()
}));

const getAuthedMock = getAuthed as unknown as ReturnType<typeof vi.fn>;

function loadEvent(requestId: string | null = null) {
  const headers = new Headers();
  if (requestId) headers.set('x-request-id', requestId);
  return {
    cookies: { get: vi.fn(() => null) },
    request: { headers }
  } as unknown as Parameters<typeof load>[0];
}

afterEach(() => vi.clearAllMocks());

const board = {
  id: 'b1',
  slug: 'tech',
  name: '技术分享',
  description: '技术文章',
  parent_id: null,
  visibility: 'public',
  posting_mode: 'normal',
  post_count: 3,
  is_active: 1
};

describe('M03-UI-06 板块总览 load', () => {
  it('成功 → 返回板块树数据（转发 X-Request-ID 与会话代理）', async () => {
    getAuthedMock.mockResolvedValueOnce({
      ok: true,
      data: { items: [board], page: { next_cursor: null, has_more: false } }
    });
    const data = (await load(loadEvent('req-1'))) as BoardsPageData;
    expect(data.boards).toEqual([board]);
    expect(data.error).toBeNull();
    const [cookies, path, requestId] = getAuthedMock.mock.calls[0];
    expect(path).toBe('/api/v1/boards');
    expect(requestId).toBe('req-1');
    expect(cookies.get).toBeTypeOf('function');
  });

  it('后端错误 → 空数组 + 错误文案（页面渲染错误横幅）', async () => {
    getAuthedMock.mockResolvedValueOnce({ ok: false, status: 503, message: 'unavailable', requestId: 'r', retryAfterSecs: null, code: null });
    const data = (await load(loadEvent())) as BoardsPageData;
    expect(data.boards).toEqual([]);
    expect(data.error).toBe('unavailable');
  });
});
