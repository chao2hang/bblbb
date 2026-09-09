// 全局壳·轻量 Toast store —— +layout.svelte 挂载 ToastHost 后全站可用。
//
// 用法：`import { show, dismiss, toasts } from '$lib/ui/toast';`
//  - `show(message, type)`：弹一条全局提示（默认 3.2s 自动消失）；
//  - `dismiss(id)`：提前关闭；
//  - `toasts`：writable store，ToastHost 渲染其当前值。
// 本批新增交互（主题切换、通知全部已读等）统一走它反馈；存量页面渐进接入。
import { writable, type Readable } from 'svelte/store';

export type ToastType = 'info' | 'success' | 'warning' | 'danger';

export interface ToastItem {
  id: number;
  message: string;
  type: ToastType;
  /** 可选第二行小字（诊断详情等）；为空则单行渲染。 */
  detail?: string;
}

const DEFAULT_DURATION = 3200;

const store = writable<ToastItem[]>([]);

/** 只读视图（ToastHost 渲染用）。 */
export const toasts: Readable<ToastItem[]> = { subscribe: store.subscribe };

let nextId = 0;
const timers = new Map<number, ReturnType<typeof setTimeout>>();

export function dismiss(id: number): void {
  timers.delete(id);
  store.update((all) => all.filter((t) => t.id !== id));
}

/** 弹出全局 Toast；duration <= 0 表示常驻（需手动 dismiss）。detail 为可选第二行小字。 */
export function show(
  message: string,
  type: ToastType = 'info',
  duration = DEFAULT_DURATION,
  detail?: string
): number {
  const id = ++nextId;
  store.update((all) => [...all, { id, message, type, detail }]);
  if (duration > 0) {
    timers.set(
      id,
      setTimeout(() => dismiss(id), duration)
    );
  }
  return id;
}
