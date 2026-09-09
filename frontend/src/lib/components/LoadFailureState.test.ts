// LoadFailureState：瞬态服务端错误（5xx/429）的页面占位——中性「加载失败 +
// 重试」卡（错误详情走全局 Toast，不整页展示错误态）。
import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import LoadFailureState from './LoadFailureState.svelte';

describe('LoadFailureState（加载失败占位 + 重试）', () => {
  it('默认渲染「加载失败」标题与说明', () => {
    render(LoadFailureState);
    expect(screen.getByText('加载失败')).toBeTruthy();
    expect(screen.getByText(/暂时没有加载出来/)).toBeTruthy();
  });

  it('重试按钮触发 onretry', async () => {
    const onretry = vi.fn();
    render(LoadFailureState, { onretry });
    const btn = screen.getByRole('button', { name: '重试' });
    await userEvent.click(btn);
    expect(onretry).toHaveBeenCalledOnce();
  });

  it('支持自定义标题（如「会话加载失败」）', () => {
    render(LoadFailureState, { title: '会话加载失败' });
    expect(screen.getByText('会话加载失败')).toBeTruthy();
  });

  it('未传 onretry 时不渲染重试按钮', () => {
    render(LoadFailureState);
    expect(screen.queryByRole('button', { name: '重试' })).toBeNull();
  });
});
