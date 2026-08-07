// M08-UI-03/04：隐私与索引设置页 SSR 快照。
//
// - 逐帖退出搜索/AI 摘要说明（含设置位置入口 /editor）；
// - 管理员全站/板块策略优先级展示；
// - robots/索引状态说明不承诺 robots 能阻止恶意抓取（声明层 ≠ 安全边界）；
// - 不输出任何会话/凭据字段。
import { describe, expect, it, vi } from 'vitest';
import { render } from 'svelte/server';
import PrivacyPage from '../../../routes/settings/privacy/+page.svelte';
import type { User } from '$lib/api/types';

vi.mock('$app/state', () => ({
  page: { url: { origin: 'http://test.local' } }
}));

const user: User = {
  id: 'u1',
  username: 'alice',
  email: 'alice@example.com',
  email_verified: true,
  status: 'active',
  display_name: '爱丽丝',
  bio: '简介',
  signature: '签名',
  timezone: 'UTC',
  theme_name: null,
  email_visible_to: 'nobody',
  profile_visible_to: 'everyone',
  level: 7,
  roles: ['member'],
  mfa_enabled: false,
  version: 3
};

describe('M08-UI-03/04 隐私与索引页 SSR', () => {
  it('渲染逐帖退出说明与设置位置入口（/editor）', () => {
    const { body } = render(PrivacyPage, { props: { data: { user, error: null } } });
    expect(body).toContain('逐帖');
    expect(body).toContain('search_index_opt_out');
    expect(body).toContain('ai_summary_opt_out');
    expect(body).toContain('href="/editor"');
  });

  it('展示管理员策略优先级（全站/板块强制关闭优先）', () => {
    const { body } = render(PrivacyPage, { props: { data: { user, error: null } } });
    expect(body).toContain('管理员策略优先级');
    expect(body).toContain('全站或板块');
    expect(body).toContain('不会绕过管理员策略');
    expect(body).toContain('M08-INDEX-07');
  });

  it('robots 说明：声明层而非安全边界，不承诺阻止恶意抓取', () => {
    const { body } = render(PrivacyPage, { props: { data: { user, error: null } } });
    expect(body).toContain('robots 与抓取边界');
    expect(body).toContain('协作性声明');
    expect(body).toContain('不能阻止恶意或');
    expect(body).toContain('真正的边界是服务端授权');
    expect(body).toContain('GPTBot');
    expect(body).not.toMatch(/robots[^。]*阻止所有/);
  });

  it('索引状态：搜索页 noindex 与公开文章投影说明', () => {
    const { body } = render(PrivacyPage, { props: { data: { user, error: null } } });
    expect(body).toContain('noindex,follow,noarchive');
    expect(body).toContain('canonical');
    expect(body).toContain('X-Robots-Tag');
  });

  it('隐私守卫：对抗性 user 字段不进入 HTML', () => {
    const adversarial = {
      ...user,
      password_hash: 'PRIVACY-SSR-HASH',
      session_token: 'PRIVACY-SSR-TOKEN'
    };
    const { body } = render(PrivacyPage, { props: { data: { user: adversarial, error: null } } });
    expect(body).not.toContain('PRIVACY-SSR-HASH');
    expect(body).not.toContain('PRIVACY-SSR-TOKEN');
  });

  it('load 失败 → 错误横幅，不渲染设置内容', () => {
    const { body } = render(PrivacyPage, { props: { data: { user: null, error: '服务暂不可用' } } });
    expect(body).toContain('服务暂不可用');
    expect(body).not.toContain('search_index_opt_out');
  });
});
