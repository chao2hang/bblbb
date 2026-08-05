// M00-FRONTEND-07：Button 键盘激活与焦点行为。
import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import Button from './Button.svelte';
import { expectFocusedOn, pressKey, tabOrder } from '$lib/testing/a11y';

describe('Button（键盘与焦点）', () => {
  it('渲染按钮文本', () => {
    render(Button, { text: '发布' });
    expect(screen.getByRole('button', { name: '发布' })).toBeInTheDocument();
  });

  it('点击触发 onclick', async () => {
    const onclick = vi.fn();
    render(Button, { text: '保存', onclick: onclick as never });
    await userEvent.click(screen.getByRole('button', { name: '保存' }));
    expect(onclick).toHaveBeenCalledOnce();
  });

  it('Enter 键激活按钮（键盘夹具）', async () => {
    const onclick = vi.fn();
    render(Button, { text: '确定', onclick: onclick as never });
    await pressKey(screen.getByRole('button', { name: '确定' }), 'Enter');
    expect(onclick).toHaveBeenCalledOnce();
  });

  it('Space 键激活按钮（键盘夹具）', async () => {
    const onclick = vi.fn();
    render(Button, { text: '确定', onclick: onclick as never });
    await pressKey(screen.getByRole('button', { name: '确定' }), ' ');
    expect(onclick).toHaveBeenCalledOnce();
  });

  it('disabled 按钮不触发 onclick', async () => {
    const onclick = vi.fn();
    render(Button, { text: '禁用', disabled: true, onclick: onclick as never });
    const btn = screen.getByRole('button', { name: '禁用' });
    expect(btn).toBeDisabled();
    await userEvent.click(btn);
    expect(onclick).not.toHaveBeenCalled();
  });

  it('链接形态渲染 <a href>，disabled 时 aria-disabled=true', () => {
    render(Button, { text: '回到首页', href: '/', disabled: true });
    const link = screen.getByRole('link', { name: '回到首页' });
    expect(link).toHaveAttribute('href', '/');
    expect(link).toHaveAttribute('aria-disabled', 'true');
  });

  it('Tab 后焦点落在按钮上（焦点夹具）', async () => {
    render(Button, { text: '下一步' });
    const btn = screen.getByRole('button', { name: '下一步' });
    await userEvent.tab();
    expectFocusedOn(btn);
  });

  it('多个按钮的 Tab 遍历序与渲染顺序一致', async () => {
    render(Button, { text: '一' });
    render(Button, { text: '二' });
    const first = screen.getByRole('button', { name: '一' });
    const second = screen.getByRole('button', { name: '二' });
    const order = await tabOrder(document.body);
    expect(order).toEqual([first, second]);
  });
});