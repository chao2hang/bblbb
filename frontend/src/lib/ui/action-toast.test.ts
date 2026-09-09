// 管理端表单动作 → 全局 Toast 共享反馈（withActionToast / toastActionResult）。
// 验证：成功/失败文案弹对应类型、reset 语义、无 message 不弹、自定义 type/detail。
import { afterEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
import { toasts, dismiss } from '$lib/ui/toast';
import { withActionToast, toastActionResult } from './action-toast';
import type { ActionResult } from '@sveltejs/kit';

function clearToasts(): void {
  for (const t of get(toasts)) dismiss(t.id);
}

function result(type: 'success' | 'failure' | 'redirect', data: unknown): ActionResult {
  return { type, status: type === 'success' ? 200 : type === 'failure' ? 422 : 303, data } as ActionResult;
}

/** 模拟 use:enhance 两段式调用：提交事件 → 结果处理函数。 */
async function run(handler: ReturnType<typeof withActionToast>, r: ActionResult, update: (o?: { reset?: boolean }) => Promise<void>): Promise<void> {
  const inner = handler({} as never) as (e: { result: ActionResult; update: typeof update }) => Promise<void>;
  await inner({ result: r, update });
}

afterEach(() => {
  clearToasts();
  vi.useRealTimers();
});

describe('toastActionResult：从 action 结果弹 Toast', () => {
  it('success 带 message → success 类型 Toast', () => {
    const fired = toastActionResult(result('success', { message: '保存成功' }));
    expect(fired).toBe(true);
    const list = get(toasts);
    expect(list).toHaveLength(1);
    expect(list[0].type).toBe('success');
    expect(list[0].message).toBe('保存成功');
  });

  it('failure 带 message → danger 类型 Toast', () => {
    toastActionResult(result('failure', { message: '版本冲突，请刷新后重试' }));
    const list = get(toasts);
    expect(list).toHaveLength(1);
    expect(list[0].type).toBe('danger');
  });

  it('无 message / 空 message / redirect → 不弹', () => {
    expect(toastActionResult(result('success', null))).toBe(false);
    expect(toastActionResult(result('success', { message: '' }))).toBe(false);
    expect(toastActionResult(result('redirect', { status: 303, location: '/x' }))).toBe(false);
    expect(get(toasts)).toHaveLength(0);
  });

  it('自定义 type 与 detail 透传', () => {
    toastActionResult(result('failure', { message: '连接失败' }), {
      type: () => 'warning',
      detail: (d) => String((d as Record<string, unknown>)?.note ?? '')
    });
    const list = get(toasts);
    expect(list[0].type).toBe('warning');
    expect(list[0].detail).toBe('');
  });
});

describe('withActionToast：use:enhance 共享回调', () => {
  it('成功后默认 update()（跟随默认 reset 语义）并弹 success Toast', async () => {
    const update = vi.fn(async () => {});
    await run(withActionToast(), result('success', { message: '已保存' }), update);
    expect(update).toHaveBeenCalledTimes(1);
    expect(update).not.toHaveBeenCalledWith({ reset: false });
    expect(get(toasts)[0].type).toBe('success');
  });

  it('reset: false → update({ reset: false }) 保留输入，失败弹 danger Toast', async () => {
    const update = vi.fn(async () => {});
    await run(withActionToast({ reset: false }), result('failure', { message: '操作原因必填' }), update);
    expect(update).toHaveBeenCalledWith({ reset: false });
    expect(get(toasts)[0].type).toBe('danger');
  });
});
