// M00-FRONTEND-08：首页 server load 在 SSR 阶段取回公开数据（无 JS 基线）。
import { describe, expect, it, vi } from 'vitest';
import { load } from '../../routes/+page.server';

function jsonResponse(data: unknown): Response {
  return new Response(JSON.stringify(data), {
    status: 200,
    headers: { 'Content-Type': 'application/json' }
  });
}

const board = {
  id: 'b1',
  slug: 'general',
  name: '综合讨论',
  description: '日常闲聊',
  post_count: 3,
  is_active: true
};

const tag = { id: 't1', slug: 'svelte', name: 'svelte', usage_count: 5 };

const post = {
  id: 'p1',
  title: '你好 BBLBB',
  author_id: 'u1',
  reply_count: 2,
  view_count: 10,
  pinned: false,
  created_at: 0,
  last_reply_at: null
};

interface HomeLoadData {
  boards: unknown[];
  tags: unknown[];
  posts: unknown[];
}

async function runLoad(fetchMock: typeof fetch): Promise<HomeLoadData> {
  return (await load({ fetch: fetchMock } as never)) as HomeLoadData;
}

describe('首页 server load（无 JS 基线）', () => {
  it('公开数据（板块/标签/最新讨论）在 SSR 阶段取回', async () => {
    const fetchMock = vi.fn(async (url: string | URL | Request) => {
      const u = String(url);
      if (u.includes('/api/v1/boards')) return jsonResponse({ items: [board], next_cursor: null, has_more: false });
      if (u.includes('/api/v1/tags')) return jsonResponse({ items: [tag], next_cursor: null, has_more: false });
      if (u.includes('/api/v1/search')) return jsonResponse({ items: [post], next_cursor: null, has_more: false });
      return jsonResponse({});
    });
    const data = await runLoad(fetchMock as typeof fetch);
    expect(data.boards).toEqual([board]);
    expect(data.tags).toEqual([tag]);
    expect(data.posts).toEqual([post]);
    expect(fetchMock).toHaveBeenCalled();
  });

  it('后端不可用时降级为空数组（页面仍渲染站点壳）', async () => {
    const fetchMock = vi.fn(async () => new Response('boom', { status: 500 }));
    const data = await runLoad(fetchMock as typeof fetch);
    expect(data.boards).toEqual([]);
    expect(data.tags).toEqual([]);
    expect(data.posts).toEqual([]);
  });
});