// M09-UI-04/05：AI 建议页 SSR 快照。
//
// - formatting：diff 预览（-/+ 行）与字段级采纳表单（expected_base_version +
//   selected_field，POST ?/accept）；
// - 409 版本冲突 → 冲突提示 + 重新加载入口；
// - moderation：只展示公开合规摘要；内部 Prompt/举报/证据字段不进入 SSR HTML；
// - 404/403 安全态。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import SuggestionPage from '../../../routes/ai/suggestions/[id]/+page.svelte';
import type { AiSuggestionPageData } from '../../../routes/ai/suggestions/[id]/+page.server';
import type { AiSuggestion } from '$lib/api/types';

const formattingSuggestion: AiSuggestion = {
  id: 's-1',
  type: 'formatting',
  status: 'pending',
  base_version: 5,
  created_at: 1700000000000,
  fields: [
    { field: 'title', current: '旧标题', proposed: '新标题', reason: '更简洁', selectable: true },
    { field: 'markdown', current: '# 旧\n正文', proposed: '# 新\n正文内容', reason: '层级修复', selectable: true }
  ]
};

const moderationSuggestion: AiSuggestion = {
  id: 's-2',
  type: 'moderation',
  status: 'pending',
  base_version: 2,
  created_at: 1700000000000,
  fields: [],
  moderation: { target_type: 'post', summary: '公开合规摘要：未发现明显违规信号' }
};

describe('M09-UI-04 AI 建议 SSR（formatting）', () => {
  it('渲染 diff 预览（-/+ 行）与字段级采纳表单', () => {
    const { body } = render(SuggestionPage, {
      props: { data: { suggestion: formattingSuggestion, forbidden: false, notFound: false, error: null }, form: null }
    });
    expect(body).toContain('旧标题');
    expect(body).toContain('新标题');
    expect(body).toContain('- 正文');
    expect(body).toContain('+ 正文内容');
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/accept"/);
    expect(body).toContain('name="expected_base_version"');
    expect(body).toContain('value="5"');
    expect(body).toContain('name="selected_field"');
    expect(body).toContain('采纳此字段');
    expect(body).toContain('基于内容版本 v5');
  });

  it('409 版本冲突 → 冲突提示与重新加载链接', () => {
    const { body } = render(SuggestionPage, {
      props: {
        data: { suggestion: formattingSuggestion, forbidden: false, notFound: false, error: null },
        form: { conflict: true, message: '内容已更新，建议已过期。加载最新建议后再采纳。' }
      }
    });
    expect(body).toContain('版本冲突');
    expect(body).toContain('href="/ai/suggestions/s-1"');
    expect(body).toContain('重新加载');
  });

  it('已采纳态展示', () => {
    const { body } = render(SuggestionPage, {
      props: {
        data: { suggestion: { ...formattingSuggestion, status: 'accepted' }, forbidden: false, notFound: false, error: null },
        form: null
      }
    });
    expect(body).toContain('已采纳');
  });
});

describe('M09-UI-05 AI 建议 SSR（moderation 信息边界）', () => {
  it('只展示公开合规摘要，不渲染任何内部字段', () => {
    // 对抗性输入：内部字段以变量扩展注入（类型层无这些字段，纯渲染守卫验证）。
    const adversarial = {
      ...moderationSuggestion,
      moderation: {
        target_type: 'post',
        summary: '公开合规摘要：未发现明显违规信号'
      },
      internal_prompt: 'MOD-SSR-PROMPT',
      report_evidence: 'MOD-SSR-EVIDENCE',
      risk_signals: ['MOD-SSR-RISK']
    };
    const { body } = render(SuggestionPage, {
      props: { data: { suggestion: adversarial, forbidden: false, notFound: false, error: null }, form: null }
    });
    expect(body).toContain('审核建议（仅审核人员可见）');
    expect(body).toContain('公开合规摘要：未发现明显违规信号');
    expect(body).toContain('内部 Prompt、模型原始输出与举报证据属于审核内部信息');
    expect(body).not.toContain('MOD-SSR-PROMPT');
    expect(body).not.toContain('MOD-SSR-EVIDENCE');
    expect(body).not.toContain('MOD-SSR-RISK');
    expect(body).not.toContain('采纳此字段'); // moderation 不提供字段级采纳
  });
});

describe('M09-UI-04/05 AI 建议页安全态', () => {
  it('404 → 安全提示', () => {
    const { body } = render(SuggestionPage, {
      props: { data: { suggestion: null, forbidden: false, notFound: true, error: null }, form: null }
    });
    expect(body).toContain('建议不存在或已被移除');
  });

  it('403 → 无权限提示', () => {
    const { body } = render(SuggestionPage, {
      props: { data: { suggestion: null, forbidden: true, notFound: false, error: 'forbidden' }, form: null }
    });
    expect(body).toContain('没有权限查看该建议');
  });
});
