// M00-FRONTEND-07：Toast 屏幕阅读器播报（role=status）与关闭按钮。
import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import Toast from './Toast.svelte';
import { expectSrAnnouncement } from '$lib/testing/a11y';

describe('Toast（屏幕阅读器）', () => {
  it('以 role=status 播报消息', () => {
    const { container } = render(Toast, { message: '已保存成功' });
    expectSrAnnouncement(container, '已保存成功', 'status');
  });

  it('关闭按钮带 aria-label 且触发 onclose', async () => {
    const onclose = vi.fn();
    render(Toast, { message: '已删除', onclose: onclose as never });
    const closeBtn = screen.getByRole('button', { name: '关闭' });
    await userEvent.click(closeBtn);
    expect(onclose).toHaveBeenCalledOnce();
  });

  it('未传 onclose 时不渲染关闭按钮', () => {
    render(Toast, { message: '纯提示' });
    expect(screen.queryByRole('button', { name: '关闭' })).toBeNull();
  });
});