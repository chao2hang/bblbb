import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import TopicList from './TopicList.svelte';
import source from './TopicList.svelte?raw';

const createdAt = Date.now() - 30 * 60 * 1000;

const rows = [
  {
    id: 'post-1',
    title: 'Rust 与 SvelteKit 实践',
    author: 'chaos',
    boardLabel: '技术交流',
    replyCount: 5,
    viewCount: 128,
    likeCount: 2,
    createdAt
  },
  {
    id: 'post-2',
    title: '付费文章：深入 BBLBB 架构',
    author: 'author_wang',
    replyCount: 0,
    viewCount: 42,
    createdAt,
    lastReplyAt: null
  }
];

describe('TopicList', () => {
  it('renders the shared head columns and one TopicRow per entry', () => {
    const { container } = render(TopicList, { props: { rows } });

    expect(container.querySelectorAll('.topic-list-head > span')).toHaveLength(5);
    const participantsHeader = screen.getByText('参与者');
    expect(participantsHeader).toBeInTheDocument();
    expect(participantsHeader).toHaveClass('topic-list-head__participants');
    expect(source).toContain('.topic-list-head__participants');
    expect(source).toContain('text-align: center;');
    expect(screen.getAllByRole('link', { name: /^查看帖子：/ })).toHaveLength(2);
    expect(container.querySelector('.topic-row__views')).toHaveTextContent('128');
    // boardLabel 按行可选：提供的行渲染板块脚注，未提供的行不渲染
    expect(screen.getByTitle('板块：技术交流')).toBeInTheDocument();
  });

  it('omits the board footnote when boardLabel is null (single-board pages)', () => {
    render(TopicList, {
      props: { rows: [{ ...rows[0], boardLabel: null }] }
    });
    expect(screen.queryByTitle(/^板块：/)).toBeNull();
  });

  it('renders the homepage-style empty state with an optional publish CTA', () => {
    render(TopicList, {
      props: {
        rows: [],
        emptyTitle: '暂未匹配帖子',
        emptyDesc: '调整筛选后再试',
        emptyCta: { href: '/editor', label: '发布第一篇讨论' }
      }
    });

    expect(screen.getByText('暂未匹配帖子')).toBeInTheDocument();
    expect(screen.getByText('调整筛选后再试')).toBeInTheDocument();
    expect(screen.getByRole('link', { name: '发布第一篇讨论' })).toHaveAttribute('href', '/editor');
  });

  it('omits the CTA when the host page does not provide one (filtered empty state)', () => {
    render(TopicList, { props: { rows: [], emptyTitle: '没有匹配的帖子' } });
    expect(screen.getByText('没有匹配的帖子')).toBeInTheDocument();
    expect(screen.queryByRole('link')).toBeNull();
  });

  it('frameless + label: container drops its own frame and carries the feed semantics', () => {
    const { container } = render(TopicList, {
      props: { rows, frameless: true, label: '板块帖子列表' }
    });

    const list = container.querySelector('.topic-list');
    expect(list).toHaveClass('topic-list--frameless');
    expect(list).toHaveAttribute('role', 'feed');
    expect(list).toHaveAttribute('aria-label', '板块帖子列表');
  });

  it('keeps the head degradation aligned with the TopicRow mobile breakpoint', () => {
    // 表头整行隐藏必须与 TopicRow 的移动端断点（767px）一致，
    // 避免 640–767px 区间「表头 4 列 + 行已单列」的错位。
    expect(source).toContain('@media (max-width: 767px)');
    expect(source).toContain('@media (max-width: 900px)');
  });
});
