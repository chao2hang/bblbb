// M13-UI-06/PLUGIN-07 & M18-ADMIN-BATCH：管理插件页 SSR 快照
// （无 JS 退化 + 按钮→弹层新契约 + 能力边界 + 隐私守卫）。
// 弹层（安装/启用/停用/设置/批量）为客户端交互，无 JS 只渲染触发按钮与列表。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import AdminPluginsPage from '../../../routes/admin/plugins/+page.svelte';
import type { AdminPluginsPageData } from '../../../routes/admin/plugins/+page.server';

const okData: AdminPluginsPageData = {
  state: 'ok',
  error: null,
  plugins: [
    {
      id: 'welcome-reward',
      name: '新用户欢迎奖励',
      version: '1.0.0',
      supports: '>=1.0 <2.0',
      status: 'enabled',
      capabilities: ['notification.create', 'points.award'],
      subscriptions: ['user.verified.v1'],
      settings: { amount: 100 },
      policy_revision: 3,
      created_at: 1700000000000,
      updated_at: 1700000000000
    }
  ],
  capabilities: {
    capabilities: ['notification.create', 'points.award', 'plugin_data.put'],
    events: ['user.verified.v1'],
    service_interface: [],
    provider_adapters: [
      { provider: 'direct', kind: 'core_adapter', managed: true },
      { provider: 'hls', kind: 'core_adapter', managed: true },
      { provider: 'xigua', kind: 'core_adapter', managed: true }
    ],
    v1_execution: 'config_only',
    note: 'code/WASM plugin execution is a v2 research item'
  }
};

describe('M13-UI-06 管理插件 SSR', () => {
  it('ok → 插件列表 + 能力徽章 + 订阅 + policy 版本 + 启停/设置触发按钮', () => {
    const { body } = render(AdminPluginsPage, { props: { data: okData, form: null } });
    expect(body).toContain('新用户欢迎奖励');
    expect(body).toContain('/welcome-reward');
    expect(body).toContain('notification.create');
    expect(body).toContain('points.award');
    expect(body).toContain('user.verified.v1');
    expect(body).toContain('policy v3');
    // 约定 C 新契约：每行唯一「操作」按钮（启用/停用/设置在弹层内选择），
    // 旧平铺行按钮不再出现在 SSR body。
    expect(body).toContain('<span>操作</span>');
    expect(body).not.toContain('<span>停用</span>');
    expect(body).not.toContain('<span>设置</span>');
    expect(body).not.toContain('action="?/disable"');
    // 行选择列 aria 标签（批量操作契约）
    expect(body).toContain('aria-label="选择插件 新用户欢迎奖励"');
  });

  it('ok → 能力边界说明（v1 无在线代码执行；受控 Provider Adapter）', () => {
    const { body } = render(AdminPluginsPage, { props: { data: okData, form: null } });
    expect(body).toContain('v2');
    expect(body).toContain('direct');
    expect(body).toContain('hls');
    expect(body).toContain('xigua');
  });

  it('安装改按钮+弹层：头部触发按钮渲染，弹层表单无 JS 不渲染', () => {
    const { body } = render(AdminPluginsPage, { props: { data: okData, form: null } });
    // 触发按钮（?/install Dialog）
    expect(body).toContain('+ 安装插件');
    // 弹层表单本体不再直接暴露在页面上（无 JS 不执行写操作）
    expect(body).not.toContain('action="?/install"');
    expect(body).not.toContain('name="settings_schema"');
  });

  it('403 → 无权限态不泄漏插件数据', () => {
    const forbidden = render(AdminPluginsPage, {
      props: {
        data: { state: 'forbidden', plugins: null, capabilities: null, error: 'forbidden' },
        form: null
      }
    });
    expect(forbidden.body).toContain('无权限');
    expect(forbidden.body).not.toContain('welcome-reward');
  });

  it('隐私守卫：settings 是插件自身命名空间数据（编辑用）；非白名单字段不进入 HTML', () => {
    const adversarial = {
      state: 'ok',
      error: null,
      capabilities: okData.capabilities,
      plugins: [
        {
          id: 'evil',
          name: 'Evil',
          version: '1.0.0',
          supports: '>=1.0 <2.0',
          status: 'enabled',
          capabilities: ['notification.create'],
          subscriptions: [],
          settings: { amount: 5 },
          policy_revision: 1,
          created_at: 1,
          updated_at: 1,
          settings_schema: { type: 'object', __internal: 'PLUGIN-SSR-SCHEMA-SECRET' },
          internal_body: 'PLUGIN-SSR-PRIVATE-BODY'
        }
      ]
    } as unknown as AdminPluginsPageData;
    const { body } = render(AdminPluginsPage, { props: { data: adversarial, form: null } });
    // DTO 外的内部字段（settings_schema 内部标记/隐藏正文）不进入 SSR HTML
    expect(body).not.toContain('PLUGIN-SSR-SCHEMA-SECRET');
    expect(body).not.toContain('PLUGIN-SSR-PRIVATE-BODY');
  });
});
