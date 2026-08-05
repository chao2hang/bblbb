// M03-UI-03：用户资料卡交互测试——Hover/Focus 共用浮层、离开延迟、
// Escape 关闭、滚动关闭、隐私 allowlist。
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import UserCard from './UserCard.svelte';

const user = {
  id: 'u1',
  username: 'alice',
  display_name: '爱丽丝',
  bio: '公开简介',
  level: 7,
  avatar_attachment_id: null,
  cover_attachment_id: null,
  signature: '公开签名',
  created_at: 0
};

const adversarial = {
  ...user,
  email: 'alice@example.com',
  status: 'active',
  password_hash: 'USER-CARD-HASH'
};

function renderCard(props: Record<string, unknown> = {}) {
  return render(UserCard, { props: { user, ...props } });
}

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe('M03-UI-03 用户资料卡', () => {
  it('触发元素是链接（无 JS 可跳主页），初始不渲染浮层', () => {
    renderCard();
    const trigger = screen.getByRole('link', { name: '查看 爱丽丝 的个人资料' });
    expect(trigger).toHaveAttribute('href', '/users/alice');
    expect(document.querySelector('.user-card-popover')).toBeNull();
  });

  it('mouseenter 打开浮层，显示公开字段（姓名/LV/简介）', async () => {
    renderCard();
    const trigger = screen.getByRole('link', { name: '查看 爱丽丝 的个人资料' });
    await fireEvent.mouseEnter(trigger);
    const card = document.querySelector('.user-card-popover');
    expect(card).not.toBeNull();
    expect(card).toHaveAttribute('role', 'dialog');
    expect(card!.textContent).toContain('爱丽丝');
    expect(card!.textContent).toContain('LV.7');
    expect(card!.textContent).toContain('公开简介');
  });

  it('mouseleave 延迟关闭（250ms），浮层 mouseenter 取消关闭', async () => {
    renderCard();
    const trigger = screen.getByRole('link', { name: '查看 爱丽丝 的个人资料' });
    await fireEvent.mouseEnter(trigger);
    expect(document.querySelector('.user-card-popover')).not.toBeNull();

    await fireEvent.mouseLeave(trigger);
    // 延迟内仍打开。
    expect(document.querySelector('.user-card-popover')).not.toBeNull();
    // 回到浮层取消关闭。
    await fireEvent.mouseEnter(document.querySelector('.user-card-popover')!);
    await vi.advanceTimersByTimeAsync(500);
    expect(document.querySelector('.user-card-popover')).not.toBeNull();

    // 离开浮层，延迟结束后关闭。
    await fireEvent.mouseLeave(document.querySelector('.user-card-popover')!);
    await vi.advanceTimersByTimeAsync(250);
    expect(document.querySelector('.user-card-popover')).toBeNull();
  });

  it('键盘 Focus 同样打开，blur 延迟关闭', async () => {
    renderCard();
    const trigger = screen.getByRole('link', { name: '查看 爱丽丝 的个人资料' });
    await fireEvent.focus(trigger);
    expect(document.querySelector('.user-card-popover')).not.toBeNull();
    await fireEvent.blur(trigger);
    await vi.advanceTimersByTimeAsync(250);
    expect(document.querySelector('.user-card-popover')).toBeNull();
  });

  it('Escape 立即关闭', async () => {
    renderCard();
    const trigger = screen.getByRole('link', { name: '查看 爱丽丝 的个人资料' });
    await fireEvent.mouseEnter(trigger);
    expect(document.querySelector('.user-card-popover')).not.toBeNull();
    await fireEvent.keyDown(document, { key: 'Escape' });
    expect(document.querySelector('.user-card-popover')).toBeNull();
  });

  it('隐私 allowlist：混入邮箱/状态/凭据也不进入浮层 DOM', async () => {
    render(UserCard, { props: { user: adversarial } });
    const trigger = screen.getByRole('link', { name: '查看 爱丽丝 的个人资料' });
    await fireEvent.focus(trigger);
    const card = document.querySelector('.user-card-popover')!;
    expect(card.textContent).toContain('爱丽丝');
    expect(card.textContent).not.toContain('alice@example.com');
    expect(card.textContent).not.toContain('USER-CARD-HASH');
    expect(card.textContent).not.toContain('active');
  });

  it('触发内容可自定义（children），缺省为头像', () => {
    renderCard();
    const trigger = screen.getByRole('link', { name: '查看 爱丽丝 的个人资料' });
    // 缺省 children → 渲染头像（.avatar）。
    expect(trigger.querySelector('.avatar')).not.toBeNull();
  });
});
