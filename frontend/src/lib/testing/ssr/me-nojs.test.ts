// M02-UX-05：/me 个人主页（概览枢纽）无 JS 基线——SSR 输出安全投影
// （账号/验证/角色状态），不输出任何会话 token。2026-09 功能拆分后，
// 会话设备管理（?/revoke、?/logoutall）与两步验证（MFA）表单移至
// /me/security 与 /mfa，本页只保留安全状态概览与入口（见
// me-security-nojs.test.ts / mfa-nojs.test.ts）。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import MePage from '../../../routes/me/+page.svelte';

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

function meData(overrides: Record<string, unknown> = {}) {
  return { user, sessions, error: null, ...overrides };
}

describe('无 JS：/me 个人主页（M02-UX-05，概览枢纽）', () => {
  it('SSR 渲染安全投影：账号/状态/角色，不输出敏感值与会话表单', () => {
    const { body } = render(MePage, { props: { data: meData() } });
    expect(body).toContain('alice');
    expect(body).not.toContain('alice@example.com'); // 邮箱不对外展示
    expect(body).toContain('正常'); // status=active
    expect(body).toContain('TL3');
    expect(body).not.toContain('sess-current'); // 会话 id 不泄漏
    // 功能拆分：设备/MFA 写操作不在本页（迁至 /me/security 与 /mfa）
    expect(body).not.toMatch(/action="\?\/(revoke|logoutall|mfa-enroll)"/);
  });

  it('安全概览：两步验证状态 + 设备计数 + 安全中心入口', () => {
    const { body } = render(MePage, { props: { data: meData() } });
    expect(body).toContain('账号与安全');
    expect(body).toContain('未启用'); // user.mfa_enabled=false
    expect(body).toContain('登录设备');
    expect(body).toContain('2'); // 设备计数（概览信息条与安全卡）
    expect(body).toMatch(/href="\/me\/security"/);
  });

  it('已验证账号正常渲染，不泄漏邮箱', () => {
    const { body } = render(MePage, {
      props: {
        data: meData({ user: { ...user, email_verified: true } })
      }
    });
    expect(body).not.toContain('alice@example.com');
    expect(body).toContain('安全中心');
  });

  it('load 错误 → 渲染错误横幅，不渲染账号信息', () => {
    const { body } = render(MePage, {
      props: { data: { user: null, sessions: [], error: '服务暂不可用' } }
    });
    expect(body).toContain('服务暂不可用');
    expect(body).not.toContain('账号信息');
  });
});
