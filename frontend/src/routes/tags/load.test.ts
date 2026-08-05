// M03-UI-06：标签页 load 测试——取标签+分组，错误降级。
import { afterEach, describe, expect, it, vi } from 'vitest';
import { load, type TagsPageData } from './+page.server';
import { getAuthed } from '$lib/api/server';

vi.mock('$lib/api/server', () => ({
  getAuthed: vi.fn()
}));

const getAuthedMock = getAuthed as unknown as ReturnType<typeof vi.fn>;

function loadEvent() {
  return {
    cookies: { get: vi.fn(() => null) },
    request: { headers: new Headers() }
  } as unknown as Parameters<typeof load>[0];
}

afterEach(() => vi.clearAllMocks());

describe('M03-UI-06 标签页 load', () => {
  it('成功 → 返回标签与分组', async () => {
    getAuthedMock.mockResolvedValueOnce({
      ok: true,
      data: {
        items: [{ id: 't1', slug: 'svelte', name: 'Svelte', description: null, color: '#ff3e00', group_id: 'g1', usage_count: 5 }],
        groups: [{ id: 'g1', name: '前端', slug: 'frontend', sort_order: 1 }]
      }
    });
    const data = (await load(loadEvent())) as TagsPageData;
    expect(data.tags).toHaveLength(1);
    expect(data.tags[0].slug).toBe('svelte');
    expect(data.groups).toHaveLength(1);
    expect(data.error).toBeNull();
    expect(getAuthedMock.mock.calls[0][1]).toBe('/api/v1/tags');
  });

  it('后端错误 → 空数组 + 错误文案', async () => {
    getAuthedMock.mockResolvedValueOnce({ ok: false, status: 503, message: 'unavailable', requestId: 'r', retryAfterSecs: null, code: null });
    const data = (await load(loadEvent())) as TagsPageData;
    expect(data.tags).toEqual([]);
    expect(data.groups).toEqual([]);
    expect(data.error).toBe('unavailable');
  });
});
