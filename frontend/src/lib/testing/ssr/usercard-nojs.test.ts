// M03-UI-03/04：UserCard 无 JS SSR 基线——SSR 只输出可跳转的触发链接，
// 不渲染任何浮层/私有字段（portal 仅客户端增强）。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import UserCard from '$lib/components/UserCard.svelte';

describe('M03-UI-03/04 UserCard 无 JS SSR 基线', () => {
  it('SSR 输出触发链接（href 直达主页），不渲染浮层内容', () => {
    const { body } = render(UserCard, {
      props: {
        user: {
          username: 'alice',
          display_name: '爱丽丝',
          level: 7,
          bio: '公开简介',
          signature: '公开签名'
        }
      }
    });
    expect(body).toMatch(/<a[^>]*class="author-hover-trigger"[^>]*aria-label="查看 爱丽丝 的个人资料"/);
    expect(body).toContain('href="/users/alice"');
    expect(body).not.toContain('user-card-popover');
    expect(body).not.toContain('user-card-sheet');
    expect(body).not.toContain('公开简介'); // 浮层内容不出现在 SSR HTML
  });

  it('SSR 隐私守卫：对抗性字段（邮箱/状态/凭据）不进入 HTML', () => {
    const { body } = render(UserCard, {
      props: {
        user: {
          username: 'alice',
          display_name: '爱丽丝',
          level: 7,
          bio: '公开简介',
          signature: '公开签名',
          email: 'alice@example.com',
          status: 'banned',
          password_hash: 'USER-CARD-SSR-HASH'
        } as never
      }
    });
    expect(body).not.toContain('alice@example.com');
    expect(body).not.toContain('banned');
    expect(body).not.toContain('USER-CARD-SSR-HASH');
  });
});
