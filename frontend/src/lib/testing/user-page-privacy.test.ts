// M03-PROFILE-09：用户页客户端缓存/渲染隐私守卫。
//
// 用户页 /users/[username] 在客户端 onMount 拉取 `getUser` 并把结果存入
// `$state`（页面级客户端缓存）。本测试用「对抗性后端响应」（混入邮箱、
// 状态、版本、凭据、会话字段）验证：页面只渲染公开投影字段，私有字段
// 即使混入也不会进入 DOM（SSR/Hover Card 侧见 ssr/privacy.test.ts）。
import { describe, expect, it, vi } from 'vitest';
import { render, waitFor } from '@testing-library/svelte';
import UserPage from '../../routes/users/[username]/+page.svelte';

vi.mock('$app/state', () => ({
  page: {
    url: { pathname: '/users/alice' },
    params: { username: 'alice' },
    data: {},
    route: { id: '/users/[username]' }
  }
}));
vi.mock('$app/navigation', () => ({
  goto: vi.fn(),
  invalidate: vi.fn(),
  invalidateAll: vi.fn()
}));

/** 对抗性公开资料：公开字段之外混入邮箱/状态/凭据/会话/内部时间。 */
const adversarialProfile = {
  id: 'u1',
  username: 'alice',
  display_name: '爱丽丝',
  bio: '公开简介',
  level: 7,
  avatar_attachment_id: null,
  cover_attachment_id: null,
  signature: '公开签名',
  created_at: 0,
  email: 'alice@example.com',
  email_normalized: 'alice@example.com',
  status: 'active',
  version: 3,
  password_hash: 'CLIENT-CACHE-HASH',
  session_token: 'CLIENT-CACHE-TOKEN',
  mfa_enabled: true,
  last_login_at: 1780000000000
};

vi.mock('$lib/api/client', () => ({
  getUser: vi.fn(async () => adversarialProfile)
}));

describe('M03-PROFILE-09 用户页客户端缓存/渲染隐私守卫', () => {
  it('用户页只渲染公开字段，对抗性响应被投影拦截', async () => {
    render(UserPage);

    // 公开字段正常展示，私有字段不得进入 DOM（页面级客户端缓存 = $state 数据源）
    await waitFor(() => {
      const text = document.body.textContent ?? '';
      expect(text).toContain('爱丽丝');
      expect(text).toContain('@ alice');
      expect(text).toContain('LV.7');
      expect(text).not.toContain('alice@example.com');
      expect(text).not.toContain('CLIENT-CACHE-HASH');
      expect(text).not.toContain('CLIENT-CACHE-TOKEN');
      expect(text).not.toContain('mfa');
      expect(text).not.toContain('last_login');
      expect(text).not.toContain('status');
    });
  });
});
