// 全局壳（GAP-FIX 第四节）：Navbar 主题切换 / 通知速览下拉 / 更多下拉 /
// 菜单外点与 Escape 关闭 + 全局 Toast store 接线的交互测试。
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { get } from 'svelte/store';
import Navbar from '$lib/components/Navbar.svelte';
import ToastHost from '$lib/components/ui/ToastHost.svelte';
import { toasts, show, dismiss } from '$lib/ui/toast';

vi.mock('$app/state', () => ({
  page: { url: { pathname: '/' }, data: {} }
}));
vi.mock('$app/navigation', () => ({
  goto: vi.fn(),
  invalidate: vi.fn(),
  invalidateAll: vi.fn()
}));

const navUser = {
  username: 'alice',
  display_name: '爱丽丝',
  level: 7,
  roles: ['member']
};

const recentNotifications = [
  {
    id: 'n1',
    type: 'reply',
    title: '有新回复',
    body: null,
    link: '/posts/p1',
    is_read: false,
    created_at: 0,
    read_at: null
  },
  {
    id: 'n2',
    type: 'system',
    title: '社区规范已更新',
    body: null,
    link: null,
    is_read: true,
    created_at: 0,
    read_at: 0
  }
];

beforeEach(() => {
  localStorage.clear();
  document.documentElement.classList.remove('light', 'dark');
});

afterEach(() => {
  cleanup();
  for (const t of get(toasts)) dismiss(t.id);
});

describe('全局壳·主题切换', () => {
  it('点击循环 light → dark → system：写 localStorage + html class + toast 反馈', async () => {
    const user = userEvent.setup();
    render(Navbar, { props: { user: navUser, unread: 0 } });
    const btn = screen.getByRole('button', { name: '切换主题' });

    // 默认 system → 点击切到 light
    await user.click(btn);
    expect(localStorage.getItem('bblbb-theme')).toBe('light');
    expect(document.documentElement.classList.contains('light')).toBe(true);
    expect(document.documentElement.dataset.themePreference).toBe('light');

    // light → dark
    await user.click(btn);
    expect(localStorage.getItem('bblbb-theme')).toBe('dark');
    expect(document.documentElement.classList.contains('dark')).toBe(true);
    expect(btn.getAttribute('aria-pressed')).toBe('true');

    // dark → system（解析回浅色）
    await user.click(btn);
    expect(localStorage.getItem('bblbb-theme')).toBe('system');
    expect(document.documentElement.classList.contains('light')).toBe(true);
    expect(btn.getAttribute('aria-pressed')).toBe('false');
  });

  it('主题切换有全局 Toast 反馈（ToastHost 渲染 store）', async () => {
    const user = userEvent.setup();
    render(ToastHost);
    render(Navbar, { props: { user: navUser } });
    await user.click(screen.getByRole('button', { name: '切换主题' }));
    expect(screen.getAllByText('已切换为亮色主题').length).toBeGreaterThan(0);
    expect(get(toasts).length).toBe(1);
  });
});

describe('全局壳·通知速览下拉', () => {
  it('登录用户：铃铛下拉展示最近通知 + 全部已读 + 查看全部', async () => {
    const user = userEvent.setup();
    const onreadall = vi.fn();
    render(Navbar, {
      props: { user: navUser, unread: 2, notifications: recentNotifications, onreadall }
    });

    await user.click(screen.getByRole('button', { name: '通知' }));
    expect(screen.getByRole('menu', { name: '通知速览' })).toBeTruthy();
    expect(screen.getByText('2 条未读')).toBeTruthy();
    expect(screen.getByText('有新回复')).toBeTruthy();
    expect(screen.getByRole('menuitem', { name: /查看全部/ })).toHaveAttribute('href', '/notifications');

    await user.click(screen.getByRole('button', { name: '全部已读' }));
    expect(onreadall).toHaveBeenCalledOnce();
  });

  it('未读为 0 时不显示「全部已读」按钮；未登录不展示铃铛入口', () => {
    const { rerender } = render(Navbar, {
      props: { user: navUser, unread: 0, notifications: recentNotifications }
    });
    expect(screen.queryByRole('button', { name: '全部已读' })).toBeNull();

    // 未登录投影：通知铃铛（含匿名直达链接）整体不展示。
    rerender({ user: null, unread: 0 });
    expect(screen.queryByRole('link', { name: '通知' })).toBeNull();
    expect(screen.queryByRole('button', { name: '通知' })).toBeNull();
  });

  it('unavailable 通知的速览项跳转到通知中心（不透传失效链接）', async () => {
    const user = userEvent.setup();
    render(Navbar, {
      props: {
        user: navUser,
        unread: 1,
        notifications: [{ ...recentNotifications[0], unavailable: true, link: '/posts/secret' }]
      }
    });
    await user.click(screen.getByRole('button', { name: '通知' }));
    expect(screen.getByRole('menuitem', { name: /有新回复/ })).toHaveAttribute('href', '/notifications');
  });
});

