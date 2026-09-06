// M03-UI-07：管理页无 JS SSR 基线——板块/标签/角色/Assignment 页面渲染
// 后端裁决状态（ok/forbidden/not_implemented/error），表单在 SSR 中可提交。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import AdminBoards from '../../../routes/admin/boards/+page.svelte';
import AdminTags from '../../../routes/admin/tags/+page.svelte';
import AdminRoles from '../../../routes/admin/roles/+page.svelte';
import AdminAssignments from '../../../routes/admin/assignments/+page.svelte';

describe('M03-UI-07 管理板块页 SSR', () => {
  it('ok 状态渲染列表；not_implemented 渲染开发中状态', () => {
    const ok = render(AdminBoards, {
      props: { data: { loadState: { state: 'ok', items: [{ id: 'b1', slug: 'tech', name: '技术分享', description: '技术', version: 0, created_at: 0, updated_at: 0 }] } } }
    });
    expect(ok.body).toContain('技术分享');
    expect(ok.body).toContain('/tech');

    const ni = render(AdminBoards, { props: { data: { loadState: { state: 'not_implemented', message: 'x' } } } });
    expect(ni.body).toContain('开发中');
    // 创建表单始终可提交（后端已实现的 POST）。
    expect(ni.body).toMatch(/<form[^>]*action="\?\/create"/);
    expect(ni.body).toContain('name="reason"');
  });

  it('403 渲染无权限态（后端裁决）', () => {
    const { body } = render(AdminBoards, { props: { data: { loadState: { state: 'forbidden', message: 'forbidden' } } } });
    expect(body).toContain('无权限');
  });

  it('error 渲染错误文案', () => {
    const { body } = render(AdminBoards, { props: { data: { loadState: { state: 'error', message: 'unavailable' } } } });
    expect(body).toContain('unavailable');
  });
});

describe('M03-UI-07 管理标签/角色/Assignment 页 SSR', () => {
  it('标签页：not_implemented 状态 + 创建表单', () => {
    const { body } = render(AdminTags, { props: { data: { loadState: { state: 'not_implemented', message: 'x' } } } });
    expect(body).toContain('标签列表接口开发中');
    expect(body).toMatch(/<form[^>]*action="\?\/create"/);
  });

  it('角色页：403 无权限态', () => {
    const { body } = render(AdminRoles, { props: { data: { loadState: { state: 'forbidden', message: 'forbidden' }, allPermissions: [] } } });
    expect(body).toContain('无权限');
  });

  it('角色页：ok 渲染角色卡 + 权限复选网格（视觉对齐 M17-GAPFIX-06）', () => {
    const { body } = render(AdminRoles, {
      props: { data: { loadState: { state: 'ok', items: [{ id: 'r1', name: 'administrator', scope: 'global', permissions: ['admin.manage'] }] }, allPermissions: ['admin.manage'] } }
    });
    expect(body).toContain('app-role-grid');
    expect(body).toContain('管理员 · 全站');
    expect(body).toContain('管理后台');
    expect(body).toContain('admin.manage');
  });

  it('角色委派页：产品语气说明 + 后端裁决状态（视觉对齐 M17-GAPFIX-06：不暴露表名/工作项编号）', () => {
    const { body } = render(AdminAssignments, { props: { data: { loadState: { state: 'not_implemented', message: 'x' } } } });
    expect(body).toContain('角色委派');
    expect(body).toContain('所有授予与撤销都会写入审计日志');
    expect(body).toContain('角色委派接口开发中');
    expect(body).not.toContain('board_role_assignments');
    expect(body).not.toContain('M13-ADMIN');
  });
});
