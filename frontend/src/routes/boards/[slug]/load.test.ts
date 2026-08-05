// M03-UI-06：板块详情 load 测试——成功取板块+帖子、404（不存在/隐藏）
// 抛 404 不泄漏存在性、帖子失败降级。
import { afterEach, describe, expect, it, vi } from 'vitest';
import { load, type BoardDetailData } from './+page.server';
import { getAuthed } from '$lib/api/server';

vi.mock('$lib/api/server', () => ({
  getAuthed: vi.fn()
}));

const getAuthedMock = getAuthed as unknown as ReturnType<typeof vi.fn>;

function loadEvent(slug: string) {
  return {
    params: { slug },
    cookies: { get: vi.fn(() => null) },
    request: { headers: new Headers() }
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

describe('M03-UI-06 板块详情 load', () => {
  it('成功 → 返回板块与帖子列表', async () => {
    getAuthedMock
      .mockResolvedValueOnce({ ok: true, data: board })
      .mockResolvedValueOnce({
        ok: true,
        data: { items: [{ id: 'p1', title: 'hi', author_id: 'u1', reply_count: 0, view_count: 1, pinned: 0, created_at: 0, last_reply_at: null }], page: { next_cursor: null, has_more: false } }
      });
    const data = (await load(loadEvent('tech'))) as BoardDetailData;
    expect(data.board).toEqual(board);
    expect(data.posts).toHaveLength(1);
    expect(data.error).toBeNull();
    expect(getAuthedMock).toHaveBeenCalledTimes(2);
    expect(getAuthedMock.mock.calls[1][1]).toBe('/api/v1/boards/tech/posts');
  });

  it('404（不存在/隐藏板块）→ 抛 404，不泄漏存在性', async () => {
    getAuthedMock.mockResolvedValueOnce({ ok: false, status: 404, message: 'board not found', requestId: null, retryAfterSecs: null, code: null });
    try {
      await load(loadEvent('ghost'));
      expect.unreachable('必须抛 404');
    } catch (err) {
      expect((err as { status?: number }).status).toBe(404);
    }
    expect(getAuthedMock).toHaveBeenCalledTimes(1); // 不再请求帖子
  });

  it('帖子接口失败 → 板块照常返回，posts 空数组 + 错误文案', async () => {
    getAuthedMock
      .mockResolvedValueOnce({ ok: true, data: board })
      .mockResolvedValueOnce({ ok: false, status: 503, message: 'unavailable', requestId: 'r', retryAfterSecs: null, code: null });
    const data = (await load(loadEvent('tech'))) as BoardDetailData;
    expect(data.board).toEqual(board);
    expect(data.posts).toEqual([]);
    expect(data.error).toBe('unavailable');
  });
});