describe('全局壳·更多下拉与菜单关闭', () => {
  it('更多下拉包含商城/市场/成就/下载账单/API 密钥入口', async () => {
    const user = userEvent.setup();
    render(Navbar, { props: { user: navUser } });
    await user.click(screen.getByRole('button', { name: '更多导航' }));
    const menu = screen.getByRole('menu', { name: '更多导航' });
    for (const [label, href] of [
      ['商城', '/shop'],
      ['市场', '/marketplace'],
      ['成就', '/achievements'],
      ['下载账单', '/me/billing'],
      ['API 密钥', '/apikeys']
    ] as const) {
      const link = menu.querySelector(`a[href="${href}"]`);
      expect(link, `${label} → ${href}`).toBeTruthy();
      expect(link!.textContent).toContain(label);
    }
  });

  it('菜单外点击与 Escape 关闭打开的下拉（原型有而前端缺）', async () => {
    const user = userEvent.setup();
    render(Navbar, { props: { user: navUser, unread: 1, notifications: recentNotifications } });
    await user.click(screen.getByRole('button', { name: '通知' }));
    expect(screen.getByRole('menu', { name: '通知速览' })).toBeTruthy();

    // 外点（正文区域，冒泡到 window）→ 关闭
    await fireEvent.click(document.body);
    expect(screen.queryByRole('menu', { name: '通知速览' })).toBeNull();

    // 重新打开后 Escape → 关闭
    await user.click(screen.getByRole('button', { name: '通知' }));
    await fireEvent.keyDown(window, { key: 'Escape' });
    expect(screen.queryByRole('menu', { name: '通知速览' })).toBeNull();
  });

  it('点击触发按钮区域内部不会立即关闭（菜单锚定区域判定）', async () => {
    const user = userEvent.setup();
    render(Navbar, { props: { user: navUser, unread: 1, notifications: recentNotifications } });
    // 打开用户菜单
    await user.click(screen.getByRole('button', { name: '用户菜单' }));
    expect(screen.getByRole('menu', { name: '用户菜单' })).toBeTruthy();
    // 再次点击头像按钮（在 data-nav-menu 区域内）由按钮自身 toggle，不因外点关闭后残留
    await user.click(screen.getByRole('button', { name: '用户菜单' }));
    expect(screen.queryByRole('menu', { name: '用户菜单' })).toBeNull();
  });
});

describe('全局壳·Toast store', () => {
  it('show 自动入列并可手动 dismiss；类型透传', async () => {
    render(ToastHost);
    show('保存成功', 'success');
    show('网络错误', 'danger');
    // store 更新 → DOM 渲染走微任务（effect flush），findByText 轮询等待。
    expect(await screen.findByText('保存成功')).toBeTruthy();
    expect(screen.getByText('网络错误')).toBeTruthy();
    expect(get(toasts).map((t) => t.type)).toEqual(['success', 'danger']);

    dismiss(get(toasts)[0].id);
    await waitFor(() => expect(screen.queryByText('保存成功')).toBeNull());
    expect(get(toasts).length).toBe(1);
  });

  it('重复提示合并，并最多保留四条可见 Toast', () => {
    const first = show('重复错误', 'danger');
    const duplicate = show('重复错误', 'danger');
    expect(duplicate).toBe(first);

    show('提示 1');
    show('提示 2');
    show('提示 3');
    show('提示 4');
    expect(get(toasts).map((toast) => toast.message)).toEqual(['提示 1', '提示 2', '提示 3', '提示 4']);
  });
});

