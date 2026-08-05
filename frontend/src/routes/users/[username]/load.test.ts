// M03-UI-01：用户主页 SSR load 测试——不存在/已注销→404、5xx→500、
// 成功→公开投影、其余错误→透传状态。
import { afterEach, describe, expect, it, vi } from 'vitest';
import { load, type UserPageData } from './+page.server';
import { getAuthed } from '$lib/api/server';

vi.mock('$lib/api/server', () => ({
  getAuthed: vi.fn()
}));

const getAuthedMock = getAuthed as unknown as ReturnType<typeof vi.fn>;

const publicProfile = {
  id: 'u1',
  username: 'alice',
  display_name: '爱丽丝',
  bio: '公开简介',
  level: 7,
  avatar_attachment_id: null,
  cover_attachment_id: null,
  signature: '公开签名',
  created_at: 1700000000000
};

function loadEvent(username: string, requestId: string | null = null) {
  const headers = new Headers();
  if (requestId) headers.set('x-request-id', requestId);
  return {
    params: { username },
    cookies: { get: vi.fn(() => null) },
    request: { headers }
  } as unknown as Parameters<typeof load>[0];
}

afterEach(() => {
  vi.clearAllMocks();
});

describe('M03-UI-01 用户主页 SSR load', () => {
  it('成功 → 返回公开投影（转发 X-Request-ID）', async () => {
    getAuthedMock.mockResolvedValueOnce({ ok: true, data: publicProfile });
    const data = (await load(loadEvent('alice', 'req-1'))) as UserPageData;
    expect(data).toEqual({ user: publicProfile });
    const [cookies, path, requestId] = getAuthedMock.mock.calls[0];
    expect(path).toBe('/api/v1/users/alice');
    expect(requestId).toBe('req-1');
  });

  it('404（不存在/已注销/匿名化）→ 抛 404（不泄漏存在性）', async () => {
    getAuthedMock.mockResolvedValueOnce({ ok: false, status: 404, message: 'user not found', requestId: null });
    try {
      await load(loadEvent('ghost'));
      expect.unreachable('必须抛 404');
    } catch (err) {
      expect((err as { status?: number }).status).toBe(404);
    }
    // deleted 用户同样 404：页面与「不存在」不可区分。
    getAuthedMock.mockResolvedValueOnce({ ok: false, status: 404, message: 'user not found', requestId: null });
    try {
      await load(loadEvent('deleted-user'));
      expect.unreachable('必须抛 404');
    } catch (err) {
      expect((err as { status?: number }).status).toBe(404);
    }
  });

  it('5xx → 抛 500', async () => {
    getAuthedMock.mockResolvedValueOnce({ ok: false, status: 503, message: 'unavailable', requestId: null });
    try {
      await load(loadEvent('alice'));
      expect.unreachable('必须抛 500');
    } catch (err) {
      expect((err as { status?: number }).status).toBe(500);
    }
  });

  it('banned/pending_delete 降级投影 → 200 正常返回（页面按公开投影渲染）', async () => {
    // 后端对 banned/pending_delete 返回 200 降级投影（bio/signature/媒体置空），
    // load 不额外区分，直接透传公开投影（状态字段永不出现）。
    const degraded = { ...publicProfile, bio: null, signature: null };
    getAuthedMock.mockResolvedValueOnce({ ok: true, data: degraded });
    const data = (await load(loadEvent('banned-user'))) as UserPageData;
    expect(data.user).toEqual(degraded);
    expect(data.user).not.toHaveProperty('status');
    expect(data.user).not.toHaveProperty('email');
  });
});
