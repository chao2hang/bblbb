// 社交域·关注：/users/{username}/following SSR load 测试——
// 成功→公开投影行透传（?after= 游标跟随）、404→抛 404（不泄漏存在性）、
// 5xx 与非 Problem 异常→500、其余 4xx→透传状态。
import { afterEach, describe, expect, it, vi } from 'vitest';
import { load, type UserFollowingPageData } from './+page.server';
import { listFollowing } from '$lib/api/client';

vi.mock('$lib/api/client', () => ({
  listFollowing: vi.fn()
}));

const listFollowingMock = listFollowing as unknown as ReturnType<typeof vi.fn>;

const rows = [
  { username: 'carol', display_name: '卡萝尔', level: 3, created_at: 1_700_000_000_000 },
  { username: 'dave', display_name: null, level: 2, created_at: 1_699_000_000_000 }
];

function loadEvent(username: string, after: string | null = null) {
  const url = new URL(
    after
      ? `/users/${encodeURIComponent(username)}/following?after=${encodeURIComponent(after)}`
      : `/users/${encodeURIComponent(username)}/following`,
    'http://localhost'
  );
  return { params: { username }, url, fetch: vi.fn() } as unknown as Parameters<typeof load>[0];
}

afterEach(() => {
  vi.clearAllMocks();
});

describe('社交域 /users/{username}/following SSR load', () => {
  it('成功 → 公开投影行透传，next_cursor 空串 → null', async () => {
    listFollowingMock.mockResolvedValueOnce({ items: rows, next_cursor: '' });
    const data = (await load(loadEvent('chaos'))) as UserFollowingPageData;
    expect(data).toEqual({ username: 'chaos', items: rows, nextCursor: null, after: null });
    expect(listFollowingMock).toHaveBeenCalledWith(expect.anything(), 'chaos', null);
  });

  it('?after= 游标跟随（无 JS 分页回退与客户端追加同游标口径）', async () => {
    listFollowingMock.mockResolvedValueOnce({ items: rows, next_cursor: '1699000000000' });
    const data = (await load(loadEvent('chaos', '1698000000000'))) as UserFollowingPageData;
    expect(listFollowingMock).toHaveBeenCalledWith(expect.anything(), 'chaos', '1698000000000');
    expect(data.nextCursor).toBe('1699000000000');
    expect(data.after).toBe('1698000000000');
  });

  it('404（用户不存在/已注销/匿名化）→ 抛 404（不泄漏存在性）', async () => {
    listFollowingMock.mockRejectedValueOnce({ status: 404, detail: 'user not found' });
    try {
      await load(loadEvent('ghost'));
      expect.unreachable('必须抛 404');
    } catch (err) {
      expect((err as { status?: number }).status).toBe(404);
    }
  });

  it('5xx → 抛 500', async () => {
    listFollowingMock.mockRejectedValueOnce({ status: 500, detail: 'unavailable' });
    try {
      await load(loadEvent('chaos'));
      expect.unreachable('必须抛 500');
    } catch (err) {
      expect((err as { status?: number }).status).toBe(500);
    }
  });

  it('其余 4xx → 状态透传', async () => {
    listFollowingMock.mockRejectedValueOnce({ status: 400, detail: 'bad cursor' });
    try {
      await load(loadEvent('chaos'));
      expect.unreachable('必须抛 400');
    } catch (err) {
      expect((err as { status?: number }).status).toBe(400);
    }
  });

  it('非 Problem 异常（无 status）→ 500（与用户主页 load 同口径）', async () => {
    listFollowingMock.mockRejectedValueOnce(new Error('boom'));
    try {
      await load(loadEvent('chaos'));
      expect.unreachable('必须抛 500');
    } catch (err) {
      expect((err as { status?: number }).status).toBe(500);
    }
  });
});
