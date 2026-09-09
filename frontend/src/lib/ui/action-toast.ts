// 管理端表单动作 → 全局 Toast 共享反馈（产品约定：操作结果用浮窗消息提示，
// 不占页面主体；页面顶部的内联横幅仅保留为无 JS 回退，SSR HTML 中仍渲染）。
//
// 用法（form 上，use:enhance 的 value 必须是“提交事件 → 结果处理函数”）：
//   <form use:enhance={withActionToast()}>                  // 成功后重置表单（与默认 enhance 一致）
//   <form use:enhance={withActionToast({ reset: false })}>  // 成功后保留已填输入（如 S3 凭据草稿）
//   <form use:enhance={withActionToast({ message: (d) => d?.message ?? d?.error })}>
//
// 已有自定义 enhance 回调的表单：在结果处理函数内调用 `toastActionResult(result)`，
// 避免重复弹（它弹过的 form message 不要再用 showToast 弹第二次）。
import type { ActionResult } from '@sveltejs/kit';
import { show } from './toast';
import type { ToastType } from './toast';

/** 动作结果 Toast 的展示时长（比默认 3.2s 略长，便于读完错误详情）。 */
export const ACTION_TOAST_DURATION = 4500;

export interface ActionToastOptions {
  /** 成功后是否重置表单；undefined = 跟随 SvelteKit 默认（成功重置、失败保留）。 */
  reset?: boolean;
  /** 自定义 Toast 类型；默认 success → 'success'、failure → 'danger'。 */
  type?: (data: Record<string, unknown> | null) => ToastType;
  /** 自定义文案提取；默认读取 data.message（个别页面错误放在 data.error）。 */
  message?: (data: Record<string, unknown> | null) => string | null | undefined;
  /** 展示时长（毫秒）；默认 ACTION_TOAST_DURATION。 */
  duration?: number;
  /** detail 第二行小字（如诊断详情）；为空则单行。 */
  detail?: (data: Record<string, unknown> | null) => string | undefined;
}

/** enhance 结果处理函数能拿到、本模块关心的字段。 */
export interface ActionFormEvent {
  result: ActionResult;
  update: (opts?: { reset?: boolean }) => Promise<void>;
}

/**
 * 从 action 结果提取 message 并弹全局 Toast。
 * 返回 true 表示已弹（调用方不要对同一条消息再弹第二次）。
 */
export function toastActionResult(
  result: ActionResult,
  opts: Pick<ActionToastOptions, 'type' | 'duration' | 'detail' | 'message'> = {}
): boolean {
  if (result.type !== 'success' && result.type !== 'failure') return false;
  const data = (result.data ?? null) as Record<string, unknown> | null;
  const message = opts.message
    ? (opts.message(data) ?? null)
    : typeof data?.message === 'string' && data.message
      ? data.message
      : null;
  if (!message) return false;
  const type = opts.type ? opts.type(data) : result.type === 'success' ? 'success' : 'danger';
  show(message, type, opts.duration ?? ACTION_TOAST_DURATION, opts.detail?.(data));
  return true;
}

/**
 * 管理端表单 use:enhance 共享回调：
 * 返回“提交事件 → 结果处理函数”（SubmitFunction 形态）；
 * 结果 message → 全局 Toast；随后按原有 reset 语义 update() 应用动作结果。
 */
export function withActionToast(opts: ActionToastOptions = {}) {
  return (_e: unknown) => async ({ result, update }: ActionFormEvent): Promise<void> => {
    toastActionResult(result, opts);
    if (opts.reset === false) {
      await update({ reset: false });
    } else {
      await update();
    }
  };
}
