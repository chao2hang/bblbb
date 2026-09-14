// 登录页独立全屏布局 SSR 测试
// 断言：
// 1. /login 路由根布局不渲染前台 Navbar 与 BottomNav；
// 2. /login 渲染独立全屏认证视口（auth-viewport-main）与 app-shell--auth；
// 3. /login 仍可正常输出主要内容（children）与 Toast 宿主。
import { describe, expect, it, vi } from 'vitest';
import { render } from 'svelte/server';
import { createRawSnippet } from 'svelte';
import RootLayout from '../../../routes/+layout.svelte';
import { resolveSiteCopy } from '$lib/site/copy';

vi.mock('$app/state', () => ({
  page: {
    url: { pathname: '/login', searchParams: new URLSearchParams() },
    data: {}
  }
}));

const dummyChildren = createRawSnippet(() => ({
  render: () => '<div id="login-test-content">登录测试表单</div>'
}));

describe('登录页独立全屏布局系统（无前台 Header 与 BottomNav）', () => {
  it('在 /login 路径下：根布局不渲染前台 Navbar 与 BottomNav', () => {
    const { body } = render(RootLayout, {
      props: {
        children: dummyChildren,
        data: {
          user: null,
          notifications: { unreadCount: 0, recent: [] },
          activeTheme: null,
          site: resolveSiteCopy(null)
        }
      }
    });

    expect(body).toContain('app-shell--auth');
    expect(body).not.toContain('class="navbar"');
    expect(body).not.toContain('class="bottom-nav"');
    expect(body).toContain('auth-viewport-main');
    expect(body).toContain('登录测试表单');
  });
});
