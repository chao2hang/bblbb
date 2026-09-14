import { browser } from '$app/environment';

/**
 * 帖子已读状态共享态（Svelte 5 模块级 $state 单例；纯客户端本地数据）。
 *
 * 用途：首页话题列表左侧 2px 色条以「未读亮 / 已读暗」区分浏览状态
 * （与标题 :visited 变灰的语义一致）。
 *
 * - 已读集合持久在 localStorage（key: bblbb-read-posts，命名跟随 bblbb-theme）；
 *   只保留最近 MAX_TRACKED 条 id，防止无限膨胀；
 * - SSR 不触碰 storage：模块水合仅浏览器执行，isPostRead 恒 false（=未读），
 *   客户端水合后由响应式派生自动纠正（模块状态服务端不写入，无跨请求泄漏）；
 * - markPostRead 在帖子详情页挂载时调用（幂等）；localStorage 不可用
 *   （隐私模式等）时静默降级为会话内状态。
 *
 * 响应式注意：水合在模块加载时一次性完成（不在派生上下文内写状态，
 * 规避 state_unsafe_mutation）；写入只发生在 markPostRead（事件/挂载路径）。
 */

const STORAGE_KEY = 'bblbb-read-posts';
const MAX_TRACKED = 500;

let readIds = $state<ReadonlySet<string>>(new Set());

if (browser) {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const parsed: unknown = JSON.parse(raw);
      if (Array.isArray(parsed)) {
        readIds = new Set(
          parsed
            .filter((v): v is string => typeof v === 'string' && v.length > 0)
            .slice(-MAX_TRACKED)
        );
      }
    }
  } catch {
    /* 存储不可用/内容损坏 → 视为全未读，不影响渲染 */
  }
}

/** 该帖是否已读（响应式读取；SSR/未水合时恒 false=未读）。 */
export function isPostRead(id: string): boolean {
  return readIds.has(id);
}

/** 标记帖子已读（详情页挂载时调用；幂等）。 */
export function markPostRead(id: string): void {
  if (!browser || !id) return;
  if (readIds.has(id)) return;
  const list = [...readIds, id].slice(-MAX_TRACKED);
  readIds = new Set(list);
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(list));
  } catch {
    /* 写入失败（隐私模式等）→ 仅保留会话内状态 */
  }
}
