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
  icon: null,
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
    expect(empty.body).toContain('登录后发布讨论');
    expect(empty.body).toContain('href="/login?next=%2Feditor"');
    const err = render(BoardsPage, { props: { data: { boards: [], error: 'unavailable' } } });
    expect(err.body).toContain('unavailable');
  });
});

describe('M03-UI-06 板块详情 SSR', () => {
  // M18-BOARD-05：分类页与首页同构——posts 为首页同款白名单行投影（FeedRow），
  // 排序为首页 tab 集（'' 最新 | featured | following | popular）。
  const feedRow = {
    id: 'p1',
    title: 'Rust 入门',
    author_id: 'u1',
    author_name: 'alice',
    author_display_name: null,
    board_id: 'b1',
    is_featured: false,
    reply_count: 2,
    view_count: 10,
    like_count: 0,
    pinned: false,
    created_at: 0,
    last_reply_at: null,
    participants: [{ id: 'u2', username: 'nina', display_name: null }]
  };

  it('详情 + 权限提示（members + readonly）在 SSR HTML 中', () => {
    const board: Board = { ...rootBoard, visibility: 'members', posting_mode: 'readonly', post_count: 7 };
    const { body } = render(BoardDetailPage, {
      props: { data: { board, posts: [feedRow], error: null, sort: '', following: false, authed: true, after: null, nextCursor: null, hasMore: false, popular: [], boards: [] } }
    });
    expect(body).toContain('技术分享');
    expect(body).toContain('该板块仅对登录成员可见');
    expect(body).toContain('当前为只读，不能发布新帖');
    expect(body).toContain('Rust 入门');
  });

  it('无帖子 → 空状态可读', () => {
    const { body } = render(BoardDetailPage, {
      props: { data: { board: rootBoard, posts: [], error: null, sort: '', following: false, authed: true, after: null, nextCursor: null, hasMore: false, popular: [], boards: [] } }
    });
    expect(body).toContain('暂无帖子');
  });

  it('信息流与首页同构：feed 工具条 + 单一列表外框 + 右栏板块内推荐位', () => {
    const { body } = render(BoardDetailPage, {
      props: { data: { board: rootBoard, posts: [feedRow], error: null, sort: '', following: false, authed: true, after: null, nextCursor: null, hasMore: false, popular: [{ ...feedRow, id: 'p2', title: '热门帖' }], boards: [] } }
    });
    // 首页同款工具条（筛选 tab + 发布入口）
    expect(body).toContain('feed-toolbar');
    expect(body).toContain('filter-btn');
    expect(body).toContain('发布内容');
    // 整个页面仍只有一个列表外框（共享 TopicList 自带外框）
    expect((body.match(/class="topic-list[" ]/g) ?? []).length).toBe(1);
    expect(body).toContain('topic-list-head');
    // 右栏推荐位：板块内热门（每分区推荐内容不同）
    expect(body).toContain('推荐内容');
    expect(body).toContain('热门帖');
  });

  it('板块导航侧栏（首页同款分类卡）：全部 + 各板块行（计数 + 当前板块高亮）；空列表不渲染卡片', () => {
    const current: Board = { ...rootBoard, post_count: 6 };
    const other: Board = { ...rootBoard, id: 'b3', slug: 'creative', name: '创意工坊', post_count: 0 };
    const { body } = render(BoardDetailPage, {
      props: { data: { board: current, posts: [], error: null, sort: '', following: false, authed: true, after: null, nextCursor: null, hasMore: false, popular: [], boards: [current, other] } }
    });
    // 首页同款 category-card（aria-label 标识）
    expect(body).toContain('板块分类');
    // 「全部」行 → 首页全部信息流（与首页左栏「全部」同语义）
    expect(body).toContain('href="/"');
    // 当前板块高亮（aria-current + selected；scoped class 前缀在 selected 之前）
    expect(body).toContain('aria-current="page"');
    expect(body).toContain(' selected"');
    // 各板块行：slug 链接 + 主题数（em 计数）
    expect(body).toContain('href="/boards/tech"');
    expect(body).toContain('href="/boards/creative"');
    expect(body).toContain('>6</em>');
    // 导航列表为空（降级态）→ 整卡不渲染
    const empty = render(BoardDetailPage, {
      props: { data: { board: rootBoard, posts: [], error: null, sort: '', following: false, authed: true, after: null, nextCursor: null, hasMore: false, popular: [], boards: [] } }
    });
    expect(empty.body).not.toContain('板块分类');
  });
});

describe('M03-UI-06 标签页 SSR', () => {
  const tags = [
    { id: 't1', slug: 'svelte', name: 'Svelte', description: null, color: '#ff3e00', group_id: 'g1', usage_count: 5 },
    { id: 't2', slug: 'rust', name: 'Rust', description: null, color: null, group_id: null, usage_count: 9 }
  ];
  const groups = [{ id: 'g1', name: '前端', slug: 'frontend', sort_order: 1 }];

  it('标签按分组展示，点击进入标签聚合页', () => {
    const { body } = render(TagsPage, { props: { data: { tags, groups, error: null } } });
    expect(body).toContain('前端');
    expect(body).toContain('Svelte');
    expect(body).toContain('href="/tags/svelte"');
    expect(body).toContain('Rust'); // 未分组进「其他」
    expect(body).toContain('href="/tags/rust"');
  });

  it('空状态可读', () => {
    const { body } = render(TagsPage, { props: { data: { tags: [], groups: [], error: null } } });
    expect(body).toContain('暂无标签');
  });
});
