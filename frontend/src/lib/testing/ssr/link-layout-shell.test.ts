import { describe, expect, it, vi } from 'vitest';
import { render } from 'svelte/server';
import { createRawSnippet } from 'svelte';
import { page } from '$app/state';
import RootLayout from '../../../routes/+layout.svelte';
import LinkPage from '../../../routes/link/+page.svelte';
import { resolveSiteCopy } from '$lib/site/copy';

vi.mock('$app/state', () => {
  const page = {
    url: { pathname: '/link', searchParams: new URLSearchParams() },
    data: {}
  };
  return { page };
});

const dummyChildren = createRawSnippet(() => ({
  render: () => '<div id="link-test-content">跳转确认内容</div>'
}));

describe('外链跳转确认页独立全屏布局系统（无前台 Navbar 与 BottomNav）', () => {
  it('在 /link 路径下：根布局不渲染前台 Navbar 与 BottomNav，渲染独立全屏视口', () => {
    (page as any).url.pathname = '/link';
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
    expect(body).not.toContain('class=\"navbar\"');
    expect(body).not.toContain('class=\"bottom-nav\"');
    expect(body).toContain('link-viewport-main');
    expect(body).toContain('跳转确认内容');
  });

  it('LinkPage 在 SSR 环境下输出完整静态 HTML', () => {
    const { body, head } = render(LinkPage, {
      props: {
        data: {
          target: 'https://www.arsrna.cn'
        }
      }
    });

    // 标题与关键内容
    expect(head).toContain('跳转确认');
    expect(body).toContain('BBLBB');
    expect(body).toContain('即将离开 BBLBB，请注意账号和财产安全');
    expect(body).toContain('https://www.arsrna.cn');
    expect(body).toContain('继续访问');
    expect(body).not.toContain('Tencent');
    expect(body).not.toContain('CNB');
    expect(body).not.toContain('href=\"#');
    expect(body).toContain('返回 BBLBB');
  });
});
