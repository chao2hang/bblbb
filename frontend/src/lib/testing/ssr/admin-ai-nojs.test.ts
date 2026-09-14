// M09-UI-06：管理端 AI SSR 快照。
//
// - not_implemented → 开发中态（后端未实现时核心论坛不受影响）；
// - ok → 策略/渠道/任务写操作均为「按钮 + 弹层」（约定 A：写表单不进 SSR HTML）；
//   M18-ADMIN-OPS（约定 D）：Provider 行「测试/编辑/删除」与任务行「重试/取消」
//   收敛为每行一个「⋮」三点菜单（RowActionsMenu，aria-label 含行语义），点菜单项
//   打开对应单动作确认 Dialog，编辑/测试/删除（渠道）与重试/取消（任务）写表单
//   与关闭态菜单列表全部不进 SSR HTML；
//   Provider 脱敏状态、任务行数据与批量选择列照常渲染；
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

  it('ok → 全站策略 = 「编辑全局策略」按钮 + 弹层（SSR 不渲染写表单），概览保留', () => {
    const { body } = render(AdminAiPage, { props: { data: okData, form: null } });
    // 策略写操作入口 = 按钮；?/save 写表单收进弹层，不进 SSR HTML（约定 A）
    expect(body).toContain('编辑全局策略');
    expect(body).not.toMatch(/<form[^>]*action="\?\/save"/);
    // 策略概览（脱敏投影）照常渲染：数据模式 / 启用态 / 预算
    expect(body).toContain('全站 AI 策略与预算');
    expect(body).toContain('redacted');
    expect(body).toContain('全站 AI 能力已启用');
    expect(body).toContain('100000');
  });

  it('Provider 脱敏状态（密钥仅布尔，不回显）', () => {
    const { body } = render(AdminAiPage, { props: { data: okData, form: null } });
    expect(body).toContain('测试提供商');
    expect(body).toContain('密钥已配置');
    expect(body).toContain('密钥只写入受保护 Secret Store');
    expect(body).not.toContain('sk-');
  });

  it('渠道行 = 每行一个「⋮」动作菜单（约定 D）：编辑/测试/删除写表单收进弹层（不进 SSR HTML）', () => {
    const { body } = render(AdminAiPage, { props: { data: okData, form: null } });
    expect(body).toContain('测试提供商');
    // 每行唯一「⋮」三点触发按钮（aria-label 含行语义）；旧平铺按钮（测试/编辑/删除）
    // 与关闭态菜单列表（role="menu"）均不进 SSR HTML
    expect(body).toContain('aria-label="更多操作：渠道 测试提供商"');
    expect(body).not.toContain('>测试</span>');
    expect(body).not.toContain('>编辑</span>');
    expect(body).not.toContain('>删除</span>');
    expect(body).not.toContain('role="menu"');
    // 页头「添加渠道」入口保留（页级按钮，非行内写操作）
    expect(body).toContain('添加渠道');
    // saveProvider / test / deleteProvider 写表单全部收进关闭态弹层，不进 SSR HTML
    expect(body).not.toMatch(/<form[^>]*action="\?\/saveProvider"/);
    expect(body).not.toMatch(/<form[^>]*action="\?\/test"/);
    expect(body).not.toMatch(/<form[^>]*action="\?\/deleteProvider"/);
  });

  it('渠道列表为空时展示添加指引，无假数据', () => {
    const emptyProviderData: AdminAiPageData = {
      ...okData,
      config: {
        ...okData.config!,
        providers: []
      }
    };
    const { body } = render(AdminAiPage, { props: { data: emptyProviderData, form: null } });
    expect(body).toContain('尚未配置任何 AI 模型渠道');
    expect(body).toContain('添加第一个渠道');
    expect(body).not.toContain('受控 Gateway');
    expect(body).not.toContain('本地 Ollama');
  });

  it('任务行 = 每行一个「⋮」动作菜单（约定 D）；选择列与批量重试入口保留', () => {
    const { body } = render(AdminAiPage, { props: { data: okData, form: null } });
    // 任务行数据照常渲染
    expect(body).toContain('t-1');
    expect(body).toContain('provider_5xx');
    // 每行唯一「⋮」三点触发按钮（aria-label 含行语义）；旧平铺「重试/取消」按钮
    // 与关闭态菜单列表（role="menu"）不进 SSR HTML
    expect(body).toContain('aria-label="更多操作：任务 t-1"');
    expect(body).not.toContain('>重试</span>');
    expect(body).not.toContain('>取消</span>');
    expect(body).not.toContain('role="menu"');
    // 选择列（aria-label 明确）
    expect(body).toContain('aria-label="全选任务"');
    expect(body).toContain('aria-label="选中任务 t-1"');
    // 重试/取消/批量重试写表单均在关闭态弹层内，不进 SSR HTML
    expect(body).not.toMatch(/<form[^>]*action="\?\/retry"/);
    expect(body).not.toMatch(/<form[^>]*action="\?\/cancel"/);
    expect(body).not.toMatch(/<form[^>]*action="\?\/batchRetry"/);
  });

  it('P1-08: 渠道停用状态映射与默认渠道排除', () => {
    const disabledProviderData: AdminAiPageData = {
      ...okData,
      config: {
        ...okData.config!,
        providers: [
          {
            id: 'prov-disabled',
            name: '停用渠道',
            api_type: 'openai_compatible',
            base_url: 'https://api.openai.com/v1',
            model: 'gpt-4o-mini',
            status: 'disabled',
            secret_configured: false,
            available: false
          }
        ]
      }
    };
    const { body } = render(AdminAiPage, { props: { data: disabledProviderData, form: null } });
    expect(body).toContain('已停用');
    expect(body).not.toContain('默认渠道');
    expect(body).toContain('暂无可用启用渠道');
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
