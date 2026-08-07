// M08-UI-06：挑战门禁 a11y 交互测试——键盘、屏幕阅读器、触屏与失败回退。
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import ChallengeGate from './ChallengeGate.svelte';
import { expectSrAnnouncement, fireTouchStart, fireTouchEnd } from '$lib/testing/a11y';

beforeEach(() => {
  cleanup();
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe('M08-UI-06 ChallengeGate', () => {
  it('屏幕阅读器：role=alert 播报限流文案', () => {
    const { container } = render(ChallengeGate, {
      props: { title: '访问频率过高', retryAfterSecs: null }
    });
    expectSrAnnouncement(container, '访问频率过高', 'alert');
    expect(container.querySelector('[aria-live="assertive"]')).not.toBeNull();
  });

  it('键盘：Enter 触发「重新搜索」按钮并调用 onRetry', async () => {
    const onRetry = vi.fn();
    const { getByRole } = render(ChallengeGate, { props: { onRetry } });
    const button = getByRole('button', { name: '重新搜索' });
    button.focus();
    await userEvent.keyboard('{Enter}');
    expect(onRetry).toHaveBeenCalledTimes(1);
  });

  it('触屏：触摸点击同样触发 onRetry（不依赖 hover）', () => {
    const onRetry = vi.fn();
    const { getByRole } = render(ChallengeGate, { props: { onRetry } });
    const button = getByRole('button', { name: '重新搜索' });
    fireTouchStart(button);
    fireTouchEnd(button);
    fireEvent.click(button);
    expect(onRetry).toHaveBeenCalled();
  });

  it('挑战入口为普通链接（无 JS 可点击，nofollow）', () => {
    const { container } = render(ChallengeGate, {
      props: { challengeUrl: '/challenge/xyz', onRetry: null }
    });
    const link = container.querySelector<HTMLAnchorElement>('a[href="/challenge/xyz"]');
    expect(link).not.toBeNull();
    expect(link?.getAttribute('rel')).toContain('nofollow');
    expect(container.textContent).toContain('完成验证');
  });

  it('失败回退：始终提供返回搜索首页的普通链接', () => {
    const { container } = render(ChallengeGate, { props: { challengeUrl: null, onRetry: null } });
    const fallback = container.querySelector<HTMLAnchorElement>('a[href="/search"]');
    expect(fallback).not.toBeNull();
    expect(container.textContent).toContain('不会解除服务端的内容授权边界');
  });

  it('Retry-After 倒计时：初始显示秒数，随后递减（role=status）', async () => {
    vi.useFakeTimers();
    try {
      const { container } = render(ChallengeGate, { props: { retryAfterSecs: 5 } });
      expect(container.textContent).toContain('可在约 5 秒后重试');
      await vi.advanceTimersByTimeAsync(2000);
      expect(container.textContent).toContain('可在约 3 秒后重试');
      await vi.advanceTimersByTimeAsync(4000);
      expect(container.textContent).toContain('可重试');
    } finally {
      vi.useRealTimers();
    }
  });
});
