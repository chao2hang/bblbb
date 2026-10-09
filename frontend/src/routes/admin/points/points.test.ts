// 积分与货币管理测试：用户积分列表拉取、筛选与调账 action。
import { afterEach, describe, expect, it, vi } from 'vitest';
import { load, actions, type AdminPointsPageData } from './+page.server';
import { authedPost, getAuthed } from '$lib/api/server';

vi.mock('$lib/api/server', () => ({
  getAuthed: vi.fn(),
  authedPost: vi.fn()
}));

const getAuthedMock = getAuthed as unknown as ReturnType<typeof vi.fn>;
const authedPostMock = authedPost as unknown as ReturnType<typeof vi.fn>;

function createLoadEvent(queryString = '') {
  const url = new URL(`http://localhost/admin/points${queryString ? `?${queryString}` : ''}`);
  return {
    cookies: { get: vi.fn(() => null) },
    request: { headers: new Headers() },
    url
  } as unknown as Parameters<typeof load>[0];
}

afterEach(() => vi.clearAllMocks());

describe('积分与货币管理（用户列表与调账）', () => {
  it('load: 成功获取用户列表与对应积分', async () => {
    getAuthedMock.mockResolvedValueOnce({
      ok: true,
      data: {
        items: [
          {
            id: 'u-1',
            username: 'chaos',
            email: 'chaos@example.com',
            email_verified: true,
            status: 'active',
            display_name: 'Chaos',
            level: 3,
            roles: ['administrator'],
            coin_balance: 500,
            created_at: 1700000000000,
            updated_at: 1700000000000,
            last_login_at: 1700001000000,
            version: 1
          }
        ],
        next_cursor: 'cursor-abc'
      }
    });

    const event = createLoadEvent();
    const data = (await load(event)) as AdminPointsPageData;

    expect(data.state).toBe('ok');
    expect(data.items).toHaveLength(1);
    expect(data.items[0].coin_balance).toBe(500);
    expect(data.nextCursor).toBe('cursor-abc');
  });

  it('load: 透传搜索关键词 q (或兼容 username) 与 status', async () => {
    getAuthedMock.mockResolvedValueOnce({
      ok: true,
      data: { items: [], next_cursor: null }
    });

    const event = createLoadEvent('username=alice&status=active');
    const data = (await load(event)) as AdminPointsPageData;

    expect(data.state).toBe('ok');
    expect(data.filters.q).toBe('alice');
    expect(data.filters.status).toBe('active');
    const apiCall = getAuthedMock.mock.calls[0][1] as string;
    expect(apiCall).toContain('q=alice');
    expect(apiCall).toContain('status=active');
  });

  it('load: 403 权限不足返回 forbidden 状态', async () => {
    getAuthedMock.mockResolvedValueOnce({
      ok: false,
      status: 403,
      message: '无权限访问'
    });

    const event = createLoadEvent();
    const data = (await load(event)) as AdminPointsPageData;

    expect(data.state).toBe('forbidden');
    expect(data.items).toEqual([]);
    expect(data.error).toBe('无权限访问');
  });

  it('adjust action: 仅接受 coin 并成功调账', async () => {
    authedPostMock.mockResolvedValueOnce({
      ok: true,
      data: { username: 'chaos', currency: 'coin', amount: 9999, balance: 9999 }
    });

    const formData = new FormData();
    formData.set('username', 'chaos');
    formData.set('currency', 'coin');
    formData.set('amount', '9999');
    formData.set('reason', '测试积分');

    const request = new Request('http://localhost/admin/points?/adjust', {
      method: 'POST',
      body: formData
    });

    const result = await actions.adjust({
      request,
      cookies: {} as any,
      params: {},
      url: new URL('http://localhost/admin/points?/adjust'),
      route: { id: '/admin/points' }
    } as any);

    expect(authedPostMock).toHaveBeenCalledTimes(1);
    const sentPayload = authedPostMock.mock.calls[0][2] as Record<string, unknown>;
    expect(sentPayload.username).toBe('chaos');
    expect(sentPayload.currency).toBe('coin');
    expect(sentPayload.amount).toBe(9999);
    expect(sentPayload.reason).toBe('测试积分');

    expect(result).toEqual({
      message: '已成功为 chaos 调整 +9999 站点消费货币'
    });
  });

  it('adjust action: 拒绝 exp 并返回站点消费货币限制提示', async () => {
    const formData = new FormData();
    formData.set('username', 'chaos');
    formData.set('currency', 'exp');
    formData.set('amount', '10');
    formData.set('reason', '测试积分');

    const request = new Request('http://localhost/admin/points?/adjust', {
      method: 'POST',
      body: formData
    });

    const result = (await actions.adjust({
      request,
      cookies: {} as any,
      params: {},
      url: new URL('http://localhost/admin/points?/adjust'),
      route: { id: '/admin/points' }
    } as any)) as any;

    expect(result.status).toBe(422);
    expect(result.data.message).toBe('调账币种仅支持站点消费货币');
    expect(authedPostMock).not.toHaveBeenCalled();
  });

  it('adjust action: 字段必填校验', async () => {
    const formData = new FormData();
    formData.set('username', '');
    formData.set('currency', 'coin');
    formData.set('amount', '0');
    formData.set('reason', '');

    const request = new Request('http://localhost/admin/points?/adjust', {
      method: 'POST',
      body: formData
    });

    const result = (await actions.adjust({
      request,
      cookies: {} as any,
      params: {},
      url: new URL('http://localhost/admin/points?/adjust'),
      route: { id: '/admin/points' }
    } as any)) as any;

    expect(result.status).toBe(422);
    expect(authedPostMock).not.toHaveBeenCalled();
  });

  it('adjust action: 处理 step_up_required', async () => {
    authedPostMock.mockResolvedValueOnce({
      ok: false,
      status: 403,
      code: 'step_up_required',
      message: 'Step-up required'
    });

    const formData = new FormData();
    formData.set('username', 'chaos');
    formData.set('currency', 'coin');
    formData.set('amount', '100');
    formData.set('reason', '奖励');

    const request = new Request('http://localhost/admin/points?/adjust', {
      method: 'POST',
      body: formData
    });

    const result = (await actions.adjust({
      request,
      cookies: {} as any,
      params: {},
      url: new URL('http://localhost/admin/points?/adjust'),
      route: { id: '/admin/points' }
    } as any)) as any;

    expect(result.status).toBe(403);
    expect(result.data.stepUpRequired).toBe(true);
  });
});
