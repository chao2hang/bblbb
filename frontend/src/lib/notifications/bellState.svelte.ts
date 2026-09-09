import type { Notification } from '$lib/api/client';

/**
 * Navbar 铃铛徽标共享态（Svelte 5 模块级 $state 单例）。
 *
 * 背景：徽标数原先保存在 +layout.svelte 的组件局部态（unreadOverride），只在
 * 「路由变化触发会话刷新」与「铃铛下拉『全部已读』」两条路径更新；/notifications
 * 页的「标为已读 / 全部已读」只改页面局部态，停留在该页时铃铛角标不减少，
 * 要等下一次切路由才纠正。现由本模块承载：
 *
 * - +layout.svelte 在 $derived 中经 getBellUnread / getBellRecent 读取（reactive
 *   跟踪跨模块生效），并在初始化（SSR 每次请求、客户端整页首帧）无条件播种，
 *   后续由会话刷新 / 登出 / 下拉「全部已读」维护；
 * - /notifications 页在已读操作成功后调 decrementBellUnread / setBellUnread，
 *   铃铛角标即时同步，不必等路由变化。
 *
 * 注意：模块级 $derived 不可导出（derived_invalid_export），故以 getter 函数
 * 暴露当前值；在响应式上下文中读取即建立跟踪。
 *
 * SSR 注意：模块状态在同一进程内跨请求共享，根布局每次 init 必须无条件重新
 * 播种（而不是仅在 null 时播种），否则上一请求的值会泄漏进下一请求的首帧。
 * 客户端 SPA 内 init 只在整页首帧执行一次，之后以上述两条写入路径为准。
 */

let unread = $state<number | null>(null);
let recent = $state<Notification[] | null>(null);

/** 当前未读数（null = 尚未播种；layout 渲染时回退服务端 data）。 */
export function getBellUnread(): number | null {
  return unread;
}

/** 当前最近几条速览（铃铛下拉；null = 尚未播种）。 */
export function getBellRecent(): Notification[] | null {
  return recent;
}

/** 全量同步（服务端权威值）：未读数 + 速览。 */
export function syncBell(unreadCount: number, recentItems: Notification[]) {
  unread = unreadCount;
  recent = recentItems;
}

/** 仅覆盖未读数（通知页已读操作不改变速览）。 */
export function setBellUnread(unreadCount: number) {
  unread = unreadCount;
}

/** 仅覆盖速览（下拉「全部已读」把速览全部置为已读）。 */
export function setBellRecent(items: Notification[]) {
  recent = items;
}

/** 减少未读数（单条标为已读；下界 0 防负数）。 */
export function decrementBellUnread(delta = 1) {
  unread = Math.max(0, (unread ?? 0) - delta);
}
