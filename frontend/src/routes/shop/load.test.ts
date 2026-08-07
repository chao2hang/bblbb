// M07-UI-02/03：商城 load 测试——列表/详情 SSR 阶段取数，401 跳登录，
// 余额/等级来自 activity summary（失败不阻断），后端错误降级。
import { afterEach, describe, expect, it, vi } from 'vitest';
import { load as listLoad, type ShopPageData } from './+page.server';
import { load as detailLoad, type ShopProductPageData } from './[id]/+page.server';
import { getAuthed } from '$lib/api/server';
import type { ShopProduct } from '$lib/api/types';

vi.mock('$lib/api/server', () => ({
  getAuthed: vi.fn()
}));

const getAuthedMock = getAuthed as unknown as ReturnType<typeof vi.fn>;

function loadEvent(requestId: string | null = null) {
  const headers = new Headers();
  if (requestId) headers.set('x-request-id', requestId);
  return {
    cookies: { get: vi.fn(() => null) },
    request: { headers },
    params: { id: 'p1' }
  } as unknown as Parameters<typeof listLoad>[0];
}

afterEach(() => vi.clearAllMocks());

const product: ShopProduct = {
  id: 'p1',
  kind: 'cosmetic_nickname',
  status: 'published',
  slug: 'blue-name',
  title: '蓝色昵称',
  currency: 'coin',
  unit_price: 50,
  quantity_limit: 1,
  required_level: 1,
  refund_policy: 'non_refundable',
  version: 3,
  created_at: 0,
  updated_at: 0
};

describe('M07-UI-02 商城列表 load', () => {
  it('成功 → 商品列表 + 余额/等级（activity summary）', async () => {
    getAuthedMock
      .mockResolvedValueOnce({ ok: true, data: { items: [product] } })
      .mockResolvedValueOnce({
        ok: true,
        data: { level: 5, xp: 10, checked_in_today: true, streak_days: 2, balances: [{ currency: 'coin', amount: 200 }] }
      });
    const data = (await listLoad(loadEvent('req-1'))) as ShopPageData;
    expect(data.products).toEqual([product]);
    expect(data.balance).toEqual({ currency: 'coin', amount: 200 });
    expect(data.level).toBe(5);
    expect(data.error).toBeNull();
    // 转发 X-Request-ID
    expect(getAuthedMock.mock.calls[0][2]).toBe('req-1');
  });

  it('商品接口 401 → 跳登录', async () => {
    getAuthedMock.mockResolvedValueOnce({ ok: false, status: 401, message: 'auth', requestId: null, retryAfterSecs: null, code: null });
    await expect(listLoad(loadEvent())).rejects.toMatchObject({ status: 303 });
  });

  it('商品接口错误 → 空列表 + 错误文案（不展示余额）', async () => {
    getAuthedMock.mockResolvedValueOnce({ ok: false, status: 503, message: 'unavailable', requestId: 'r', retryAfterSecs: null, code: null });
    const data = (await listLoad(loadEvent())) as ShopPageData;
    expect(data.products).toEqual([]);
    expect(data.error).toBe('unavailable');
  });

  it('activity summary 失败不阻断商品列表', async () => {
    getAuthedMock
      .mockResolvedValueOnce({ ok: true, data: { items: [product] } })
      .mockResolvedValueOnce({ ok: false, status: 503, message: 'x', requestId: null, retryAfterSecs: null, code: null });
    const data = (await listLoad(loadEvent())) as ShopPageData;
    expect(data.products).toEqual([product]);
    expect(data.balance).toBeNull();
    expect(data.level).toBeNull();
  });
});

describe('M07-UI-03 商品详情 load', () => {
  it('成功 → 商品 + 余额/等级/已购数', async () => {
    getAuthedMock
      .mockResolvedValueOnce({ ok: true, data: product })
      .mockResolvedValueOnce({
        ok: true,
        data: { level: 5, xp: 10, checked_in_today: true, streak_days: 2, balances: [{ currency: 'coin', amount: 200 }] }
      })
      .mockResolvedValueOnce({ ok: true, data: { items: [{ id: 'e1', product_id: 'p1', status: 'owned' }] } });
    const data = (await detailLoad(loadEvent() as unknown as Parameters<typeof detailLoad>[0])) as ShopProductPageData;
    expect(data.product).toEqual(product);
    expect(data.ownedCount).toBe(1);
  });

  it('详情 404 → product=null + 错误文案', async () => {
    getAuthedMock.mockResolvedValueOnce({ ok: false, status: 404, message: 'not found', requestId: null, retryAfterSecs: null, code: null });
    const data = (await detailLoad(loadEvent() as unknown as Parameters<typeof detailLoad>[0])) as ShopProductPageData;
    expect(data.product).toBeNull();
    expect(data.error).toBe('not found');
  });
});
