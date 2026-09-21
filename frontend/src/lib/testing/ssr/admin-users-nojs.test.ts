// M13-UI-06：管理用户页 SSR 快照（无 JS 退化 + 管理 DTO 隐私守卫）。
// M18-ADMIN-DIALOG：写操作弹层化后，SSR HTML 只渲染列表/筛选/行操作触发入口
// （状态表单与批量表单收进 Dialog，无 JS 不提交）；隐私守卫断言保留。
// M18-ADMIN-OPS（约定 D）：行内写操作入口 = 「⋮」三点菜单触发按钮
// （aria-label 含行语义）；菜单关闭态不渲染列表文案。
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
      coin_balance: 120,
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
  it('ok → 用户列表 + 行操作有且只有「⋯」菜单 + 选择列 aria 标签', () => {
    const { body } = render(AdminUsersPage, { props: { data: okData, form: null } });
    // 列表行仍完整渲染
    expect(body).toContain('alice');
    expect(body).toContain('alice@example.com');
    expect(body).toContain('member');
    // 筛选与计数（SSR 基线保留）
    expect(body).toContain('aria-label="按用户名过滤"');
    expect(body).toContain('aria-label="状态筛选"');
    expect(body).toContain('共 1 名成员');
    // 行数据状态可见（徽章 + 隐藏文本）
    expect(body).toContain('display:none;');
    expect(body).toContain('active');
    // 写操作弹层化（约定 D）：行「⋮」菜单触发按钮渲染（aria-label 含行语义），
    // 菜单关闭态不渲染列表项「设置用户状态」（Dialog 标题同文案，关闭态也不渲染）；
    // 状态表单本体收进 Dialog（SSR 不渲染）
    expect(body).toContain('aria-label="更多操作：用户 alice"');
    expect(body).not.toContain('设置用户状态');
    expect(body).not.toContain('action="?/update"');
    // 行内操作有且只有「⋯」触发按钮（约定 D）：设置用户状态/信任/详情/调整积分均收进菜单，
    // 菜单关闭态不进 SSR HTML（GET 导航同样由菜单 goto 完成）
    expect(body).not.toContain('调整积分');
    expect(body).not.toContain('>信任</button>');
    // 选择列 aria 标签（表头全选 + 行选择）
    expect(body).toContain('aria-label="全选当前列表"');
    expect(body).toContain('aria-label="选择 alice"');
    // 未选中时批量工具条不渲染（BatchBar count=0 无多余 DOM）
    expect(body).not.toContain('批量设置状态');
    // 底部导出按钮（客户端 CSV）
    expect(body).toContain('导出用户 CSV');
    // 昵称黑名单入口按钮渲染
    expect(body).toContain('昵称黑名单');
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

  it('step_up_required → 渲染重新验证表单（M02-MFA-07）', () => {
    const { body } = render(AdminUsersPage, {
      props: {
        data: okData,
        form: { message: '此操作需要重新验证身份，请输入密码重新验证后重试', stepUpRequired: true }
      }
    });
    expect(body).toContain('需要重新验证身份');
    expect(body).toContain('name="password"');
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/reauth"/);
    expect(body).toContain('重新验证');
  });
});
