// M03-UI-01：用户主页 SSR 输出守卫——无 JS 时公开投影可读，且对抗性
// 数据（混入邮箱/状态/凭据/内部字段）不进入 SSR HTML；降级投影（banned/
// pending_delete，bio/signature 置空）安全渲染。
import { describe, expect, it, vi } from 'vitest';
import { render } from 'svelte/server';
import UserPage from '../../../routes/users/[username]/+page.svelte';

// 用户页的 username 来自 $app/state page.params；隔离渲染需提供假 page。
vi.mock('$app/state', () => ({
  page: {
    url: { pathname: '/users/alice' },
    params: { username: 'alice' },
    data: {},
    route: { id: '/users/[username]' }
  }
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
  password_hash: 'SSR-USER-PAGE-HASH',
  session_token: 'SSR-USER-PAGE-TOKEN',
  mfa_enabled: true,
  last_login_at: 1780000000000,
  // 签名 URL（M6 渲染期临时解析，不得被任何投影持久化）
  cover_url: 'https://cdn.example.com/cover-alice?v=1&X-Amz-Signature=abc&Expires=1789999999',
  avatar_url: 'https://cdn.example.com/avatar-alice?v=1&X-Amz-Signature=def&Expires=1789999999',
  signed_url: 'https://s3.example.com/private?v=1&X-Amz-Signature=xyz&Expires=1789999999'
};

describe('M03-UI-01 用户主页 SSR 守卫', () => {
  it('SSR 渲染公开投影，对抗性响应不进入 HTML', () => {
    const { body } = render(UserPage, { props: { data: { user: adversarialProfile } } });
    expect(body).toContain('爱丽丝');
    expect(body).toContain('@ alice');
    expect(body).toContain('LV.7');
    expect(body).toContain('公开简介');
    expect(body).toContain('公开签名');
    expect(body).not.toContain('alice@example.com');
    expect(body).not.toContain('SSR-USER-PAGE-HASH');
    expect(body).not.toContain('SSR-USER-PAGE-TOKEN');
    expect(body).not.toContain('mfa');
    expect(body).not.toContain('last_login');
    expect(body).not.toContain('status');
    // 签名 URL 不进入 SSR HTML（不持久化渲染期临时 URL）。
    expect(body).not.toContain('cdn.example.com');
    expect(body).not.toContain('X-Amz-Signature');
    expect(body).not.toContain('signed_url');
  });

  it('降级投影（banned/pending_delete：bio/signature/媒体置空）安全渲染且不泄漏状态', () => {
    const degraded = {
      ...adversarialProfile,
      bio: null,
      signature: null,
      avatar_attachment_id: null,
      cover_attachment_id: null
    };
    const { body } = render(UserPage, { props: { data: { user: degraded } } });
    // 公开字段仍渲染。
    expect(body).toContain('爱丽丝');
    expect(body).toContain('@ alice');
    expect(body).toContain('LV.7');
    // 置空字段不渲染，状态/邮箱不进入 HTML。
    expect(body).not.toContain('公开简介');
    expect(body).not.toContain('公开签名');
    expect(body).not.toContain('alice@example.com');
    expect(body).not.toContain('banned');
    expect(body).not.toContain('status');
  });
});
