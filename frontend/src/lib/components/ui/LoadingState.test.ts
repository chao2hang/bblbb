// M00-FRONTEND-07：LoadingState 屏幕阅读器播报与减少动效。
import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/svelte';
import { compile } from 'svelte/compiler';
import LoadingState from './LoadingState.svelte';
import LoadingStateSource from './LoadingState.svelte?raw';
import { expectSrAnnouncement } from '$lib/testing/a11y';

describe('LoadingState（屏幕阅读器 / 减少动效）', () => {
  it('以 role=status aria-live=polite 播报加载状态', () => {
    const { container } = render(LoadingState, { title: '加载中…' });
    const region = container.querySelector('[role="status"]');
    expect(region).not.toBeNull();
    expect(region).toHaveAttribute('aria-live', 'polite');
    expectSrAnnouncement(container, '加载中…', 'status');
  });

  it('支持自定义标题与描述', () => {
    const { container } = render(LoadingState, { title: '正在拉取帖子', desc: '请稍候' });
    expect(container.textContent).toContain('正在拉取帖子');
    expect(container.textContent).toContain('请稍候');
  });

  it('组件 CSS 包含 prefers-reduced-motion 降级（停止旋转动画）', () => {
    const { css } = compile(LoadingStateSource, { generate: 'client', css: 'external' });
    expect(css?.code ?? '').toContain('prefers-reduced-motion');
    expect(css?.code ?? '').toContain('animation: none');
  });
});