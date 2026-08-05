// M02-UX-05：/me 页无 JS 基线——SSR 输出安全投影（账号/验证状态）与
// 设备管理原生 form（?/revoke 隐藏 session_id、?/logoutall），且不输出
// 任何会话 token；当前设备有标记且不可撤销。
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
  roles: ['member']
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
