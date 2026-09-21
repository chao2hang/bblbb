// M02-UX-SEC：/me/security 账号与安全页无 JS 基线——安全清单（密码/
// 两步验证/OAuth/设备各行的状态与唯一入口）+ 设备管理原生 form
// （?/revoke 隐藏 session_id、?/logoutall），且不输出任何会话 token；
// 当前设备有标记且不可撤销。自 me-nojs.test.ts 平移（功能拆分）。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import SecurityPage from '../../../routes/me/security/+page.svelte';
import type { SecurityPageData } from '../../../routes/me/security/+page.server';

const user = {
  id: 'u-1',
  username: 'alice',
  email: 'alice@example.com',
  email_verified: false,
  status: 'active',
  display_name: null,
  level: 3,
  roles: ['member'],
  mfa_enabled: false,
  version: 3
};

const sessions = [
  {
    id: 'sess-current',
    user_agent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X) Safari/605.1.15',
    created_at: 1700000000000,
    last_seen_at: 1750000000000,
    absolute_expires_at: 1750060000000,
    version: 1
  },
  {
    id: 'sess-other',
    user_agent: 'Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X)',
    created_at: 1690000000000,
    last_seen_at: 1740000000000,
    absolute_expires_at: 1750060000000,
    version: 1
  }
];

function secData(overrides: Partial<SecurityPageData> = {}): SecurityPageData {
  return { user, sessions, currentSessionId: 'sess-current', error: null, ...overrides };
}

function renderPage(data: SecurityPageData) {
  return render(SecurityPage, { props: { data, form: undefined } });
}

describe('无 JS：/me/security 安全清单（M02-UX-SEC）', () => {
  it('渲染设置导航侧栏（SettingsNav），登录设备高亮', () => {
    const { body } = renderPage(secData());
    expect(body).toContain('app-settings-nav');
    expect(body).toContain('个人资料');
    expect(body).toContain('外观与主题');
    expect(body).toContain('账号安全');
    expect(body).toContain('登录设备');
    expect(body).toContain('通知设置');
    expect(body).toContain('OAuth 授权');
    expect(body).toContain('隐私设置');
    // 登录设备处于激活状态
    expect(body).toMatch(/<a[^>]*href="\/me\/security"[^>]*class="[^"]*is-active[^"]*"/);
  });

  it('SSR 渲染安全状态：两步验证徽标（未启用）与唯一管理入口 /mfa', () => {
    const { body } = renderPage(secData());
    expect(body).toContain('安全状态');
    expect(body).toContain('未启用');
    expect(body).toMatch(/href="\/mfa"/);
    expect(body).toMatch(/href="\/settings#settings-security"/); // 修改密码入口
    expect(body).not.toContain('alice@example.com'); // 邮箱不对外展示
  });

  it('已启用：两步验证徽标为已启用', () => {
    const { body } = renderPage(secData({ user: { ...user, mfa_enabled: true } }));
    expect(body).toContain('已启用');
  });
});

describe('无 JS：/me/security 设备管理（M02-UX-05 平移）', () => {
  it('设备列表：SSR 输出 ?/revoke 原生表单（隐藏 session_id）+ 撤销按钮', () => {
    const { body } = renderPage(secData());
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/revoke"/);
    expect(body).toContain('name="session_id"');
    expect(body).toContain('撤销');
    expect(body).not.toContain('sess-current'); // 会话 id 仅作隐藏表单值不泄漏
  });

  it('当前设备有标记且不可撤销；其他设备有撤销入口', () => {
    const { body } = renderPage(secData());
    expect(body).toContain('当前设备');
    expect(body).toContain('当前设备不可撤销');
    expect(body).toContain('Mac'); // UA 派生设备标签
    expect(body).toContain('手机');
  });

  it('SSR 输出 ?/logoutall 原生表单（退出全部设备）', () => {
    const { body } = renderPage(secData());
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/logoutall"/);
    expect(body).toContain('退出全部设备');
  });

  it('无设备：显示空态，不输出撤销表单', () => {
    const { body } = renderPage(secData({ sessions: [], currentSessionId: null }));
    expect(body).toContain('暂无登录设备');
    expect(body).not.toMatch(/action="\?\/revoke"/);
  });

  it('load 错误 → 渲染错误横幅，不渲染安全清单', () => {
    const { body } = renderPage({ user: null, sessions: [], currentSessionId: null, error: '服务暂不可用' });
    expect(body).toContain('服务暂不可用');
    expect(body).not.toContain('安全状态');
  });
});
