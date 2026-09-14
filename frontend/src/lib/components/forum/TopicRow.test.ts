import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import TopicRow from './TopicRow.svelte';
import source from './TopicRow.svelte?raw';
import { markPostRead } from '$lib/readState.svelte';

const createdAt = Date.now() - 45 * 60 * 1000;

function renderRow(overrides: Record<string, unknown> = {}) {
  return render(TopicRow, {
    props: {
      id: 'post-42',
      title: '如何在小机器上做好 SQLite 并发实践？',
      author: 'chaos',
      boardLabel: '技术交流',
      replyCount: 24,
      viewCount: 3820,
      likeCount: 7,
      createdAt,
      ...overrides
    }
  });
}

describe('TopicRow', () => {
  it('renders the topic, real author avatar, category, and aligned metrics', () => {
    const { container } = renderRow();

    expect(screen.getByRole('link', { name: '查看帖子：如何在小机器上做好 SQLite 并发实践？' })).toHaveAttribute(
      'href',
      '/posts/post-42'
    );
    expect(screen.getByRole('img', { name: 'chaos' })).toBeInTheDocument();
    expect(screen.getByTitle('板块：技术交流')).toBeInTheDocument();
    expect(container.querySelector('.topic-row__replies')).toHaveTextContent('24');
    expect(container.querySelector('.topic-row__views')).toHaveTextContent('3.8k');
    expect(container.querySelector('.topic-row__activity')).toBeInTheDocument();
    const activityLevel = Number(container.querySelector('[data-activity-level]')?.getAttribute('data-activity-level'));
    expect(activityLevel).toBeGreaterThanOrEqual(0.08);
    expect(activityLevel).toBeLessThanOrEqual(1);
  });

  it('renders the board identity icon (not a bare square) in the category chip', () => {
    // 分类徽标 = 板块身份图标 + 身份色（与左栏分类卡同源解析），
    // 不再是无语义的品牌色方块（回归守卫）。
    const { container } = renderRow({ boardSlug: 'general', boardIcon: 'message-circle' });

    const mark = container.querySelector('.topic-row__board-mark');
    expect(mark).not.toBeNull();
    expect(mark?.querySelector('svg.icon-message-circle')).not.toBeNull();
    expect(mark?.getAttribute('style')).toContain('rgb(241, 89, 42)');
    // 回归守卫：board-mark 是图标容器（inline-flex），不再是无语义实心方块
    expect(source).toMatch(/\.topic-row__board-mark \{[^}]*display: inline-flex/);
    expect(source).not.toMatch(/\.topic-row__board-mark \{[^}]*background:/);
  });

  it('falls back to slug/default board visuals when no persisted icon is set', () => {
    const { container } = renderRow({ boardSlug: 'tech' });
    // tech 无持久化 icon → slug 映射 code + 身份色。
    expect(container.querySelector('.topic-row__board-mark svg.icon-code')).not.toBeNull();

    const unknown = renderRow({ boardSlug: 'water', boardIcon: null });
    // 未收录 slug → 默认视觉（message-square）。
    expect(unknown.container.querySelector('.topic-row__board-mark svg.icon-message-square')).not.toBeNull();
  });

  it('renders a bounded, deduplicated real participant stack when provided', () => {
    const { container } = renderRow({
      participants: [
        { id: 'u1', username: 'chaos' },
        { id: 'u2', username: 'nina' },
        { id: 'u3', username: 'lin' },
        { id: 'u4', username: 'dave' }
      ]
    });

    // 楼主 (chaos, u1去重) + nina + lin + dave = 4 人（不超过 5 个，全量展示且无省略号）
    expect(container.querySelectorAll('.topic-row__avatar')).toHaveLength(4);
    expect(container.querySelector('.topic-row__more')).toBeNull();
    expect(container.querySelector('.topic-row__ellipsis')).toBeNull();
    expect(container.querySelector('.topic-row__participants')).toHaveAttribute(
      'aria-label',
      '参与者：chaos、nina、lin、dave'
    );
  });

  it('displays up to 5 participants without ellipsis when there are exactly 5', () => {
    const { container } = renderRow({
      participants: [
        { id: 'u2', username: 'nina' },
        { id: 'u3', username: 'lin' },
        { id: 'u4', username: 'dave' },
        { id: 'u5', username: 'eve' }
      ]
    });

    // 楼主 (chaos) + 4 名回复者 = 共 5 个参与者，最多显示 5 个头像，未超 5 个不显示省略号
    expect(container.querySelectorAll('.topic-row__avatar')).toHaveLength(5);
    expect(container.querySelector('.topic-row__more')).toBeNull();
    expect(container.querySelector('.topic-row__ellipsis')).toBeNull();
    expect(container.querySelector('.topic-row__participants')).toHaveAttribute(
      'aria-label',
      '参与者：chaos、nina、lin、dave、eve'
    );
  });

  it('displays at most 5 avatars and an ellipsis symbol after the last avatar when exceeding 5 participants', () => {
    const { container } = renderRow({
      participants: [
        { id: 'u2', username: 'nina' },
        { id: 'u3', username: 'lin' },
        { id: 'u4', username: 'dave' },
        { id: 'u5', username: 'eve' },
        { id: 'u6', username: 'frank' },
        { id: 'u7', username: 'grace' }
      ]
    });

    // 楼主 + 6 名回复者 = 共 7 人，超过 5 个后最多显示 5 个头像，最后头像后显示省略符号
    const avatars = container.querySelectorAll('.topic-row__avatar');
    expect(avatars).toHaveLength(5);
    const ellipsis = container.querySelector('.topic-row__ellipsis');
    expect(ellipsis).not.toBeNull();
    expect(ellipsis).toHaveTextContent('…');
    expect(ellipsis).toHaveClass('topic-row__more');
    expect(ellipsis).toHaveAttribute('aria-hidden', 'true');
    expect(container.querySelector('.topic-row__participants')).toHaveAttribute(
      'aria-label',
      '参与者：chaos、nina、lin、dave、eve 等'
    );
  });

  it('renders decorated participants as square cosmetic avatars', () => {
    const { container } = renderRow({
      participants: [
        {
          id: 'u2',
          username: 'nina',
          display_name: '妮娜',
          presentation_tokens: { avatar_frame: 'glow' }
        }
      ]
    });

    expect(container.querySelector('.topic-row__participant-avatar')).toBeInTheDocument();
    expect(container.querySelector('.cosmetic-avatar.has-frame')).toBeInTheDocument();
    expect(source).toContain('border-radius: 0');
    expect(source).not.toContain('border-radius: 4px');
  });

  it('keeps frame-equipped avatars in the plain DOM-order stack (ring tucks under the next avatar)', () => {
    // 产品约定：金色帧环与头像本体同层——-7px 叠加保留、下一个头像照常
    // 盖在帧环上，不做任何抬升/间隙（回归守卫：不得再引入帧环专用层级）。
    expect(source).toContain('.topic-row__avatar + .topic-row__avatar');
    expect(source).not.toMatch(/:has\(:global\(\.cosmetic-avatar\.has-frame\)\)/);
  });

  it('renders the equipped avatar frame on the topic author (楼主) avatar too', () => {
    const { container } = renderRow({
      authorUsername: 'chaos',
      authorPresentation: { avatar_frame: 'gold_ring' }
    });

    // 参与者列第一个头像是楼主：已装备头像框必须随楼主头像渲染（与悬浮
    // 资料卡/帖子页同源），未装备的参与者不受影响。
    const avatars = container.querySelectorAll('.topic-row__avatar');
    expect(avatars.length).toBeGreaterThanOrEqual(1);
    expect(avatars[0].querySelector('.cosmetic-avatar.has-frame')).toBeInTheDocument();
    expect(
      avatars[0].querySelector('.cosmetic-avatar__fallback-frame.avatar-frame-gold')
    ).not.toBeNull();
  });

  it('keeps the author avatar undecorated when no presentation tokens are provided', () => {
    const { container } = renderRow({ authorUsername: 'chaos' });

    const avatars = container.querySelectorAll('.topic-row__avatar');
    expect(avatars[0].querySelector('.cosmetic-avatar.has-frame')).toBeNull();
  });

  it('renders uploaded avatar images for author and participants', () => {
    const { container } = renderRow({
      authorUsername: 'chaos',
      authorAvatarAttachmentId: 'att-chaos-avatar',
      participants: [
        {
          id: 'u2',
          username: 'nina',
          display_name: '妮娜',
          avatar_attachment_id: 'att-nina-avatar'
        }
      ]
    });

    // 上传过头像的用户直接渲染图片（内容经 /attachments/{id} 稳定端点），
    // 未上传的参与者保持首字母占位。
    const imgs = [...container.querySelectorAll('.topic-row__participants img')];
    expect(imgs).toHaveLength(2);
    expect(imgs[0].getAttribute('src')).toContain('/attachments/att-chaos-avatar');
    expect(imgs[1].getAttribute('src')).toContain('/attachments/att-nina-avatar');
    expect(screen.getByRole('img', { name: '妮娜' })).toBeInTheDocument();
  });

  it('prefers display names and drops participants without any public name', () => {
    const { container } = renderRow({
      participants: [
        { id: 'u2', username: 'nina', display_name: '妮娜' },
        { id: 'u3', username: null, display_name: '  ' },
        { id: 'u4', username: null, display_name: null },
        { id: 'u5', username: 'lin' }
      ]
    });

    // 楼主 + 有公开名的参与者（昵称优先）= 3；全空名的参与者（如已注销）剔除。
    expect(container.querySelectorAll('.topic-row__avatar')).toHaveLength(3);
    expect(screen.getByRole('img', { name: '妮娜' })).toBeInTheDocument();
    expect(container.querySelector('.topic-row__participants')).toHaveAttribute(
      'aria-label',
      '参与者：chaos、妮娜、lin'
    );
  });

  it('wraps avatars that carry an account username in profile hover-card triggers', () => {
    const { container } = renderRow({
      authorUsername: 'wang',
      participants: [{ id: 'u2', username: 'nina', display_name: '妮娜' }]
    });

    // 有账号的头像 = UserCard 触发链接（hover/focus 出资料卡，点击进主页）。
    const triggers = container.querySelectorAll('.topic-row__participants a[href]');
    expect(triggers).toHaveLength(2);
    expect(container.querySelector('a[href="/users/wang"]')).toHaveAttribute(
      'aria-label',
      '查看 chaos 的个人资料'
    );
    expect(container.querySelector('a[href="/users/nina"]')).toHaveAttribute(
      'aria-label',
      '查看 妮娜 的个人资料'
    );
    expect(screen.getByRole('img', { name: '妮娜' })).toBeInTheDocument();
  });

  it('keeps a plain avatar when no account username is available (anonymous)', () => {
    const { container } = renderRow({ author: '匿名' });

    expect(container.querySelector('.topic-row__participants a')).toBeNull();
    expect(screen.getByRole('img', { name: '匿名' })).toBeInTheDocument();
  });

  it('keeps pinned and featured states textual', () => {
    renderRow({ pinned: true, featured: true });

    expect(screen.getByText('置顶')).toBeInTheDocument();
    expect(screen.getByText('精华')).toBeInTheDocument();
  });

  it('falls back to created time when there is no reply activity', () => {
    renderRow({ lastReplyAt: null });

    const activity = screen.getByLabelText(/最近活动/);
    expect(activity.textContent).toContain('45 分钟前');
  });

  it('contains a reduced-motion fallback and no fabricated participant names', () => {
    expect(source).toContain('@media (prefers-reduced-motion: reduce)');
    expect(source).toContain('@media (max-width: 900px)');
    expect(source).not.toContain('user-2');
    expect(source).not.toContain('user-3');
  });

  it('已读帖子（markPostRead 后）色条切换为暗色（is-read）', () => {
    markPostRead('post-read-1');
    const { container } = renderRow({ id: 'post-read-1' });
    expect(container.querySelector('.topic-row')).toHaveClass('is-read');
  });

  it('未读帖子不带 is-read（保持亮色信号）', () => {
    const { container } = renderRow({ id: 'post-unread-1' });
    expect(container.querySelector('.topic-row')).not.toHaveClass('is-read');
  });

  it('色条样式同时包含未读亮态与已读暗态（源码回归）', () => {
    expect(source).toContain('opacity: 0.9');
    expect(source).toContain('.topic-row.is-read::before');
  });
});
