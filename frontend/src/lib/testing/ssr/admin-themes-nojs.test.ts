// M13-UI-06：管理主题页 SSR 快照（无 JS 退化 + 版本冲突 + 隐私守卫）。
//
// - ok → 主题列表（状态徽章/隔离态/站点默认）+ 上传表单（reason 必填）+
//   Token 编辑（If-Match revision）+ 预览；
// - 409 → 版本冲突提示；
// - 隐私守卫：对抗性 Token（CSS/JS/SVG/远程资源/Secret）不进入 HTML，
//   不写入 localStorage/sessionStorage（由 projection 单测覆盖）。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import AdminThemesPage from '../../../routes/admin/themes/+page.svelte';
import type { AdminThemesPageData } from '../../../routes/admin/themes/+page.server';

const okData: AdminThemesPageData = {
  state: 'ok',
  themes: [
    {
      name: 'midnight',
      display_name: 'Midnight',
      kind: 'data',
      schema_version: 1,
      version: '1.0.0',
      supports: '>=1.0 <2.0',
      status: 'active',
      is_default: true,
      revision: 2,
      tokens: {
        'color.background': '#0f172a',
        'color.surface': '#1e293b',
        'color.text': '#e2e8f0',
        'color.accent': '#38bdf8'
      },
      created_by: 'admin',
      updated_at: 1700000000000
    },
    {
      name: 'paper',
      display_name: 'Paper',
      kind: 'data',
      schema_version: 1,
      version: '1.0.0',
      supports: '>=1.0 <2.0',
      status: 'disabled',
      is_default: false,
      revision: 1,
      tokens: {},
      created_by: 'admin',
      updated_at: 1700000000000
    }
  ],
  error: null,
  preview: {
    name: 'midnight',
    revision: 2,
    tokens: { 'color.background': '#0f172a' }
  }
};

const forbiddenData: AdminThemesPageData = {
  state: 'forbidden',
  themes: null,
  error: 'forbidden',
  preview: null
};

describe('M13-UI-06 管理主题 SSR', () => {
  it('ok → 主题列表 + 状态徽章 + 隔离态 + 版本号', () => {
    const { body } = render(AdminThemesPage, { props: { data: okData, form: null } });
    expect(body).toContain('Midnight');
    expect(body).toContain('站点默认');
    expect(body).toContain('隔离（disabled）');
    expect(body).toContain('revision v2');
    expect(body).toContain('/midnight');
  });

  it('ok → 上传表单（reason 必填）+ Token 编辑（If-Match revision）', () => {
    const { body } = render(AdminThemesPage, { props: { data: okData, form: null } });
    expect(body).toContain('action="?/upload"');
    expect(body).toContain('name="reason"');
    expect(body).toContain('action="?/save-settings"');
    expect(body).toContain('name="revision"');
    expect(body).toContain('value="2"');
  });

  it('409 版本冲突 → 提示刷新', () => {
    const { body } = render(AdminThemesPage, {
      props: {
        data: okData,
        form: { conflict: true, message: '版本冲突：请刷新后重试' }
      }
    });
    expect(body).toContain('版本已变化，请刷新页面后重试');
  });

  it('403 → 无权限态（权限不足不泄漏任何主题数据）', () => {
    const { body } = render(AdminThemesPage, { props: { data: forbiddenData, form: null } });
    expect(body).toContain('无权限');
    expect(body).not.toContain('midnight');
  });

  it('隐私守卫：对抗性 Token（CSS/JS/Secret）不进入 HTML', () => {
    const adversarial = {
      state: 'ok',
      error: null,
      preview: null,
      themes: [
        {
          name: 'evil',
          display_name: 'Evil',
          kind: 'data',
          schema_version: 1,
          version: '1.0.0',
          supports: '>=1.0 <2.0',
          status: 'active',
          is_default: false,
          revision: 1,
          tokens: {
            'color.background': '</style><svg onload=alert(1)>',
            provider_secret: 'ADMIN-THEME-SSR-SECRET',
            internal_body: 'ADMIN-THEME-SSR-BODY'
          },
          created_by: 'hacker',
          updated_at: 1700000000000
        }
      ]
    } as unknown as AdminThemesPageData;
    const { body } = render(AdminThemesPage, { props: { data: adversarial, form: null } });
    // 非封闭 key（provider_secret/internal_body）在前端投影被过滤，绝不进入 HTML
    expect(body).not.toContain('ADMIN-THEME-SSR-SECRET');
    expect(body).not.toContain('ADMIN-THEME-SSR-BODY');
    // 封闭 key 的异常值只出现在 textarea 值里且被 HTML 转义（不产生原始标签）
    expect(body).not.toContain('</style><svg');
    expect(body).toContain('&lt;/style>');
    expect(body).not.toContain('style="color:'); // 不从 token 生成内联样式
  });
});
