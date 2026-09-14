// M18-ADMIN-POINTS 测试：全站流水查询、B币归一化与调账 action。
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

describe('M18-ADMIN-POINTS: 积分管理', () => {
  it('load: 资产类型 asset=b_coin 自动归一化为 coin 传给后端', async () => {
    getAuthedMock
      .mockResolvedValueOnce({ ok: true, data: { items: [], next_cursor: null } })
      .mockResolvedValueOnce({ ok: true, data: {} });

    const event = createLoadEvent('asset=b_coin');
    const data = (await load(event)) as AdminPointsPageData;

    expect(data.ledger.state).toBe('ok');
    expect(data.ledger.filters.asset).toBe('coin');
    const ledgerApiCall = getAuthedMock.mock.calls[0][1] as string;
    expect(ledgerApiCall).toContain('asset=coin');
    expect(ledgerApiCall).not.toContain('asset=b_coin');
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
      message: '已成功为 chaos 调整 +9999 B币'
    });
  });

  it('adjust action: 拒绝 exp 并返回 B币-only 中文提示', async () => {
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
    expect(result.data.message).toBe('调账币种仅支持 B币');
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
});
