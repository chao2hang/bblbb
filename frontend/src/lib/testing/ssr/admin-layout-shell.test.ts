// 管理后台独立布局系统 SSR 测试
// 断言：
// 1. /admin 路由不渲染前台 Navbar 与 BottomNav；
// 2. /admin 渲染独立后台顶栏（Topbar）、侧栏导航（Admin Nav）、用户角色卡与退出按钮；
// 3. / 路由正常渲染前台 Navbar 与 BottomNav。
import { describe, expect, it, vi } from 'vitest';
import { render } from 'svelte/server';
import { createRawSnippet } from 'svelte';
import RootLayout from '../../../routes/+layout.svelte';
import AdminLayout from '../../../routes/admin/+layout.svelte';
import { resolveSiteCopy } from '$lib/site/copy';
import type { User } from '$lib/api/types';

vi.mock('$app/state', () => ({
  page: {
    url: { pathname: '/admin', searchParams: new URLSearchParams() },
    data: {}
  }
}));

const mockUser = {
  id: 'u1',
  username: 'chaos',
  display_name: 'Chaos',
  roles: ['administrator', 'admin'],
  avatar_attachment_id: null,
  level: 1,
  version: 1,
  email: 'chaos@example.com',
  email_verified: true,
  status: 'active' as const
} as unknown as User;

const dummyChildren = createRawSnippet(() => ({ render: () => '<div id="admin-test-page">仪表盘内容</div>' }));

describe('管理后台独立布局系统（无前台 Header）', () => {
  it('在 /admin 路径下：根布局不渲染前台 Navbar 与 BottomNav', () => {
    const { body } = render(RootLayout, {
      props: {
        children: dummyChildren,
        data: {
          user: mockUser,
          notifications: { unreadCount: 0, recent: [] },
          activeTheme: null,
          site: resolveSiteCopy(null)
        }
      }
    });

    expect(body).toContain('class="app-shell app-shell--admin"');
    expect(body).not.toContain('class="navbar"');
    expect(body).not.toContain('class="bottom-nav"');
    expect(body).toContain('admin-viewport-main');
    expect(body).toContain('仪表盘内容');
  });

  it('后台布局 AdminLayout：渲染独立侧栏、独立顶栏、面包屑与用户卡', () => {
    const { body } = render(AdminLayout, {
      props: {
        children: dummyChildren,
        data: {
          user: mockUser,
          notifications: { unreadCount: 0, recent: [] },
          activeTheme: null,
          site: resolveSiteCopy(null)
        }
      }
    });

    // 独立侧栏与品牌
    expect(body).toContain('app-admin-side');
    expect(body).toContain('BBLBB Admin');
    expect(body).toContain('CONTROL ROOM');
    expect(body).toContain('LIVE');

    // 侧栏导航项
    expect(body).toContain('仪表盘');
    expect(body).toContain('用户管理');
    expect(body).toContain('主题管理');
    expect(body).toContain('系统设置');

    // 后台专属 Topbar 与功能操作
    expect(body).toContain('app-admin-topbar');
    expect(body).toContain('app-admin-breadcrumb');
    expect(body).toContain('切换主题偏好');
    expect(body).toContain('通知中心');
    expect(body).toContain('返回前台');
    expect(body).toContain('app-admin-user');
    expect(body).toContain('超级管理员');
    expect(body).toContain('Chaos');
    expect(body).toContain('退出登录');
  });
});
