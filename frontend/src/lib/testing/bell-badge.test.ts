// 回归：通知铃铛角标（bug「通知已读之后，铃铛右上角的角标不会消失或减少」）。
//
// 根因：徽标数原先是根布局的组件局部态，仅「路由变化触发会话刷新」与「铃铛下拉
// 『全部已读』」两条路径更新；/notifications 页的「标为已读 / 全部已读」只改页面
// 局部态，停留在该页时角标不减少，要等下一次切路由才纠正。
// 修复：徽标态移入共享模块 bellState.svelte（模块级 $state 单例），通知页在已读
// 操作成功后同步写入；根布局在 $derived 中经 getter 读取（跨模块响应式跟踪）。
//
// 本用例在同一模块图内渲染真实根布局 + 真实通知页（jsdom，stub 全局 fetch 为
// 内存小服务器），断言「标为已读 / 全部已读」后 Navbar 铃铛角标即时减少 / 消失，
// 不依赖路由变化；并覆盖布局侧重构后的三条写入路径（播种 / 会话刷新 / 下拉
// 「全部已读」）。
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { createRawSnippet } from 'svelte';
import RootLayout from '../../routes/+layout.svelte';
import NotificationsPage from '../../routes/notifications/+page.svelte';
import type { Notification, User } from '$lib/api/client';

vi.mock('$app/state', () => ({
  page: { url: { pathname: '/', searchParams: new URLSearchParams() }, data: {} }
}));
vi.mock('$app/navigation', () => ({
  goto: vi.fn(),
  invalidate: vi.fn(),
  invalidateAll: vi.fn()
}));

const meUser = {
  username: 'alice',
  display_name: '爱丽丝',
  level: 7,
  roles: ['member']
} as unknown as User;

function makeNotification(id: string, title: string, type: string, link: string | null, isRead: boolean): Notification {
  return {
    id,
    type,
    title,
    body: null,
    link,
    is_read: isRead,
    created_at: Date.now() - 3600_000,
    read_at: isRead ? Date.now() : null
  };
}

/** 内存小服务器：2 条未读通知；/read 与 read-all 真实变更服务端状态。 */
function installServer() {
  const items = [
    makeNotification('n1', '有新回复', 'reply', '/posts/p1', false),
    makeNotification('n2', '社区规范已更新', 'system', null, false)
  ];
  const body = (data: unknown, status = 200): Response =>
    ({
      ok: status >= 200 && status < 300,
      status,
      headers: { get: () => null },
      json: async () => data
    }) as unknown as Response;

  const fetchStub = vi.fn(async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    const url = String(input);
    const method = (init?.method ?? 'GET').toUpperCase();
    if (url.endsWith('/auth/csrf')) return body({ token: 't1' });
    if (url.endsWith('/me')) return body(meUser);
    if (url.startsWith('/api/v1/notifications')) {
      if (method === 'POST' && url.endsWith('/read-all')) {
        let updated = 0;
        for (const i of items) if (!i.is_read) { i.is_read = true; updated += 1; }
        return body({ updated });
      }
      const m = url.match(/\/notifications\/(n\d)\/read$/);
      if (method === 'POST' && m) {
        const i = items.find((x) => x.id === m[1]);
        if (i) i.is_read = true;
        return body(null, 204);
      }
      if (url.endsWith('/preferences')) {
        return method === 'GET' ? body({ items: [] }) : body({ category: 'system', updated: true });
      }
      return body({
        items,
        unread_count: items.filter((i) => !i.is_read).length,
        next_cursor: null,
        has_more: false
      });
    }
    return body({ detail: `unexpected ${method} ${url}` }, 404);
  });

  vi.stubGlobal('fetch', fetchStub);
  return { items };
}

const children = createRawSnippet(() => ({ render: () => '<div data-testid="layout-children" />' }));

/** 渲染真实根布局：服务端 data 播种 2 条未读（角标首帧即 2）。 */
function renderLayout(): HTMLElement {
  const { container } = render(RootLayout, {
    props: {
      children,
      data: {
        user: meUser,
        notifications: {
          unreadCount: 2,
          recent: [
            makeNotification('n1', '有新回复', 'reply', '/posts/p1', false),
            makeNotification('n2', '社区规范已更新', 'system', null, false)
          ]
        }
      }
    }
  });
  return container;
}

function bellDot(container: HTMLElement): Element | null {
  return container.querySelector('.nav-notif-dot');
}

beforeEach(() => {
  installServer();
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('通知铃铛角标回归（已读后角标即时减少/消失）', () => {
  it('通知页「标为已读」→ 角标即时 2→1，页内未读徽标同步', async () => {
    const user = userEvent.setup();
    const container = renderLayout();
    render(NotificationsPage);

    // 布局播种 + 会话刷新路径：首帧与服务端一致为 2。
    expect(bellDot(container)?.textContent).toBe('2');
    await screen.findByTestId('read-n1'); // 页面 load() 完成（onMount）

    await user.click(screen.getByTestId('read-n1'));
    // 关键回归：无路由变化，角标也要减少。
    await waitFor(() => expect(bellDot(container)?.textContent).toBe('1'));
    expect(screen.getByText('1 未读')).toBeTruthy();
  });

  it('通知页「全部已读」→ 角标即时消失', async () => {
    const user = userEvent.setup();
    const container = renderLayout();
    render(NotificationsPage);

    await screen.findByTestId('read-all'); // 页面 load() 完成且未读数 > 0
    expect(bellDot(container)?.textContent).toBe('2');

    await user.click(screen.getByTestId('read-all'));
    await waitFor(() => expect(bellDot(container)).toBeNull());
  });

  it('铃铛下拉「全部已读」（布局侧重构路径）→ 角标消失', async () => {
    const user = userEvent.setup();
    const container = renderLayout();

    await waitFor(() => expect(bellDot(container)?.textContent).toBe('2'));
    await user.click(screen.getByRole('button', { name: '通知' }));
    await user.click(screen.getByRole('button', { name: '全部已读' }));
    await waitFor(() => expect(bellDot(container)).toBeNull());
  });
});
