// M14-COMPONENTS-01/04：可访问基础组件测试（Input/Select/Table/Pagination/
// Dialog/DangerConfirm/AccountingConfirm）。
import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import Input from './Input.svelte';
import Select from './Select.svelte';
import Table from './Table.svelte';
import Pagination from './Pagination.svelte';
import Dialog from './Dialog.svelte';
import DangerConfirm from './DangerConfirm.svelte';
import AccountingConfirm from './AccountingConfirm.svelte';
import { expectErrorAssociation, expectFocusedOn } from '$lib/testing/a11y';

describe('Input（表单关联）', () => {
  it('label[for] 关联控件，hint 经 aria-describedby 关联', () => {
    render(Input, { label: '用户名', hint: '4-20 个字符', id: 'username' });
    const input = screen.getByLabelText('用户名');
    expect(input).toHaveAttribute('aria-describedby', 'username-hint');
    const hint = document.getElementById('username-hint');
    expect(hint).toHaveTextContent('4-20 个字符');
  });

  it('error 时 aria-invalid=true 且错误经 role=alert 关联', () => {
    render(Input, { label: '邮箱', error: '邮箱格式不正确', id: 'email' });
    const input = screen.getByLabelText('邮箱');
    expect(input).toHaveAttribute('aria-invalid', 'true');
    expect(input).toHaveAttribute('aria-describedby', 'email-error');
    const errorEl = document.getElementById('email-error');
    expect(errorEl).toHaveAttribute('role', 'alert');
    expect(errorEl).toHaveTextContent('邮箱格式不正确');
    expectErrorAssociation(input as HTMLElement, '邮箱格式不正确');
  });
});

describe('Select（原生 select 可访问）', () => {
  it('label/option 可读，占位选项禁用', () => {
    render(Select, {
      label: '板块',
      id: 'board',
      placeholder: '请选择板块',
      options: [
        { value: 'general', label: '综合讨论' },
        { value: 'tech', label: '技术分享' }
      ]
    });
    const select = screen.getByLabelText('板块');
    expect(select).toBeInstanceOf(HTMLSelectElement);
    const options = select.querySelectorAll('option');
    expect(options).toHaveLength(3);
    expect(options[0]).toBeDisabled(); // placeholder
    expect(options[0]).toHaveTextContent('请选择板块');
  });

  it('error 关联（aria-invalid + role=alert）', () => {
    render(Select, { label: '板块', error: '请选择板块', id: 'board', options: [] });
    expect(screen.getByLabelText('板块')).toHaveAttribute('aria-invalid', 'true');
    const errorEl = document.getElementById('board-error');
    expect(errorEl).toHaveTextContent('请选择板块');
  });
});

describe('Table（语义表格）', () => {
  it('caption + th scope=col 表头', () => {
    render(Table, {
      caption: '订单列表',
      columns: [{ label: '订单号' }, { label: '金额', align: 'right' }]
    });
    const table = screen.getByRole('table');
    expect(table).toBeInTheDocument();
    const caption = table.querySelector('caption');
    expect(caption).toHaveTextContent('订单列表');
    const headers = table.querySelectorAll('th');
    expect(headers).toHaveLength(2);
    expect(headers[0]).toHaveAttribute('scope', 'col');
  });

  it('无数据时显示空态文案', () => {
    render(Table, { columns: [{ label: '订单号' }], emptyText: '暂无订单' });
    expect(screen.getByText('暂无订单')).toBeInTheDocument();
  });
});

describe('Pagination（分页可访问）', () => {
  it('当前页带 aria-current="page"，nav 有 aria-label', () => {
    render(Pagination, {
      label: '搜索结果分页',
      prevHref: '/search?q=x&page=1',
      nextHref: '/search?q=x&page=3',
      pages: [
        { href: '/search?q=x&page=1', label: '1' },
        { href: '/search?q=x&page=2', label: '2', current: true },
        { href: '/search?q=x&page=3', label: '3' }
      ]
    });
    const nav = screen.getByRole('navigation', { name: '搜索结果分页' });
    const current = nav.querySelector('a[aria-current="page"]');
    expect(current).toHaveTextContent('2');
    expect(nav.querySelector('a[aria-label="上一页"]')).toHaveTextContent('‹');
    expect(nav.querySelector('a[aria-label="下一页"]')).toHaveTextContent('›');
  });

  it('点击页码回调 onchange', async () => {
    const onchange = vi.fn();
    render(Pagination, {
      pages: [{ href: '/search?q=x&page=1', label: '1' }],
      onchange: onchange as never
    });
    await userEvent.click(screen.getByRole('link', { name: '1' }));
    expect(onchange).toHaveBeenCalledOnce();
  });
});

