// M02-UX-05：/me 个人中心（tag「账户资产」默认页）无 JS 基线——SSR 输出
// 安全投影（账号/验证/角色状态），不输出任何会话 token。设备管理（?/revoke、
// ?/logoutall）与两步验证（MFA）表单在 /me/security 与 /mfa（见
// me-security-nojs.test.ts / mfa-nojs.test.ts）。
// 成熟个人中心 IA：资料卡 hero + 资产统计条（三列可点统计）+ 分组 cell
// 列表（内容与互动 / 资产与凭证 / 账户与偏好）；分区导航 = MeIdentityBar
// 身份条 + MeSectionTabs tag 栏，每个 tag 为独立路由页面（账户资产 /me ·
// 账号安全 /me/security · 信任等级 /me/level），状态/进度详情不在本页重复。
import { describe, expect, it, vi } from 'vitest';
import { render } from 'svelte/server';
import MePage from '../../../routes/me/+page.svelte';

vi.mock('$app/state', () => ({
  page: {
    url: { pathname: '/me', searchParams: new URLSearchParams() },
    params: {},
    data: {},
    route: { id: '/me' }
  }
}));

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

const activity = {
  balances: [{ currency: 'coin', amount: 630 }],
  streak_days: 3,
  checked_in_today: false
};

function meData(overrides: Record<string, unknown> = {}) {
  return { user, sessions, activity, error: null, ...overrides };
}

describe('无 JS：/me 个人中心（成熟个人中心 IA）', () => {
  it('SSR 渲染安全投影：账号/状态/角色，不输出敏感值与会话表单', () => {
    const { body } = render(MePage, { props: { data: meData() } });
    expect(body).toContain('alice');
    expect(body).not.toContain('alice@example.com'); // 邮箱不对外展示
    expect(body).toContain('正常'); // status=active
    expect(body).toContain('TL3');
    expect(body).not.toContain('sess-current'); // 会话 id 不泄漏
    // 设备/MFA 写操作不在本页（迁至 /me/security 与 /mfa）
    expect(body).not.toMatch(/action="\?\/(revoke|logoutall|mfa-enroll)"/);
  });

  it('分区 tag 栏：每个 tag 独立路由，安全/等级详情不在本页重复', () => {
    const { body } = render(MePage, { props: { data: meData() } });
    // tag 栏三个分区（当前页「账户资产」高亮）
    expect(body).toContain('账户资产');
    expect(body).toContain('账号安全');
    expect(body).toContain('信任等级');
    expect(body).toMatch(/href="\/me\/security"/);
    expect(body).toMatch(/href="\/me\/level"/);
    expect(body).toMatch(/aria-current="page"[^>]*>\s*账户资产/);
    // 账号安全状态卡已拆至 /me/security：本页不再渲染两步验证状态
    expect(body).not.toContain('两步验证');
    // 信任等级进度详情已拆至 /me/level：本页不再渲染逐项进度
    expect(body).not.toContain('窗口内访问天数');
  });

  it('资产统计条：余额/签到/设备三列可点统计，无 JS 可读', () => {
    const { body } = render(MePage, { props: { data: meData() } });
    expect(body).toContain('金币余额');
    expect(body).toContain('630');
    expect(body).toContain('连续签到');
    expect(body).toContain('登录设备');
    expect(body).toContain('2'); // 设备数（统计条与账户组 cell）
    // 统计卡整行可点：余额 → 积分明细，设备 → 账号与安全
    expect(body).toMatch(/href="\/me\/balance"/);
  });

  it('分组 cell 列表：三个领域组覆盖全部功能入口（无 JS 可达）', () => {
    const { body } = render(MePage, { props: { data: meData() } });
    expect(body).toContain('内容与互动');
    expect(body).toContain('我的帖子');
    expect(body).toContain('我的收藏');
    expect(body).toContain('资产与凭证');
    expect(body).toContain('积分明细');
    expect(body).toContain('API 密钥');
    expect(body).toContain('账户与偏好');
    expect(body).toContain('账号与安全');
    expect(body).toContain('账号设置');
    expect(body).toContain('通知设置');
    expect(body).toContain('OAuth 授权');
    // 身份吸顶条（三页共用）
    expect(body).toContain('me-identity-bar');
  });

  it('已验证账号正常渲染，不泄漏邮箱', () => {
    const { body } = render(MePage, {
      props: {
        data: meData({ user: { ...user, email_verified: true } })
      }
    });
    expect(body).not.toContain('alice@example.com');
    expect(body).toMatch(/href="\/me\/security"/);
  });

  it('load 错误 → 渲染错误横幅，不渲染账号信息', () => {
    const { body } = render(MePage, {
      props: { data: { user: null, sessions: [], error: '服务暂不可用' } }
    });
    expect(body).toContain('服务暂不可用');
    expect(body).not.toContain('账号信息');
  });
});