describe('全局壳·未登录菜单投影', () => {
  it('匿名用户：主导航与铃铛/更多不含登录专属入口', () => {
    render(Navbar, { props: { user: null, unread: 0 } });

    // 主导航只留公开项：首页 / 发现（消息为 authOnly）。
    const nav = screen.getByRole('navigation', { name: '主导航' });
    expect(nav.querySelector('a[href="/messages"]')).toBeNull();
    expect(nav.querySelector('a[href="/"]')).toBeTruthy();
    expect(nav.querySelector('a[href="/discover"]')).toBeTruthy();

    // 铃铛（登录按钮 / 匿名直达链接）整体不展示。
    expect(screen.queryByRole('button', { name: '通知' })).toBeNull();
    expect(screen.queryByRole('link', { name: '通知' })).toBeNull();

    // 「更多」下拉里登录专属模块（商城/市场/成就/下载账单/API 密钥）被过滤。
    const more = screen.getByRole('button', { name: '更多导航' });
    expect(more).toBeTruthy();
  });

  it('匿名用户：更多下拉展开后无登录专属入口，公开入口仍在', async () => {
    const user = userEvent.setup();
    render(Navbar, { props: { user: null } });
    await user.click(screen.getByRole('button', { name: '更多导航' }));
    const menu = screen.getByRole('menu', { name: '更多导航' });
    for (const href of ['/shop', '/marketplace', '/achievements', '/me/billing', '/apikeys']) {
      expect(menu.querySelector(`a[href="${href}"]`), `未登录不应出现 ${href}`).toBeNull();
    }
    for (const href of ['/boards', '/tags']) {
      expect(menu.querySelector(`a[href="${href}"]`), `公开入口应保留 ${href}`).toBeTruthy();
    }
  });

  it('登录后：主导航与更多下拉恢复登录专属入口', async () => {
    const user = userEvent.setup();
    render(Navbar, { props: { user: navUser, unread: 0 } });

    expect(screen.getByRole('navigation', { name: '主导航' }).querySelector('a[href="/messages"]'))
      .toBeTruthy();

    await user.click(screen.getByRole('button', { name: '更多导航' }));
    const menu = screen.getByRole('menu', { name: '更多导航' });
    for (const href of ['/shop', '/marketplace', '/achievements', '/me/billing', '/apikeys']) {
      expect(menu.querySelector(`a[href="${href}"]`), `登录后应出现 ${href}`).toBeTruthy();
    }
  });
});

describe('全局壳·移动端半屏用户菜单 (Bottom Sheet)', () => {
  it('展开用户菜单时渲染底栏拉手、背景遮罩层与关闭按钮', async () => {
    const user = userEvent.setup();
    render(Navbar, { props: { user: navUser } });

    // 默认未展开
    expect(screen.queryByRole('menu', { name: '用户菜单' })).toBeNull();
    expect(document.querySelector('.user-sheet-backdrop')).toBeNull();

    // 点击头像按钮展开
    await user.click(screen.getByRole('button', { name: '用户菜单' }));
    const menu = screen.getByRole('menu', { name: '用户菜单' });
    expect(menu).toBeTruthy();

    // 包含 iOS/Android 交互规范的拉手条与半屏遮罩
    expect(menu.querySelector('.user-menu-handle-bar')).not.toBeNull();
    expect(menu.querySelector('.user-menu-handle')).not.toBeNull();
    expect(document.querySelector('.user-sheet-backdrop')).not.toBeNull();

    // 头部包含关闭按钮
    const closeBtn = screen.getByRole('button', { name: '关闭' });
    expect(closeBtn).toBeTruthy();

    // 点击关闭按钮可收起
    await user.click(closeBtn);
    expect(screen.queryByRole('menu', { name: '用户菜单' })).toBeNull();
    expect(document.querySelector('.user-sheet-backdrop')).toBeNull();
  });

  it('点击遮罩层可收起半屏菜单', async () => {
    const user = userEvent.setup();
    render(Navbar, { props: { user: navUser } });

    await user.click(screen.getByRole('button', { name: '用户菜单' }));
    expect(screen.getByRole('menu', { name: '用户菜单' })).toBeTruthy();

    const backdrop = document.querySelector('.user-sheet-backdrop') as HTMLElement;
    expect(backdrop).not.toBeNull();
    await user.click(backdrop);

    expect(screen.queryByRole('menu', { name: '用户菜单' })).toBeNull();
  });
});
