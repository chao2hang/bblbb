// M13-UI-06：管理用户页 SSR 快照（无 JS 退化 + 管理 DTO 隐私守卫）。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import AdminUsersPage from '../../../routes/admin/users/+page.svelte';
import type { AdminUsersPageData } from '../../../routes/admin/users/+page.server';

const okData: AdminUsersPageData = {
  state: 'ok',
  items: [
    {
      id: 'u1',
      username: 'alice',
      email: 'alice@example.com',
      email_verified: true,
      status: 'active',
      display_name: 'Alice',
      level: 3,
      roles: ['member'],
      created_at: 1700000000000,
      updated_at: 1700000000000,
      last_login_at: null,
      version: 1
    }
  ],
  error: null
};

describe('M13-UI-06 管理用户 SSR', () => {
  it('ok → 用户列表 + 状态徽章 + 角色 + 更新表单（If-Match version + reason）', () => {
    const { body } = render(AdminUsersPage, { props: { data: okData, form: null } });
    expect(body).toContain('alice');
    expect(body).toContain('alice@example.com');
    expect(body).toContain('member');
    expect(body).toContain('action="?/update"');
    expect(body).toContain('name="version"');
    expect(body).toContain('name="reason"');
    expect(body).toContain('value="1"');
  });

  it('隐私守卫：管理 DTO 不含 password_hash/secret/session', () => {
    const adversarial = {
      state: 'ok',
      error: null,
      items: [
        {
          id: 'u2',
          username: 'bob',
          email: 'bob@example.com',
          email_verified: false,
          status: 'pending',
          display_name: null,
          level: 1,
          roles: [],
          created_at: 1,
          updated_at: 1,
          last_login_at: null,
          version: 1,
          password_hash: 'ADMIN-USERS-SSR-HASH',
          recovery_codes: ['ADMIN-USERS-SSR-RECOVERY']
        }
      ]
    } as unknown as AdminUsersPageData;
    const { body } = render(AdminUsersPage, { props: { data: adversarial, form: null } });
    expect(body).not.toContain('ADMIN-USERS-SSR-HASH');
    expect(body).not.toContain('ADMIN-USERS-SSR-RECOVERY');
  });

  it('403 → 无权限态', () => {
    const forbidden = render(AdminUsersPage, {
      props: { data: { state: 'forbidden', items: null, error: 'forbidden' }, form: null }
    });
    expect(forbidden.body).toContain('无权限');
    expect(forbidden.body).not.toContain('alice');
  });

  it('空状态 → 提示无用户', () => {
    const { body } = render(AdminUsersPage, {
      props: { data: { state: 'ok', items: [], error: null }, form: null }
    });
    expect(body).toContain('暂无用户数据');
  });
});
