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
  author_name: 'chaos',
  author_display_name: null,
  board_id: 'b1',
  is_featured: false,
  reply_count: 2,
  view_count: 10,
  like_count: 0,
  pinned: false,
  created_at: 0,
  last_reply_at: null,
  // 作者装扮/上传头像投影：未携带时为 null（列表头像回退首字母占位）。
  author_presentation_tokens: null,
  author_avatar_attachment_id: null,
  // 参与者预览（公开投影）：原样透传（pickPostRow 仅做 null 归一）。
  participants: [{ id: 'u2', username: 'nina', display_name: null }]
};

interface HomeLoadData {
  boards: unknown[];
  tags: unknown[];
  posts: unknown[];
}

async function runLoad(fetchMock: typeof fetch): Promise<HomeLoadData> {
  // GAP-FIX 首页增强：load 需读 ?sort=/?after=（url）——测试按 load 自身
  // 结构补 fixture（默认全部 tab）。
  return (await load({
    fetch: fetchMock,
    url: new URL('http://test.local/')
  } as never)) as HomeLoadData;
}

describe('首页 server load（无 JS 基线）', () => {
  it('公开数据（板块/标签/最新讨论）在 SSR 阶段取回', async () => {
    const fetchMock = vi.fn(async (url: string | URL | Request) => {
      const u = String(url);
      if (u.includes('/api/v1/boards')) return jsonResponse({ items: [board], next_cursor: null, has_more: false });
      if (u.includes('/api/v1/tags')) return jsonResponse({ items: [tag], next_cursor: null, has_more: false });
      // GAP-FIX：首页帖子流由 /search 改为直连 /api/v1/posts（sort 筛选）。
      if (u.includes('/api/v1/posts')) return jsonResponse({ items: [post], page: { next_cursor: null, has_more: false } });
      return jsonResponse({});
    });
    const data = await runLoad(fetchMock as typeof fetch);
    expect(data.boards).toEqual([board]);
    expect(data.tags).toEqual([tag]);
    expect(data.posts).toEqual([post]);
    expect(fetchMock).toHaveBeenCalled();
  });

  it('作者/参与者装扮投影（presentation_tokens）原样透传', async () => {
    // 后端将作者装扮放在嵌套 author 投影里（participants 平级透传）；
    // pickPostRow 负责把 author.presentation_tokens / avatar_attachment_id
    // 压平到行上。
    const decorated = {
      ...post,
      author: {
        id: 'u1',
        username: 'chaos',
        display_name: 'Chaos',
        avatar_attachment_id: 'att-avatar-1',
        presentation_tokens: { avatar_frame: 'gold_ring' }
      },
      participants: [
        {
          id: 'u2',
          username: 'nina',
          display_name: '妮娜',
          avatar_attachment_id: 'att-avatar-2',
          presentation_tokens: { avatar_frame: 'glow' }
        }
      ]
    };
    const fetchMock = vi.fn(async (url: string | URL | Request) => {
      const u = String(url);
      if (u.includes('/api/v1/boards')) return jsonResponse({ items: [], next_cursor: null, has_more: false });
      if (u.includes('/api/v1/tags')) return jsonResponse({ items: [], next_cursor: null, has_more: false });
      if (u.includes('/api/v1/posts')) return jsonResponse({ items: [decorated], page: { next_cursor: null, has_more: false } });
      return jsonResponse({});
    });
    const data = await runLoad(fetchMock as typeof fetch);
    expect(data.posts[0]).toMatchObject({
      author_presentation_tokens: { avatar_frame: 'gold_ring' },
      author_avatar_attachment_id: 'att-avatar-1',
      participants: [{ avatar_attachment_id: 'att-avatar-2', presentation_tokens: { avatar_frame: 'glow' } }]
    });
  });

  it('后端不可用时降级为空数组（页面仍渲染站点壳）', async () => {
    const fetchMock = vi.fn(async () => new Response('boom', { status: 500 }));
    const data = await runLoad(fetchMock as typeof fetch);
    expect(data.boards).toEqual([]);
    expect(data.tags).toEqual([]);
    expect(data.posts).toEqual([]);
  });
});