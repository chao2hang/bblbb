// M02-UX-05/06：/me 页无 JS 基线——SSR 输出安全投影（账号/验证状态）与
// 设备管理原生 form（?/revoke 隐藏 session_id、?/logoutall），且不输出
// 任何会话 token；当前设备有标记且不可撤销。MFA 卡：启用/停用/恢复码/
// step-up 各态均为原生 form[method=POST]。
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

describe('无 JS：/me 页（M02-UX-05）', () => {
  it('SSR 渲染安全投影：账号/验证/角色状态，不输出敏感值', () => {
    const { body } = render(MePage, {
      props: {
        data: { user, sessions, currentSessionId: 'sess-current', error: null },
        form: undefined
      }
    });
    expect(body).toContain('alice');
    expect(body).toContain('alice@example.com'); // 本人账号邮箱可展示
    expect(body).toContain('未验证'); // email_verified=false → 去验证入口
    expect(body).toContain('正常'); // status=active
    expect(body).toContain('LV.3');
    expect(body).not.toContain('sess-current'); // 会话 id 仅作隐藏表单值不泄漏
  });

  it('设备列表：SSR 输出 ?/revoke 原生表单（隐藏 session_id）+ 撤销按钮', () => {
    const { body } = render(MePage, {
      props: {
        data: { user, sessions, currentSessionId: 'sess-current', error: null },
        form: undefined
      }
    });
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/revoke"/);
    expect(body).toContain('name="session_id"');
    expect(body).toContain('撤销');
  });

  it('当前设备有标记且不可撤销；其他设备有撤销入口', () => {
    const { body } = render(MePage, {
      props: {
        data: { user, sessions, currentSessionId: 'sess-current', error: null },
        form: undefined
      }
    });
    expect(body).toContain('当前设备');
    expect(body).toContain('当前设备不可撤销');
    expect(body).toContain('Mac'); // UA 派生设备标签
    expect(body).toContain('手机');
  });

  it('SSR 输出 ?/logoutall 原生表单（退出全部设备）', () => {
    const { body } = render(MePage, {
      props: {
        data: { user, sessions, currentSessionId: 'sess-current', error: null },
        form: undefined
      }
    });
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/logoutall"/);
    expect(body).toContain('退出全部设备');
  });

  it('已验证账号不显示去验证入口', () => {
    const { body } = render(MePage, {
      props: {
        data: {
          user: { ...user, email_verified: true },
          sessions: [],
          currentSessionId: null,
          error: null
        },
        form: undefined
      }
    });
    expect(body).toContain('已验证');
    expect(body).not.toContain('去验证');
    expect(body).toContain('暂无登录设备');
  });

  it('load 错误 → 渲染错误横幅，不渲染账号信息', () => {
    const { body } = render(MePage, {
      props: {
        data: { user: null, sessions: [], currentSessionId: null, error: '服务暂不可用' },
        form: undefined
      }
    });
    expect(body).toContain('服务暂不可用');
    expect(body).not.toContain('账号信息');
  });
});

describe('无 JS：/me 页 MFA 管理（M02-UX-06）', () => {
  it('未启用：SSR 输出 ?/mfa-enroll 原生表单 + 未启用徽标', () => {
    const { body } = render(MePage, {
      props: {
        data: { user: { ...user, mfa_enabled: false }, sessions: [], currentSessionId: null, error: null },
        form: undefined
      }
    });
    expect(body).toContain('未启用');
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/mfa-enroll"/);
    expect(body).not.toContain('?/mfa-disable');
  });

  it('已启用：SSR 输出 ?/mfa-disable 与 ?/mfa-recovery 原生表单', () => {
    const { body } = render(MePage, {
      props: {
        data: { user: { ...user, mfa_enabled: true }, sessions: [], currentSessionId: null, error: null },
        form: undefined
      }
    });
    expect(body).toContain('已启用');
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/mfa-disable"/);
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/mfa-recovery"/);
    expect(body).not.toContain('?/mfa-enroll');
  });

  it('enroll-challenge：SSR 输出密钥 + ?/mfa-confirm（code 输入）与 ?/mfa-cancel', () => {
    const { body } = render(MePage, {
      props: {
        data: { user: { ...user, mfa_enabled: false }, sessions: [], currentSessionId: null, error: null },
        form: { mfa: { kind: 'enroll-challenge', otpauth_uri: 'otpauth://totp/BBLBB:alice@example.com', secret_base32: 'JBSWY3DPEHPK3PXP' } }
      }
    });
    expect(body).toContain('JBSWY3DPEHPK3PXP');
    expect(body).toContain('otpauth://totp/');
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/mfa-confirm"/);
    expect(body).toContain('name="code"');
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/mfa-cancel"/);
  });

  it('recovery-codes：SSR 一次展示恢复码并提示只显示一次', () => {
    const { body } = render(MePage, {
      props: {
        data: { user: { ...user, mfa_enabled: true }, sessions: [], currentSessionId: null, error: null },
        form: { mfa: { kind: 'recovery-codes', codes: ['ABCDEFGHIJKLMNOP', 'QRSTUVWXYZ234567'] } }
      }
    });
    expect(body).toContain('ABCDEFGHIJKLMNOP');
    expect(body).toContain('QRSTUVWXYZ234567');
    expect(body).toContain('只显示这一次');
    expect(body).toContain('我已保存');
  });

  it('step-up：SSR 输出 ?/re-auth 原生表单 + intent 隐藏域', () => {
    const { body } = render(MePage, {
      props: {
        data: { user: { ...user, mfa_enabled: true }, sessions: [], currentSessionId: null, error: null },
        form: { mfa: { kind: 'step-up', intent: 'disable' } }
      }
    });
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/re-auth"/);
    expect(body).toContain('name="password"');
    expect(body).toContain('name="intent"');
    expect(body).toContain('验证身份');
  });

  it('reauth-done：SSR 输出重试原操作表单（intent=disable → ?/mfa-disable）', () => {
    const { body } = render(MePage, {
      props: {
        data: { user: { ...user, mfa_enabled: true }, sessions: [], currentSessionId: null, error: null },
        form: { mfa: { kind: 'reauth-done', intent: 'disable' } }
      }
    });
    expect(body).toContain('身份已验证');
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/mfa-disable"/);
  });

  it('disabled：SSR 输出重新启用入口', () => {
    const { body } = render(MePage, {
      props: {
        data: { user: { ...user, mfa_enabled: false }, sessions: [], currentSessionId: null, error: null },
        form: { mfa: { kind: 'disabled' } }
      }
    });
    expect(body).toContain('两步验证已停用');
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/mfa-enroll"/);
  });
});
