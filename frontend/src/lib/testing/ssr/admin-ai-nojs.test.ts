// M09-UI-06：管理端 AI SSR 快照。
//
// - not_implemented → 开发中态（后端未实现时核心论坛不受影响）；
// - ok → 配置表单（If-Match 版本 + reason 必填）、Provider 脱敏状态、
//   任务重试/取消表单（reason 必填审计）；
// - 隐私守卫：对抗性 Provider（密钥明文）不进入 SSR HTML；
// - 403 → 无权限态。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import AdminAiPage from '../../../routes/admin/ai/+page.svelte';
import type { AdminAiPageData } from '../../../routes/admin/ai/+page.server';

const clientRequestId = 'req-key-0000000000000001';

const okData: AdminAiPageData = {
  state: 'ok',
  clientRequestId,
  error: null,
  config: {
    enabled: true,
    data_mode: 'redacted',
    purposes: ['formatting', 'seo', 'tagging', 'moderation'],
    version: 4,
    providers: [
      {
        id: 'prov-1',
        name: '测试提供商',
        api_type: 'openai-compatible',
        model: 'test-model',
        secret_configured: true,
        available: true,
        purposes: ['formatting']
      }
    ],
    budgets: { per_user_daily_tokens: 1000, site_daily_tokens: 100000 },
    flags: { formatting: true, seo: true, tagging: false, moderation: false }
  },
  tasks: [
    {
      id: 't-1',
      task_type: 'formatting',
      status: 'dead',
      created_at: 1700000000000,
      error_code: 'provider_5xx',
      error_message: '脱敏错误'
    },
    {
      id: 't-2',
      task_type: 'seo',
      status: 'running',
      created_at: 1700000000000
    }
  ]
};

const notImplementedData: AdminAiPageData = {
  state: 'not_implemented',
  clientRequestId,
  error: 'not implemented',
  config: null,
  tasks: []
};

describe('M09-UI-06 管理端 AI SSR', () => {
  it('not_implemented → 开发中态（不影响核心论坛）', () => {
    const { body } = render(AdminAiPage, { props: { data: notImplementedData, form: null } });
    expect(body).toContain('AI 管理接口开发中');
    expect(body).toContain('核心论坛功能不受影响');
  });

  it('ok → 配置表单：If-Match 版本、操作原因必填、数据策略选项', () => {
    const { body } = render(AdminAiPage, { props: { data: okData, form: null } });
    expect(body).toMatch(/<form[^>]*action="\?\/save"/);
    expect(body).toContain('name="expected_version"');
    expect(body).toContain('value="4"');
    expect(body).toContain('name="reason"');
    expect(body).toContain('必填（写审计）');
    expect(body).toContain('disabled（不发送）');
    expect(body).toContain('full_with_consent（逐次同意）');
    expect(body).toContain('name="flag_formatting"');
    expect(body).toContain('每用户每日 token 预算');
  });

  it('Provider 脱敏状态（密钥仅布尔，不回显）', () => {
    const { body } = render(AdminAiPage, { props: { data: okData, form: null } });
    expect(body).toContain('测试提供商');
    expect(body).toContain('密钥已配置');
    expect(body).toContain('密钥只写入受保护 Secret Store');
    expect(body).not.toContain('sk-');
  });

  it('任务表：重试/取消表单（reason 必填）', () => {
    const { body } = render(AdminAiPage, { props: { data: okData, form: null } });
    expect(body).toContain('t-1');
    expect(body).toContain('provider_5xx');
    expect(body).toMatch(/<form[^>]*action="\?\/retry"/);
    expect(body).toMatch(/<form[^>]*action="\?\/cancel"/);
    expect(body).toContain('name="task_id"');
    expect(body).toContain('重试');
    expect(body).toContain('取消');
  });

  it('隐私守卫：对抗性 Provider（密钥明文/内部字段）不进入 HTML', () => {
    // 对抗性输入：以变量扩展注入内部字段（类型层无这些字段，纯渲染守卫验证）。
    const adversarial = {
      ...okData,
      config: {
        ...okData.config!,
        providers: [
          {
            id: 'prov-2',
            name: '对抗提供商',
            secret_configured: true,
            api_key: 'ADMIN-AI-SSR-KEY',
            access_key_secret: 'ADMIN-AI-SSR-SECRET'
          }
        ]
      }
    } as unknown as AdminAiPageData;
    const { body } = render(AdminAiPage, { props: { data: adversarial, form: null } });
    expect(body).not.toContain('ADMIN-AI-SSR-KEY');
    expect(body).not.toContain('ADMIN-AI-SSR-SECRET');
  });

  it('403 → 无权限态', () => {
    const { body } = render(AdminAiPage, {
      props: { data: { state: 'forbidden', clientRequestId, error: 'forbidden', config: null, tasks: [] }, form: null }
    });
    expect(body).toContain('没有权限访问 AI 管理');
  });
});
