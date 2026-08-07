// M09-UI-01/03：AI 能力页 SSR 快照。
//
// - 默认关闭（disabled）→ 关闭说明，不承诺功能可用；
// - 启用 → Provider 脱敏状态（Secret 仅布尔）、数据模式、同意记录与撤回表单；
// - 隐私守卫：对抗性 Provider（含密钥明文）不进入 SSR HTML；
// - 403 → 无权限态。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import AiPage from '../../../routes/ai/+page.svelte';
import type { AiPageData } from '../../../routes/ai/+page.server';

const disabledData: AiPageData = {
  state: 'disabled',
  disabledMessage: 'AI 能力当前未开放（默认关闭）。你的内容不会被发送给任何外部 AI 提供商，普通发帖与审核不受影响。',
  capabilities: null,
  error: null
};

const enabledData: AiPageData = {
  state: 'ok',
  disabledMessage: null,
  capabilities: {
    enabled: true,
    data_mode: 'full_with_consent',
    purposes: ['formatting', 'seo', 'tagging', 'moderation'],
    synchronous: true,
    providers: [
      {
        id: 'prov-1',
        name: '测试提供商',
        secret_configured: true,
        available: true,
        model: 'test-model',
        purposes: ['formatting'],
        retention: '不保存',
        training: '不用于训练',
        region: '境内'
      }
    ],
    consents: [
      {
        provider_id: 'prov-1',
        provider_name: '测试提供商',
        purpose: 'formatting',
        data_mode: 'full_with_consent',
        disclosure_version: 3,
        disclosure_hash: 'deadbeef',
        granted_at: 1700000000000
      }
    ]
  },
  error: null
};

describe('M09-UI-01 AI 能力页 SSR', () => {
  it('默认关闭 → 关闭说明 + 不承诺可用', () => {
    const { body } = render(AiPage, { props: { data: disabledData, form: null } });
    expect(body).toContain('AI 功能未开放');
    expect(body).toContain('默认关闭');
    expect(body).toContain('不会发送给任何外部 AI 提供商');
    expect(body).toContain('普通发帖、编辑与人工审核不受影响');
  });

  it('启用 → 渲染 Provider 脱敏状态（密钥仅布尔）', () => {
    const { body } = render(AiPage, { props: { data: enabledData, form: null } });
    expect(body).toContain('测试提供商');
    expect(body).toContain('密钥已配置');
    expect(body).toContain('不保存');
    expect(body).not.toContain('sk-');
    expect(body).not.toContain('secret');
    expect(body).toContain('密钥只保存在受保护的 Secret Store');
  });

  it('同意记录：版本/hash 与原生撤回表单（POST ?/revoke）', () => {
    const { body } = render(AiPage, { props: { data: enabledData, form: null } });
    expect(body).toContain('v3');
    expect(body).toContain('deadbeef');
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/revoke"/);
    expect(body).toContain('name="provider_id"');
    expect(body).toContain('name="purpose"');
    expect(body).toContain('name="disclosure_version"');
    expect(body).toContain('撤回同意');
  });

  it('隐私守卫：对抗性 Provider（密钥明文/内部字段）不进入 HTML', () => {
    // 对抗性输入用 as unknown 注入（组件/类型层无这些字段，纯渲染守卫验证）。
    const adversarial = {
      state: 'ok',
      disabledMessage: null,
      capabilities: {
        enabled: true,
        data_mode: 'full_with_consent',
        providers: [
          {
            id: 'prov-2',
            name: '对抗提供商',
            secret_configured: true,
            api_key: 'AI-SSR-SECRET-KEY',
            access_token: 'AI-SSR-TOKEN',
            prompt: '内部 prompt 不应外显'
          }
        ]
      },
      error: null
    } as unknown as AiPageData;
    const { body } = render(AiPage, { props: { data: adversarial, form: null } });
    expect(body).not.toContain('AI-SSR-SECRET-KEY');
    expect(body).not.toContain('AI-SSR-TOKEN');
    expect(body).not.toContain('内部 prompt 不应外显');
  });

  it('403 → 无权限态', () => {
    const { body } = render(AiPage, {
      props: { data: { state: 'forbidden', disabledMessage: null, capabilities: null, error: 'forbidden' }, form: null }
    });
    expect(body).toContain('没有权限访问 AI 能力状态');
  });
});
