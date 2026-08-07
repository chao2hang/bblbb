// M07-UI-07：ReactionBar 组件测试——选择/撤销、429 限流提示、403 目标权限
// 错误、未登录提示与键盘可达。
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, waitFor } from '@testing-library/svelte';
import ReactionBar from './ReactionBar.svelte';
import * as client from '$lib/api/client';

vi.mock('$lib/api/client', () => ({
  addPostReaction: vi.fn(),
  removePostReaction: vi.fn(),
  addCommentReaction: vi.fn(),
  removeCommentReaction: vi.fn()
}));

const mocked = client as unknown as {
  addPostReaction: ReturnType<typeof vi.fn>;
  removePostReaction: ReturnType<typeof vi.fn>;
};

const baseProps = {
  targetType: 'post' as const,
  targetId: 'post-1',
  reactions: [
    { reaction: '👍', count: 3, active: false },
    { reaction: '🎉', count: 1, active: true }
  ]
};

beforeEach(() => vi.clearAllMocks());

describe('M07-UI-07 ReactionBar', () => {
  it('点击未激活反应 → POST 添加；计数增加且按钮激活', async () => {
    mocked.addPostReaction.mockResolvedValueOnce({ reaction: '👍', active: true, count: 4 });
    const { getByRole } = render(ReactionBar, { props: baseProps });
    const button = getByRole('button', { name: /👍/ });
    expect(button.getAttribute('aria-pressed')).toBe('false');
    await fireEvent.click(button);
    await waitFor(() => expect(mocked.addPostReaction).toHaveBeenCalledWith(expect.anything(), 'post-1', '👍'));
    expect(button.getAttribute('aria-pressed')).toBe('true');
  });

  it('点击已激活反应 → DELETE 撤销；计数回退', async () => {
    mocked.removePostReaction.mockResolvedValueOnce(undefined);
    const { getByRole } = render(ReactionBar, { props: baseProps });
    const button = getByRole('button', { name: /🎉/ });
    expect(button.getAttribute('aria-pressed')).toBe('true');
    await fireEvent.click(button);
    await waitFor(() => expect(mocked.removePostReaction).toHaveBeenCalledWith(expect.anything(), 'post-1', '🎉'));
    expect(button.getAttribute('aria-pressed')).toBe('false');
  });

  it('429 → 显示限流提示并禁用按钮（冷却期）', async () => {
    mocked.addPostReaction.mockRejectedValueOnce({ status: 429, detail: 'rate', retry_after: 60 });
    const { getByRole, findByRole } = render(ReactionBar, { props: baseProps });
    await fireEvent.click(getByRole('button', { name: /👍/ }));
    const alert = await findByRole('alert');
    expect(alert.textContent).toContain('操作过于频繁');
    expect(alert.textContent).toContain('60 秒');
    expect(getByRole('button', { name: /👍/ }).hasAttribute('disabled')).toBe(true);
  });

  it('403 → 目标权限错误提示', async () => {
    mocked.addPostReaction.mockRejectedValueOnce({ status: 403, detail: 'forbidden' });
    const { getByRole, findByRole } = render(ReactionBar, { props: baseProps });
    await fireEvent.click(getByRole('button', { name: /👍/ }));
    const alert = await findByRole('alert');
    expect(alert.textContent).toContain('你没有权限');
  });

  it('未登录 → 提示登录且不发请求', async () => {
    const { getByRole, findByRole } = render(ReactionBar, { props: { ...baseProps, authed: false } });
    await fireEvent.click(getByRole('button', { name: /👍/ }));
    const alert = await findByRole('alert');
    expect(alert.textContent).toContain('请先登录');
    expect(mocked.addPostReaction).not.toHaveBeenCalled();
  });

  it('反应按钮为原生 button（键盘 Enter/Space 可用），含通知偏好提示链接', async () => {
    const { container } = render(ReactionBar, { props: baseProps });
    expect(container.querySelectorAll('button.reaction-btn').length).toBe(2);
    expect(container.textContent).toContain('通知设置');
    expect(container.querySelector('a[href="/notifications"]')).not.toBeNull();
  });
});
