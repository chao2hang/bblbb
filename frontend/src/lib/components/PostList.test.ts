import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/svelte';
import PostList from './PostList.svelte';

const createdAt = Date.now() - 60 * 60 * 1000;

describe('PostList 作者头像悬浮卡触发', () => {
  it('有账号投影的头像渲染为 /users/ 资料卡触发链接', () => {
    const { container } = render(PostList, {
      props: {
        posts: [
          {
            id: 'p1',
            title: '你好 BBLBB',
            author_name: 'alice',
            reply_count: 2,
            view_count: 10,
            created_at: createdAt
          }
        ]
      }
    });

    const trigger = container.querySelector('a.author-hover-trigger[href="/users/alice"]');
    expect(trigger).not.toBeNull();
    expect(trigger).toHaveAttribute('aria-label', '查看 alice 的个人资料');
    // 触发链接内保留原头像（md）。
    expect(trigger?.querySelector('.avatar')).not.toBeNull();
  });

  it('无账号投影（匿名）保持普通头像，不渲染触发链接', () => {
    const { container } = render(PostList, {
      props: {
        posts: [
          {
            id: 'p2',
            title: '匿名帖',
            reply_count: 0,
            view_count: 1,
            created_at: createdAt
          }
        ]
      }
    });

    expect(container.querySelector('a.author-hover-trigger')).toBeNull();
    expect(container.querySelector('.app-post-row .avatar')).not.toBeNull();
  });
});
