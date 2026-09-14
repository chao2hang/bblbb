// M03-UI-07：管理页无 JS SSR 基线——板块/标签/角色/Assignment 页面渲染
// 后端裁决状态（ok/forbidden/not_implemented/error）。
// M18-ADMIN-DIALOG：写操作弹层化后，SSR 只渲染触发按钮（新建表单收进 Dialog）。
// M18-ADMIN-OPS（约定 D）：行内写操作收敛为每行一个「⋮」三点菜单（RowActionsMenu）；
// 菜单关闭态只渲染触发按钮（行语义 aria-label），操作列表与行操作 Dialog 分节表单
// 均不进 SSR HTML；隐私/守卫断言原样保留。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import AdminBoards from '../../../routes/admin/boards/+page.svelte';
import AdminTags from '../../../routes/admin/tags/+page.svelte';
import AdminRoles from '../../../routes/admin/roles/+page.svelte';
import AdminAssignments from '../../../routes/admin/assignments/+page.svelte';

describe('M03-UI-07 管理板块页 SSR', () => {
  it('ok 状态渲染列表；not_implemented 渲染开发中状态', () => {
    const ok = render(AdminBoards, {
      props: { data: { loadState: { state: 'ok', items: [{ id: 'b1', slug: 'tech', name: '技术分享', description: '技术', icon: null, version: 0, created_at: 0, updated_at: 0 }] } } }
    });
    expect(ok.body).toContain('技术分享');
    expect(ok.body).toContain('/tech');
    // 选择列 aria 标签（批量启停接入 BatchBar）+ 每行一个「⋮」三点菜单（约定 D）
    expect(ok.body).toContain('aria-label="全选当前列表"');
    expect(ok.body).toContain('aria-label="选择板块 技术分享"');
    expect(ok.body).toContain('aria-label="更多操作：板块 技术分享"');
    // 菜单关闭态：操作列表（编辑/置顶项）不进 SSR HTML
    expect(ok.body).not.toContain('row-actions__menu');
    // 行内操作有且只有「⋯」触发按钮（约定 D）：查看/编辑/置顶均收进菜单，关闭态不进 SSR HTML
    expect(ok.body).not.toContain('查看');
    expect(ok.body).not.toContain('<span>编辑</span>');
    expect(ok.body).not.toContain('<span>置顶</span>');
    // 写操作弹层化：SSR 不渲染行内/常量表单本体
    expect(ok.body).not.toContain('action="?/update"');
    expect(ok.body).not.toContain('action="?/create"');

    const ni = render(AdminBoards, { props: { data: { loadState: { state: 'not_implemented', message: 'x' } } } });
    expect(ni.body).toContain('开发中');
    // 「新建板块」触发按钮保留（表单在 Dialog 内，无 JS 不提交）。
    expect(ni.body).toContain('新建板块');
    expect(ni.body).not.toContain('action="?/create"');
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
  it('标签页：not_implemented 状态 + 新建标签触发按钮', () => {
    const { body } = render(AdminTags, { props: { data: { loadState: { state: 'not_implemented', message: 'x' } } } });
    expect(body).toContain('标签列表接口开发中');
    // 写操作弹层化：SSR 只渲染「新建标签」触发按钮（创建表单收进 Dialog）。
    expect(body).toContain('新建标签');
    expect(body).not.toContain('action="?/create"');
  });

  it('标签页 ok 状态：每行一个「⋮」三点菜单（约定 D），平铺 编辑/启停/合并 按钮不再渲染', () => {
    const tag = {
      id: 't1',
      slug: 'svelte',
      name: 'Svelte',
      description: null,
      color: null,
      group_id: null,
      usage_count: 5,
      is_active: true,
      status: null,
      updated_at: 7
    };
    const { body } = render(AdminTags, { props: { data: { loadState: { state: 'ok', items: [tag] } } } });
    // 行数据渲染；「查看」已收进「⋯」菜单（关闭态不进 SSR HTML）
    expect(body).toContain('Svelte');
    expect(body).not.toContain('查看');
    // 新契约（约定 D）：唯一「⋮」三点入口（行语义 aria-label）；菜单关闭态列表不进 SSR
    expect(body).toContain('aria-label="更多操作：标签 Svelte"');
    expect(body).not.toContain('row-actions__menu');
    // 旧平铺写按钮断言保留（含「编辑/停用/合并」菜单项不渲染的关闭态守卫）
    expect(body).not.toContain('<span>编辑</span>');
    expect(body).not.toContain('<span>停用</span>');
    expect(body).not.toContain('<span>合并</span>');
    expect(body).not.toContain('合并到其他标签');
    // 行操作 Dialog 关闭时不渲染三节表单（?/update / ?/toggle / ?/merge）
    expect(body).not.toContain('action="?/update"');
    expect(body).not.toContain('action="?/toggle"');
    expect(body).not.toContain('action="?/merge"');
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
    const { body } = render(AdminAssignments, {
      props: { data: { loadState: { state: 'not_implemented', message: 'x' }, q: '', users: null, selectedUser: null, userError: null } }
    });
    expect(body).toContain('角色委派');
    expect(body).toContain('所有授予与撤销都会写入审计日志');
    expect(body).toContain('角色委派接口开发中');
    expect(body).not.toContain('board_role_assignments');
    expect(body).not.toContain('M13-ADMIN');
  });

  it('角色委派页：ok 状态渲染搜索表单 + 可授予角色', () => {
    const { body } = render(AdminAssignments, {
      props: {
        data: {
          loadState: { state: 'ok', items: [{ id: 'r1', name: 'board_moderator' }] },
          q: '',
          users: null,
          selectedUser: null,
          userError: null
        }
      }
    });
    // 用户搜索表单（GET → ?q=）与授予/撤销入口在 SSR 中可用。
    expect(body).toMatch(/<form[^>]*action="\/admin\/assignments"[^>]*method="GET"|<form[^>]*method="GET"[^>]*action="\/admin\/assignments"/);
    expect(body).toContain('查找用户');
    expect(body).toContain('板块版主');
  });
});
