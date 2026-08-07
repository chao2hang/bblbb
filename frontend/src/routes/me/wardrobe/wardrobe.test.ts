// M07-UI-05 衣柜 load 测试——presentation + entitlements 取数、401 跳登录、
// 后端错误降级（SSR 渲染见 src/lib/testing/ssr/wardrobe.test.ts）。
import { afterEach, describe, expect, it, vi } from 'vitest';
import { load, type WardrobePageData } from './+page.server';
import { getAuthed } from '$lib/api/server';

vi.mock('$lib/api/server', () => ({
  getAuthed: vi.fn()
}));

const getAuthedMock = getAuthed as unknown as ReturnType<typeof vi.fn>;

const presentation = {
  version: 4,
  presentation_tokens: { nickname_color: 'blue' },
  updated_at: 0
};

const entitlements = [
  { id: 'e1', product_id: 'p1', product_title: '蓝色昵称', status: 'equipped', quantity: 1, remaining_quantity: 1, valid_from: 0, expires_at: null, created_at: 0 }
];

function loadEvent() {
  return {
    cookies: { get: vi.fn(() => null) },
    request: { headers: new Headers() }
  } as unknown as Parameters<typeof load>[0];
}

afterEach(() => vi.clearAllMocks());

describe('M07-UI-05 衣柜 load', () => {
  it('成功 → presentation + entitlements', async () => {
    getAuthedMock
      .mockResolvedValueOnce({ ok: true, data: presentation })
      .mockResolvedValueOnce({ ok: true, data: { items: entitlements } });
    const data = (await load(loadEvent())) as WardrobePageData;
    expect(data.presentation).toEqual(presentation);
    expect(data.entitlements).toHaveLength(1);
    expect(data.error).toBeNull();
  });

  it('presentation 401 → 跳登录', async () => {
    getAuthedMock.mockResolvedValueOnce({ ok: false, status: 401, message: 'auth', requestId: null, retryAfterSecs: null, code: null });
    await expect(load(loadEvent())).rejects.toMatchObject({ status: 303 });
  });

  it('presentation 错误 → 空数据 + 错误文案', async () => {
    getAuthedMock.mockResolvedValueOnce({ ok: false, status: 503, message: 'unavailable', requestId: null, retryAfterSecs: null, code: null });
    const data = (await load(loadEvent())) as WardrobePageData;
    expect(data.presentation).toBeNull();
    expect(data.entitlements).toEqual([]);
    expect(data.error).toBe('unavailable');
  });

  it('entitlements 失败不阻断 presentation', async () => {
    getAuthedMock
      .mockResolvedValueOnce({ ok: true, data: presentation })
      .mockResolvedValueOnce({ ok: false, status: 503, message: 'x', requestId: null, retryAfterSecs: null, code: null });
    const data = (await load(loadEvent())) as WardrobePageData;
    expect(data.presentation).toEqual(presentation);
    expect(data.entitlements).toEqual([]);
  });
});
