// 瞬态服务端错误 → 全局 Toast 提示（产品约定：5xx/429 页面加载失败用消息
// 提示，不整页展示错误态）。本测试验证 announceTransientProblem 的
// 弹提示/去重/非瞬态忽略 行为。
import { afterEach, describe, expect, it } from 'vitest';
import { get } from 'svelte/store';
import { toasts, dismiss } from '$lib/ui/toast';
import { announceTransientProblem } from './problem-toast';

function clearToasts(): void {
  for (const t of get(toasts)) dismiss(t.id);
}

afterEach(() => {
  clearToasts();
});

describe('announceTransientProblem：瞬态错误弹 Toast', () => {
  it('500 internal_error → 一条 danger Toast，文案含稳定中文 + 请求号', () => {
    announceTransientProblem({
      status: 500,
      code: 'internal_error',
      request_id: 'req-123'
    });
    const list = get(toasts);
    expect(list).toHaveLength(1);
    expect(list[0].type).toBe('danger');
    expect(list[0].message).toContain('服务器开小差了');
    expect(list[0].message).toContain('req-123');
  });

  it('同一 problem 实例只弹一次（$effect 重跑/水合去重）', () => {
    const problem = { status: 500, code: 'internal_error' };
    announceTransientProblem(problem);
    announceTransientProblem(problem);
    announceTransientProblem(problem);
    expect(get(toasts)).toHaveLength(1);
  });

  it('新的 problem 实例（新一轮加载失败）→ 再弹一条', () => {
    announceTransientProblem({ status: 500, code: 'internal_error' });
    announceTransientProblem({ status: 503, code: 'service_unavailable' });
    expect(get(toasts)).toHaveLength(2);
  });

  it('429 限流 → danger Toast（操作过于频繁）', () => {
    announceTransientProblem({ status: 429, code: 'rate_limited', retry_after: 30 });
    const list = get(toasts);
    expect(list).toHaveLength(1);
    expect(list[0].type).toBe('danger');
    expect(list[0].message).toContain('操作过于频繁');
  });

  it('持续性错误（401/403/404/409/422）→ 不弹（仍整页 ProblemState）', () => {
    announceTransientProblem({ status: 404, code: 'not_found' });
    announceTransientProblem({ status: 401, code: 'authentication_required' });
    announceTransientProblem({ status: 409, code: 'version_conflict' });
    announceTransientProblem({ status: 422, code: 'validation_failed' });
    expect(get(toasts)).toHaveLength(0);
  });

  it('null / undefined → 不弹', () => {
    announceTransientProblem(null);
    announceTransientProblem(undefined);
    expect(get(toasts)).toHaveLength(0);
  });
});
