import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/svelte';
import TopicComposer from './TopicComposer.svelte';

vi.mock('$app/navigation', () => ({
  goto: vi.fn(),
}));

let currentUrl = 'http://localhost/editor';

vi.mock('$app/state', () => ({
  get page() {
    return {
      url: new URL(currentUrl),
    };
  },
}));

function jsonResponse(data: unknown, status = 200) {
  return new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

beforeEach(() => {
  currentUrl = 'http://localhost/editor';
  globalThis.fetch = vi.fn(async (input: RequestInfo | URL) => {
    const u = typeof input === 'string' ? input : input instanceof URL ? input.toString() : input.url;
    if (u.includes('/api/v1/users/me')) {
      return jsonResponse({ id: 'u1', username: 'alice', level: 3 });
    }
    if (u.includes('/api/v1/boards')) {
      return jsonResponse({ items: [{ id: 'b1', name: '默认板块' }] });
    }
    if (u.includes('/api/v1/tags')) {
      return jsonResponse({
        items: [
          { id: 't1', name: 'svelte' },
          { id: 't2', name: 'rust' },
        ],
      });
    }
    if (u.includes('/api/v1/ai/capabilities')) {
      return jsonResponse({ enabled: false });
    }
    if (u.includes('/api/v1/posts/post-1')) {
      return jsonResponse({
        id: 'post-1',
        title: '已发布帖子',
        markdown: '这是正文',
        version: 1,
        board_id: 'b1',
        tags: ['svelte', 'rust'],
        author: { username: 'alice' },
        access_summary: { policy: 'public', unlocked: true },
      });
    }
    if (u.includes('/api/v1/drafts/draft-1')) {
      return jsonResponse({
        id: 'draft-1',
        title: '草稿标题',
        markdown: '草稿正文',
        version: 1,
        board_id: 'b1',
        visibility_level: 1,
        access_policy: 'public',
        tags: ['svelte', 'rust'],
      });
    }
    return jsonResponse({});
  }) as unknown as typeof fetch;
});

describe('TopicComposer 再次编辑标签自动带入', () => {
  it('编辑已有帖子时（?post_id=...），标签应自动带入编辑器', async () => {
    currentUrl = 'http://localhost/editor?post_id=post-1';
    render(TopicComposer);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: '移除标签 svelte' })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: '移除标签 rust' })).toBeInTheDocument();
    });
  });

  it('恢复草稿时（?draft=...），标签应自动带入编辑器', async () => {
    currentUrl = 'http://localhost/editor?draft=draft-1';
    render(TopicComposer);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: '移除标签 svelte' })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: '移除标签 rust' })).toBeInTheDocument();
    });
  });

  it('点击全屏按钮可切换全屏模式，并支持 ESC 退出全屏与双击标题栏切换', async () => {
    const { container } = render(TopicComposer);
    const expandBtn = screen.getByRole('button', { name: '全屏' });
    expect(expandBtn).toBeInTheDocument();

    const pagePublish = container.querySelector('#page-publish');
    const composerWindow = container.querySelector('.composer-window');
    expect(pagePublish).not.toHaveClass('is-fullscreen');
    expect(composerWindow).not.toHaveClass('is-fullscreen');

    // 点击进入全屏
    await fireEvent.click(expandBtn);
    expect(pagePublish).toHaveClass('is-fullscreen');
    expect(composerWindow).toHaveClass('is-fullscreen');
    expect(screen.getByRole('button', { name: '退出全屏' })).toBeInTheDocument();

    // 再次点击退出全屏
    await fireEvent.click(screen.getByRole('button', { name: '退出全屏' }));
    expect(pagePublish).not.toHaveClass('is-fullscreen');
    expect(composerWindow).not.toHaveClass('is-fullscreen');
    expect(screen.getByRole('button', { name: '全屏' })).toBeInTheDocument();

    // 点击进入全屏后按 ESC 退出全屏
    await fireEvent.click(screen.getByRole('button', { name: '全屏' }));
    expect(pagePublish).toHaveClass('is-fullscreen');
    await fireEvent.keyDown(window, { key: 'Escape' });
    expect(pagePublish).not.toHaveClass('is-fullscreen');

    // 双击标题栏进入全屏
    const titlebar = container.querySelector('.composer-titlebar');
    expect(titlebar).not.toBeNull();
    await fireEvent.dblClick(titlebar!);
    expect(pagePublish).toHaveClass('is-fullscreen');
    await fireEvent.dblClick(titlebar!);
    expect(pagePublish).not.toHaveClass('is-fullscreen');
  });
});
