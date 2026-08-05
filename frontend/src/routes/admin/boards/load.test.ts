// M03-UI-07：管理板块页 load 测试——成功/403/501/错误状态映射、401 跳登录。
import { afterEach, describe, expect, it, vi } from 'vitest';
import { load, actions, type AdminBoardsPageData } from './+page.server';
import { getAuthed } from '$lib/api/server';

vi.mock('$lib/api/server', () => ({
  getAuthed: vi.fn(),
  authedPost: vi.fn()
}));

const getAuthedMock = getAuthed as unknown as ReturnType<typeof vi.fn>;

function loadEvent() {
  return {
    cookies: { get: vi.fn(() => null) },
    request: { headers: new Headers() }
  } as unknown as Parameters<typeof load>[0];
}

afterEach(() => vi.clearAllMocks());

describe('M03-UI-07 管理板块 load', () => {
  it('200 → ok 状态', async () => {
    getAuthedMock.mockResolvedValueOnce({ ok: true, data: { items: [{ id: 'b1', slug: 'tech', name: '技术分享' }] } });
    const data = (await load(loadEvent())) as AdminBoardsPageData;
    expect(data.loadState.state).toBe('ok');
    if (data.loadState.state === 'ok') expect(data.loadState.items).toHaveLength(1);
    expect(getAuthedMock.mock.calls[0][1]).toBe('/api/v1/admin/boards');
  });

  it('403 → forbidden 状态（后端裁决，前端不自行判权）', async () => {
    getAuthedMock.mockResolvedValueOnce({ ok: false, status: 403, message: 'forbidden', requestId: 'r', retryAfterSecs: null, code: 'forbidden' });
    const data = (await load(loadEvent())) as AdminBoardsPageData;
    expect(data.loadState.state).toBe('forbidden');
  });

  it('501 → not_implemented 状态（列表接口 M13-ADMIN 落地）', async () => {
    getAuthedMock.mockResolvedValueOnce({ ok: false, status: 501, message: 'not implemented', requestId: 'r', retryAfterSecs: null, code: 'not_implemented' });
    const data = (await load(loadEvent())) as AdminBoardsPageData;
    expect(data.loadState.state).toBe('not_implemented');
  });

  it('401 → 跳登录', async () => {
    getAuthedMock.mockResolvedValueOnce({ ok: false, status: 401, message: 'auth required', requestId: 'r', retryAfterSecs: null, code: 'authentication_required' });
    try {
      await load(loadEvent());
      expect.unreachable('必须跳登录');
    } catch (err) {
      expect((err as { status?: number }).status).toBe(303);
    }
  });
});

describe('M03-UI-07 管理板块 create action', () => {
  it('缺少名称/slug/原因 → 422 不调用代理', async () => {
    const { authedPost } = await import('$lib/api/server');
    const postMock = authedPost as unknown as ReturnType<typeof vi.fn>;
    const fd = new FormData();
    fd.set('name', 'x');
    const result = (await actions.create({
      request: { formData: () => Promise.resolve(fd), headers: new Headers() },
      cookies: { get: vi.fn(), set: vi.fn() }
    } as never)) as { status: number };
    expect(result.status).toBe(422);
    expect(postMock).not.toHaveBeenCalled();
  });
});