describe('Dialog（模态焦点管理）', () => {
  it('打开时角色 dialog + aria-modal + labelledby', () => {
    render(Dialog, { open: true, title: '确认删除' });
    const dialog = screen.getByRole('dialog');
    expect(dialog).toHaveAttribute('aria-modal', 'true');
    const labelledby = dialog.getAttribute('aria-labelledby')!;
    const titleEl = document.getElementById(labelledby);
    expect(titleEl).toHaveTextContent('确认删除');
  });

  it('打开时焦点移入 dialog 内首个可聚焦元素（关闭按钮）', async () => {
    render(Dialog, { open: true, title: '设置' });
    const closeBtn = screen.getByRole('button', { name: '关闭' });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expectFocusedOn(closeBtn);
  });

  it('Escape 触发 onclose', async () => {
    const onclose = vi.fn();
    render(Dialog, { open: true, title: '提示', onclose: onclose as never });
    await userEvent.keyboard('{Escape}');
    expect(onclose).toHaveBeenCalledOnce();
  });

  it('关闭后焦点回到触发元素（focus return）', async () => {
    const trigger = document.createElement('button');
    trigger.textContent = '打开弹窗';
    document.body.appendChild(trigger);
    const onclose = vi.fn();
    const { rerender } = render(Dialog, { open: false, title: '提示', onclose: onclose as never });
    trigger.focus();
    // 打开：焦点应移入 dialog。
    await rerender({ open: true, title: '提示', onclose: onclose as never });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(screen.getByRole('dialog').contains(document.activeElement)).toBe(true);
    // 关闭：焦点应回到触发元素。
    await rerender({ open: false, title: '提示', onclose: onclose as never });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(document.activeElement).toBe(trigger);
    trigger.remove();
  });

  it('关闭按钮带 aria-label', () => {
    render(Dialog, { open: true, title: '提示', closeLabel: '关闭对话框' });
    expect(screen.getByRole('button', { name: '关闭对话框' })).toBeInTheDocument();
  });
});

describe('DangerConfirm（危险操作确认）', () => {
  it('确认按钮 danger 变体，取消按钮触发 oncancel', async () => {
    const onconfirm = vi.fn();
    const oncancel = vi.fn();
    render(DangerConfirm, {
      open: true,
      title: '删除帖子',
      description: '删除后不可恢复',
      confirmText: '删除',
      onconfirm: onconfirm as never,
      oncancel: oncancel as never
    });
    const dialog = screen.getByRole('dialog');
    expect(dialog).toHaveTextContent('删除后不可恢复');
    await userEvent.click(screen.getByRole('button', { name: '取消' }));
    expect(oncancel).toHaveBeenCalledOnce();
    await userEvent.click(screen.getByRole('button', { name: '删除' }));
    expect(onconfirm).toHaveBeenCalledOnce();
  });

  it('busy 时按钮禁用并显示进行中文案', () => {
    render(DangerConfirm, {
      open: true,
      title: '删除',
      busy: true,
      busyText: '删除中…',
      confirmText: '删除'
    });
    expect(screen.getByRole('button', { name: '删除中…' })).toBeDisabled();
    expect(screen.getByRole('button', { name: '取消' })).toBeDisabled();
  });

  it('服务端错误经 role=alert 播报', () => {
    render(DangerConfirm, { open: true, title: '删除', error: '操作冲突，请重试' });
    const alert = screen.getByRole('alert');
    expect(alert).toHaveTextContent('操作冲突，请重试');
  });
});

describe('AccountingConfirm（账务确认）', () => {
  it('显示金额/手续费/支付后余额', () => {
    render(AccountingConfirm, {
      open: true,
      title: '确认购买',
      amount: 120,
      fee: 2,
      balanceAfter: 380,
      currency: 'B币'
    });
    const dialog = screen.getByRole('dialog');
    expect(dialog).toHaveTextContent('120 B币');
    expect(dialog).toHaveTextContent('2 B币');
    expect(dialog).toHaveTextContent('380 B币');
  });

  it('确认按钮文案可定制', () => {
    render(AccountingConfirm, { open: true, title: '确认', confirmText: '立即支付', amount: 5 });
    expect(screen.getByRole('button', { name: '立即支付' })).toBeInTheDocument();
  });
});
