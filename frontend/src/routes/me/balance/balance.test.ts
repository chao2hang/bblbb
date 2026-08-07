// M07-UI-01 积分页 load 测试——activity summary 取数、401 跳登录、后端错误
// 降级（SSR 渲染见 src/lib/testing/ssr/balance-checkin.test.ts）。
import { afterEach, describe, expect, it, vi } from 'vitest';
import { load, type BalancePageData } from './+page.server';
import { getAuthed } from '$lib/api/server';

vi.mock('$lib/api/server', () => ({
  getAuthed: vi.fn()
}));

const getAuthedMock = getAuthed as unknown as ReturnType<typeof vi.fn>;

const summary = {
  level: 7,
  xp: 320,
  checked_in_today: true,
  streak_days: 4,
  balances: [{ currency: 'coin', amount: 1500 }]
};

function loadEvent() {
  return {
    cookies: { get: vi.fn(() => null) },
    request: { headers: new Headers() }
  } as unknown as Parameters<typeof load>[0];
}

afterEach(() => vi.clearAllMocks());

describe('M07-UI-01 积分页 load', () => {
  it('成功 → summary', async () => {
    getAuthedMock.mockResolvedValueOnce({ ok: true, data: summary });
    const data = (await load(loadEvent())) as BalancePageData;
    expect(data.summary).toEqual(summary);
    expect(data.error).toBeNull();
  });

  it('401 → 跳登录', async () => {
    getAuthedMock.mockResolvedValueOnce({ ok: false, status: 401, message: 'auth', requestId: null, retryAfterSecs: null, code: null });
    await expect(load(loadEvent())).rejects.toMatchObject({ status: 303 });
  });

  it('后端错误 → summary=null + 错误文案', async () => {
    getAuthedMock.mockResolvedValueOnce({ ok: false, status: 503, message: 'unavailable', requestId: null, retryAfterSecs: null, code: null });
    const data = (await load(loadEvent())) as BalancePageData;
    expect(data.summary).toBeNull();
    expect(data.error).toBe('unavailable');
  });
});
