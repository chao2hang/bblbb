// M13-UI-06/PLUGIN-07：管理插件页 SSR 快照（无 JS 退化 + 能力边界 + 隐私守卫）。
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
  it('ok → 插件列表 + 能力徽章 + 订阅 + policy 版本 + 启停按钮', () => {
    const { body } = render(AdminPluginsPage, { props: { data: okData, form: null } });
    expect(body).toContain('新用户欢迎奖励');
    expect(body).toContain('/welcome-reward');
    expect(body).toContain('notification.create');
    expect(body).toContain('points.award');
    expect(body).toContain('user.verified.v1');
    expect(body).toContain('policy v3');
    expect(body).toContain('action="?/disable"');
  });

  it('ok → 能力边界说明（v1 无在线代码执行；受控 Provider Adapter）', () => {
    const { body } = render(AdminPluginsPage, { props: { data: okData, form: null } });
    expect(body).toContain('v2');
    expect(body).toContain('direct');
    expect(body).toContain('hls');
    expect(body).toContain('xigua');
  });

  it('安装表单：ID/能力/订阅/schema/reason 全部原生表单（无 JS 可用）', () => {
    const { body } = render(AdminPluginsPage, { props: { data: okData, form: null } });
    expect(body).toContain('action="?/install"');
    expect(body).toContain('name="capabilities"');
    expect(body).toContain('name="subscriptions"');
    expect(body).toContain('name="settings_schema"');
    expect(body).toContain('name="reason"');
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
