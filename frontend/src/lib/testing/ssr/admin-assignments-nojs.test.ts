// M18-ADMIN-OPS（约定 D）：角色委派页 SSR 快照——当前角色表行「撤销」=「⋮」
// 三点菜单触发按钮（aria-label 含行语义）；菜单关闭态不渲染列表项；撤销走常驻
// SSR 隐藏表单（user_id/role_name/reason 审计要素）+ DangerConfirm（关闭态不渲染）。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import AdminAssignmentsPage from '../../../routes/admin/assignments/+page.svelte';
import type { AdminAssignmentsPageData } from '../../../routes/admin/assignments/+page.server';

const okData: AdminAssignmentsPageData = {
  loadState: {
    state: 'ok',
    items: [
      { id: 'r1', name: 'administrator' },
      { id: 'r2', name: 'board_moderator' }
    ]
  },
  q: 'alice',
  users: null,
  selectedUser: {
    id: 'u-1',
    username: 'alice',
    email: 'alice@example.com',
    status: 'active',
    display_name: 'Alice',
    level: 3,
    roles: ['administrator', 'board_moderator']
  },
  userError: null
};

describe('M18-ADMIN-OPS 角色委派 SSR（约定 D）', () => {
  it('选中用户 → 角色行渲染 + 行「⋮」菜单触发按钮 + 隐藏撤销表单（审计要素）', () => {
    const { body } = render(AdminAssignmentsPage, { props: { data: okData, form: null } });
    // 角色管理卡与角色行仍渲染
    expect(body).toContain('角色管理 · Alice');
    expect(body).toContain('管理员');
    expect(body).toContain('板块版主');
    // 约定 D：行「撤销」=「⋮」菜单触发按钮（aria-label 含行语义，每角色一个）
    expect(body).toContain('aria-label="更多操作：角色 管理员"');
    expect(body).toContain('aria-label="更多操作：角色 板块版主"');
    // 菜单关闭态不渲染列表项「撤销角色」（DangerConfirm 标题同文案，关闭态也不渲染；
    // 批量撤销 Dialog / BatchBar 在未选中角色时不渲染）
    expect(body).not.toContain('撤销角色');
    expect(body).not.toMatch(/<form[^>]*action="\?\/batchRevoke"/);
    // 授予 Dialog 关闭态不渲染表单；撤销走常驻隐藏表单（user_id/role_name/reason）
    expect(body).not.toMatch(/<form[^>]*action="\?\/grant"/);
    expect(body).toContain('action="?/revoke"');
    expect(body).toContain('name="reason"');
    // 守卫：页面不泄露后端表名 / 工作项编号
    expect(body).not.toContain('board_role_assignments');
    expect(body).not.toContain('M13-ADMIN');
  });
});
