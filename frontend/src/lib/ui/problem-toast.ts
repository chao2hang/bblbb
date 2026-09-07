// 瞬态服务端错误 → 全局 Toast 提示（产品约定：5xx/429 的页面加载失败
// 用消息提示，不整页展示错误态；页面只保留「加载失败 + 重试」占位）。
//
// 用法（页面 $effect 中，依赖 problem 对象身份）：
//   $effect(() => { void data.problem; announceTransientProblem(data.problem); });
//
//  - 仅客户端生效（SSR/无 JS 不弹 Toast，占位卡仍是无 JS 基线）；
//  - 同一 problem 实例只弹一次（WeakSet 去重，防 $effect 重跑/水合重复）；
//  - 文案走 problemText（稳定中文映射 + request_id 便于排查）。
import { isTransientProblem, problemText, type Problem } from '$lib/errors';
import { show } from './toast';

const announced = new WeakSet<Problem>();

/** 对瞬态服务端错误弹一条 danger Toast；非瞬态/空值直接忽略。 */
export function announceTransientProblem(problem: Problem | null | undefined): void {
  if (typeof window === 'undefined') return;
  if (!isTransientProblem(problem)) return;
  if (!problem || announced.has(problem)) return;
  announced.add(problem);
  show(problemText(problem), 'danger');
}
