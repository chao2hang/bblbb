import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, cleanup, act } from '@testing-library/svelte';
import NavigationProgress from './NavigationProgress.svelte';

describe('NavigationProgress', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    document.documentElement.removeAttribute('data-navigating');
    document.documentElement.removeAttribute('aria-busy');
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    document.documentElement.removeAttribute('data-navigating');
    document.documentElement.removeAttribute('aria-busy');
  });

  it('初始无导航状态时不渲染进度条 DOM', () => {
    render(NavigationProgress, { props: { testNavigating: null } });
    expect(screen.queryByRole('progressbar')).toBeNull();
    expect(document.documentElement.getAttribute('data-navigating')).toBeNull();
    expect(document.documentElement.getAttribute('aria-busy')).toBeNull();
  });

  it('进入导航状态时立即渲染进度条并设置 ARIA 属性与根元素状态', async () => {
    const { rerender } = render(NavigationProgress, { props: { testNavigating: null } });

    await rerender({ testNavigating: { from: '/', to: '/boards' } });

    const bar = screen.getByRole('progressbar');
    expect(bar).toBeTruthy();
    expect(bar.getAttribute('aria-label')).toBe('页面加载中');
    expect(bar.getAttribute('aria-valuemin')).toBe('0');
    expect(bar.getAttribute('aria-valuemax')).toBe('100');
    expect(Number(bar.getAttribute('aria-valuenow'))).toBeGreaterThanOrEqual(20);

    expect(document.documentElement.getAttribute('data-navigating')).toBe('true');
    expect(document.documentElement.getAttribute('aria-busy')).toBe('true');
  });

  it('导航持续时按 trickle 逻辑步进增加进度值', async () => {
    render(NavigationProgress, {
      props: { testNavigating: { from: '/', to: '/topics' } }
    });

    const bar = screen.getByRole('progressbar');
    const initialVal = Number(bar.getAttribute('aria-valuenow'));
    expect(initialVal).toBe(20);

    // 前进 200ms
    await act(async () => {
      vi.advanceTimersByTime(200);
    });
    const valAfter200 = Number(bar.getAttribute('aria-valuenow'));
    expect(valAfter200).toBeGreaterThan(initialVal);

    // 再次前进 600ms
    await act(async () => {
      vi.advanceTimersByTime(600);
    });
    const valAfter800 = Number(bar.getAttribute('aria-valuenow'));
    expect(valAfter800).toBeGreaterThan(valAfter200);
    expect(valAfter800).toBeLessThanOrEqual(95); // 封顶 95
  });

  it('导航完成时迅速到达 100% 并淡出清理', async () => {
    const { rerender } = render(NavigationProgress, {
      props: { testNavigating: { from: '/', to: '/topics' } }
    });

    const bar = screen.getByRole('progressbar');
    expect(bar).toBeTruthy();

    // 导航完成
    await rerender({ testNavigating: null });

    // 立即满格到 100%
    expect(bar.getAttribute('aria-valuenow')).toBe('100');

    // 150ms 后开始淡出
    await act(async () => {
      vi.advanceTimersByTime(150);
    });
    expect(bar.classList.contains('is-fading')).toBe(true);

    // 200ms 后完全移除 DOM 与属性
    await act(async () => {
      vi.advanceTimersByTime(200);
    });
    expect(screen.queryByRole('progressbar')).toBeNull();
    expect(document.documentElement.getAttribute('data-navigating')).toBeNull();
    expect(document.documentElement.getAttribute('aria-busy')).toBeNull();
  });

  it('组件卸载时正确清理属性与定时器', async () => {
    const { unmount } = render(NavigationProgress, {
      props: { testNavigating: { from: '/', to: '/topics' } }
    });

    expect(document.documentElement.getAttribute('data-navigating')).toBe('true');
    unmount();
    expect(document.documentElement.getAttribute('data-navigating')).toBeNull();
    expect(document.documentElement.getAttribute('aria-busy')).toBeNull();
  });
});
