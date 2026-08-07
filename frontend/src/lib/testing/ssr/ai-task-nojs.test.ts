// M09-UI-03：AI 任务页 SSR 快照。
//
// - 处理中（queued/running）→ 原生取消表单（POST ?/cancel + 幂等键）；
// - 成功 → 建议入口链接；取消/失败 → 对应状态（只显示稳定码与脱敏信息）；
// - 隐私守卫：对抗性任务（含内部 Prompt/Provider 原文）不进入 SSR HTML；
// - 404/403 安全态。
import { describe, expect, it, vi } from 'vitest';
import { render } from 'svelte/server';
import AiTaskPage from '../../../routes/ai/tasks/[id]/+page.svelte';
import type { AiTaskPageData } from '../../../routes/ai/tasks/[id]/+page.server';
import type { AiTask } from '$lib/api/types';

vi.mock('$app/state', () => ({
  page: { url: new URL('http://test.local/ai/tasks/t-1') }
}));

const baseData = (task: AiTask | null, overrides: Partial<AiTaskPageData> = {}): AiTaskPageData => ({
  task,
  forbidden: false,
  notFound: false,
  error: null,
  clientRequestId: 'req-key-0000000000000001',
  ...overrides
});

const runningTask: AiTask = {
  id: 't-1',
  task_type: 'formatting',
  status: 'running',
  source_revision: 3,
  policy_version: 2,
  created_at: 1700000000000,
  started_at: 1700000001000
};

describe('M09-UI-03 AI 任务页 SSR', () => {
  it('处理中 → 状态徽章 + 原生取消表单（幂等键）', () => {
    const { body } = render(AiTaskPage, { props: { data: baseData(runningTask), form: null } });
    expect(body).toContain('处理中');
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/cancel"/);
    expect(body).toContain('name="client_request_id"');
    expect(body).toContain('value="req-key-0000000000000001"');
    expect(body).toContain('取消任务');
    expect(body).toContain('内容版本 v3');
  });

  it('成功 → 建议入口链接', () => {
    const { body } = render(AiTaskPage, {
      props: {
        data: baseData({
          id: 't-2',
          task_type: 'seo',
          status: 'succeeded',
          created_at: 1700000000000,
          suggestion_id: 's-9',
          finished_at: 1700000002000
        }),
        form: null
      }
    });
    expect(body).toContain('已完成');
    expect(body).toContain('href="/ai/suggestions/s-9"');
    expect(body).toContain('查看建议');
  });

  it('取消/失败 → 状态与脱敏错误信息', () => {
    const cancelled = render(AiTaskPage, {
      props: { data: baseData({ id: 't-3', task_type: 'formatting', status: 'cancelled', created_at: 0 }), form: null }
    });
    expect(cancelled.body).toContain('已取消');
    expect(cancelled.body).toContain('不再生成建议');

    const dead = render(AiTaskPage, {
      props: {
        data: baseData({
          id: 't-4',
          task_type: 'formatting',
          status: 'dead',
          created_at: 0,
          error_code: 'provider_5xx',
          error_message: '脱敏错误'
        }),
        form: null
      }
    });
    expect(dead.body).toContain('provider_5xx');
    expect(dead.body).toContain('脱敏错误');
  });

  it('隐私守卫：对抗性任务（内部 Prompt/Provider 原文）不进入 HTML', () => {
    // 对抗性输入：以变量扩展注入内部字段（类型层无这些字段，纯渲染守卫验证）。
    const adversarial = {
      ...{
        id: 't-5',
        task_type: 'formatting',
        status: 'dead',
        created_at: 0,
        error_code: 'provider_5xx',
        error_message: 'TASK-SSR-SECRET'
      },
      provider_response: 'TASK-SSR-PROMPT',
      prompt: 'TASK-SSR-PROMPT'
    };
    const { body } = render(AiTaskPage, { props: { data: baseData(adversarial as unknown as AiTask), form: null } });
    expect(body).not.toContain('TASK-SSR-PROMPT');
  });

  it('404 → 安全提示；403 → 无权限', () => {
    const notFound = render(AiTaskPage, { props: { data: baseData(null, { notFound: true }), form: null } });
    expect(notFound.body).toContain('任务不存在或已被移除');

    const forbidden = render(AiTaskPage, { props: { data: baseData(null, { forbidden: true }), form: null } });
    expect(forbidden.body).toContain('没有权限查看该任务');
  });
});
