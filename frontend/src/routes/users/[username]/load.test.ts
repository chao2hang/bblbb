// M03-UI-01：用户主页 SSR load 测试——不存在/已注销→404、5xx→500、
// 成功→公开投影、其余错误→透传状态。
import { afterEach, describe, expect, it, vi } from 'vitest';
import { actions, load, type UserPageData } from './+page.server';
import { authedPost, getAuthed, getPublic } from '$lib/api/server';
import { newClientRequestId } from '$lib/api/client';

vi.mock('$lib/api/server', () => ({
  getAuthed: vi.fn(),
  getPublic: vi.fn(),
  authedPost: vi.fn(),
  SESSION_COOKIE: '__Host-bblbb_session'
}));

vi.mock('$lib/api/client', () => ({
  newClientRequestId: vi.fn(() => 'message-request-test-0001')
}));

const getAuthedMock = getAuthed as unknown as ReturnType<typeof vi.fn>;
const getPublicMock = getPublic as unknown as ReturnType<typeof vi.fn>;
const authedPostMock = authedPost as unknown as ReturnType<typeof vi.fn>;
const newClientRequestIdMock = newClientRequestId as unknown as ReturnType<typeof vi.fn>;

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

function loadEvent(
  username: string,
  requestId: string | null = null,
  session: string | null = null
) {
  const headers = new Headers();
  if (requestId) headers.set('x-request-id', requestId);
  return {
    params: { username },
    cookies: { get: vi.fn(() => session) },
    request: { headers }
  } as unknown as Parameters<typeof load>[0];
}

function actionEvent(username: string, requestId: string | null = null) {
  const headers = new Headers();
  if (requestId) headers.set('x-request-id', requestId);
  return {
    params: { username },
    cookies: { get: vi.fn(() => 'session-1'), set: vi.fn() },
    request: { headers }
  } as unknown as Parameters<NonNullable<typeof actions.message>>[0];
}

afterEach(() => {
  vi.clearAllMocks();
});

describe('M03-UI-01 用户主页 SSR load', () => {
  it('成功 → 返回公开投影（转发 X-Request-ID）', async () => {
    getPublicMock.mockResolvedValueOnce({ ok: true, data: publicProfile });
    const data = (await load(loadEvent('alice', 'req-1'))) as UserPageData;
    expect(data).toEqual({ user: publicProfile, authed: false });
    const [path, requestId] = getPublicMock.mock.calls[0];
    expect(path).toBe('/api/v1/users/alice');
    expect(requestId).toBe('req-1');
  });

  it('会话 Cookie 存在 → authed=true（关注按钮门控数据）', async () => {
    getAuthedMock.mockResolvedValueOnce({ ok: true, data: publicProfile });
    const data = (await load(loadEvent('alice', null, 'sess-1'))) as UserPageData;
    expect(data.authed).toBe(true);
  });

  it('404（不存在/已注销/匿名化）→ 抛 404（不泄漏存在性）', async () => {
    getPublicMock.mockResolvedValueOnce({ ok: false, status: 404, message: 'user not found', requestId: null });
    try {
      await load(loadEvent('ghost'));
      expect.unreachable('必须抛 404');
    } catch (err) {
      expect((err as { status?: number }).status).toBe(404);
    }
    // deleted 用户同样 404：页面与「不存在」不可区分。
    getPublicMock.mockResolvedValueOnce({ ok: false, status: 404, message: 'user not found', requestId: null });
    try {
      await load(loadEvent('deleted-user'));
      expect.unreachable('必须抛 404');
    } catch (err) {
      expect((err as { status?: number }).status).toBe(404);
    }
  });

  it('5xx → 抛 500', async () => {
    getPublicMock.mockResolvedValueOnce({ ok: false, status: 503, message: 'unavailable', requestId: null });
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
    getPublicMock.mockResolvedValueOnce({ ok: true, data: degraded });
    const data = (await load(loadEvent('banned-user'))) as UserPageData;
    expect(data.user).toEqual(degraded);
    expect(data.user).not.toHaveProperty('status');
    expect(data.user).not.toHaveProperty('email');
  });
});

describe('M17-GAPFIX-07 用户主页发私信 action', () => {
  it('成功创建/复用会话 → 303 跳转 /messages?c=', async () => {
    authedPostMock.mockResolvedValueOnce({ ok: true, data: { id: 'conversation-1' } });
    await expect(actions.message(actionEvent('bob', 'req-message-1'))).rejects.toMatchObject({
      status: 303,
      location: '/messages?c=conversation-1'
    });
    expect(newClientRequestIdMock).toHaveBeenCalledOnce();
    expect(authedPostMock).toHaveBeenCalledWith(
      expect.anything(),
      '/api/v1/conversations',
      { username: 'bob', client_request_id: 'message-request-test-0001' },
      'req-message-1',
      { 'Idempotency-Key': 'message-request-test-0001' }
    );
  });

  it('401 → 登录并保留用户主页回跳', async () => {
    authedPostMock.mockResolvedValueOnce({
      ok: false,
      status: 401,
      message: 'unauthorized',
      requestId: 'req-unauthorized'
    });
    await expect(actions.message(actionEvent('bob'))).rejects.toMatchObject({
      status: 303,
      location: '/login?next=%2Fusers%2Fbob'
    });
  });

  it('后端错误 → failure 状态并保留错误信息', async () => {
    authedPostMock.mockResolvedValueOnce({
      ok: false,
      status: 422,
      message: 'cannot message yourself',
      requestId: 'req-invalid'
    });
    const result = await actions.message(actionEvent('alice'));
    expect(result).toMatchObject({ status: 422, data: { message: 'cannot message yourself', requestId: 'req-invalid' } });
  });
});
