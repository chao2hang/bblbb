// M03-UI-06：板块/标签页无 JS SSR 基线——板块树、详情权限提示、标签分组
// 在 SSR HTML 中可读，且不输出私密字段。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import BoardsPage from '../../../routes/boards/+page.svelte';
import BoardDetailPage from '../../../routes/boards/[slug]/+page.svelte';
import TagsPage from '../../../routes/tags/+page.svelte';
import type { Board, PostSummary } from '$lib/api/types';

const rootBoard: Board = {
  id: 'b1',
  slug: 'tech',
  name: '技术分享',
  description: '技术文章',
  parent_id: null,
  visibility: 'public',
  posting_mode: 'normal',
  post_count: 3,
  is_active: 1,
  version: 0,
  created_at: 0,
  updated_at: 0
};

const childBoard: Board = { ...rootBoard, id: 'b2', slug: 'rust', name: 'Rust 专区', parent_id: 'b1' };

const post: PostSummary = {
  id: 'p1',
  title: 'Rust 入门',
  author_name: 'alice',
  author_id: 'u1',
  reply_count: 2,
  view_count: 10,
  pinned: false,
  created_at: 0,
  last_reply_at: null
};

describe('M03-UI-06 板块总览 SSR', () => {
  it('板块树：根板块 + 子板块 + 子板块数在 SSR HTML 中可读', () => {
    const { body } = render(BoardsPage, {
      props: { data: { boards: [rootBoard, childBoard], error: null } }
    });
    expect(body).toContain('技术分享');
    expect(body).toContain('Rust 专区');
    expect(body).toContain('1 个子板块');
    expect(body).toContain('href="/boards/tech"');
    expect(body).toContain('href="/boards/rust"');
  });

  it('权限提示：members/restricted 板块显示可见性徽标', () => {
    const restricted: Board = { ...rootBoard, slug: 'inner', name: '内测板块', visibility: 'restricted' };
    const { body } = render(BoardsPage, {
      props: { data: { boards: [restricted], error: null } }
    });
    expect(body).toContain('需加入板块可见');
  });

  it('空状态 SSR 可读；错误横幅渲染', () => {
    const empty = render(BoardsPage, { props: { data: { boards: [], error: null } } });
    expect(empty.body).toContain('暂无板块');
    const err = render(BoardsPage, { props: { data: { boards: [], error: 'unavailable' } } });
    expect(err.body).toContain('unavailable');
  });
});

describe('M03-UI-06 板块详情 SSR', () => {
  it('详情 + 权限提示（members + readonly）在 SSR HTML 中', () => {
    const board: Board = { ...rootBoard, visibility: 'members', posting_mode: 'readonly', post_count: 7 };
    const { body } = render(BoardDetailPage, {
      props: { data: { board, posts: [post], error: null } }
    });
    expect(body).toContain('技术分享');
    expect(body).toContain('7</strong>');
    expect(body).toContain('该板块仅对登录成员可见');
    expect(body).toContain('当前为只读，不能发布新帖');
    expect(body).toContain('Rust 入门');
  });

  it('无帖子 → 空状态可读', () => {
    const { body } = render(BoardDetailPage, {
      props: { data: { board: rootBoard, posts: [], error: null } }
    });
    expect(body).toContain('暂无帖子');
  });
});

describe('M03-UI-06 标签页 SSR', () => {
  const tags = [
    { id: 't1', slug: 'svelte', name: 'Svelte', description: null, color: '#ff3e00', group_id: 'g1', usage_count: 5 },
    { id: 't2', slug: 'rust', name: 'Rust', description: null, color: null, group_id: null, usage_count: 9 }
  ];
  const groups = [{ id: 'g1', name: '前端', slug: 'frontend', sort_order: 1 }];

  it('标签按分组展示，点击进入 ?tag= 筛选', () => {
    const { body } = render(TagsPage, { props: { data: { tags, groups, error: null } } });
    expect(body).toContain('前端');
    expect(body).toContain('Svelte');
    expect(body).toContain('href="/search?tag=svelte"');
    expect(body).toContain('Rust'); // 未分组进「其他」
    expect(body).toContain('href="/search?tag=rust"');
  });

  it('空状态可读', () => {
    const { body } = render(TagsPage, { props: { data: { tags: [], groups: [], error: null } } });
    expect(body).toContain('暂无标签');
  });
});
