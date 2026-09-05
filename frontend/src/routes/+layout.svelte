<script lang="ts">
  import '../app.css';
  import { untrack } from 'svelte';
  import { page } from '$app/state';
  import {
    getMe,
    logout,
    listNotifications,
    markAllNotificationsRead,
    type User,
    type Notification
  } from '$lib/api/client';
  import { goto } from '$app/navigation';
  import Navbar from '$lib/components/Navbar.svelte';
  import BottomNav from '$lib/components/BottomNav.svelte';
  import ToastHost from '$lib/components/ui/ToastHost.svelte';
  import NoJsNotice from '$lib/components/ui/NoJsNotice.svelte';
  import { show as showToast } from '$lib/ui/toast';
  import type { LayoutData } from './$types';
  import type { Snippet } from 'svelte';

  // data 在 SvelteKit 运行时恒有（LayoutData）；测试隔离渲染时可缺省。
  let { data, children }: { data?: LayoutData; children: Snippet } = $props();

  // 首帧会话态来自 +layout.server.ts 的服务端 /me（登录用户 SSR 即渲染登录态
  // navbar，无 JS 基线一致）；匿名/后端不可达为 null。
  let user = $state<User | null>(untrack(() => data?.user ?? null));

  // 通知徽标：SSR/整页加载由 +layout.server.ts 提供（登录用户 limit=3 +
  // unread_count），服务端即渲染真实徽标（无 JS 也可见）；客户端导航后
  // 由下方会话刷新路径写入 override（null = 未覆盖，回退 server data）。
  let unreadOverride = $state<number | null>(null);
  let recentOverride = $state<Notification[] | null>(null);

  const unread = $derived(unreadOverride ?? data?.notifications?.unreadCount ?? 0);
  const recentNotifications = $derived(recentOverride ?? data?.notifications?.recent ?? []);

  // 会话态在客户端导航后保持同步（M14-A11Y/ROUTES 修复 + 导航防闪烁）。
  //
  // 原实现用 onMount 拉取 /me —— onMount 只在整页加载时执行一次，登录 action
  // 经 use:enhance 走 SPA 跳转时 layout 不会重挂载，navbar 会一直停留在未登录态。
  // 故按路径变化重取 /me（轻量 GET，无 CSRF），登录/退出后导航即反映真实会话；
  // 通知未读数与速览也放进同一刷新路径（getMe 成功后拉取，见全局壳需求）。
  //
  // 防闪烁（2026-09 修复「切页 navbar 一抽一抽」）：刷新期间**保留最后一次已知
  // user**（不再置 loading 回退未登录 UI），响应到达才更新——客户端导航过程
  // navbar 零状态跳变；登录/退出仍只发生一次有意的状态切换。
  let lastPath = '';
  $effect(() => {
    const path = page.url.pathname;
    if (path === lastPath) return;
    lastPath = path;
    refreshSession();
  });

  async function refreshSession() {
    try {
      const me = await getMe(fetch);
      user = me;
      if (me) {
        // 通知徽标刷新（best-effort：失败不阻塞会话态，保留 SSR 值）。
        try {
          const result = await listNotifications(fetch);
          unreadOverride = result.unread_count ?? 0;
          recentOverride = result.items.slice(0, 3);
        } catch {
          /* 保留现有徽标值 */
        }
      } else {
        unreadOverride = 0;
        recentOverride = [];
      }
    } catch {
      user = null;
      unreadOverride = 0;
      recentOverride = [];
    }
  }

  async function handleLogout() {
    try {
      await logout(fetch);
    } finally {
      user = null;
      unreadOverride = 0;
      recentOverride = [];
      goto('/');
    }
  }

  // 铃铛下拉「全部已读」：调 read-all → 更新徽标/速览态 + toast 反馈。
  async function handleMarkAllRead() {
    try {
      const result = await markAllNotificationsRead(fetch);
      unreadOverride = Math.max(0, unread - (result?.updated ?? unread));
      recentOverride = recentNotifications.map((n) => ({ ...n, is_read: true }));
      showToast('已将全部通知标记为已读', 'success');
    } catch {
      showToast('操作失败，请稍后重试', 'danger');
    }
  }
</script>

<svelte:head>
  <meta name="description" content="BBLBB 社区论坛" />
</svelte:head>

<a class="skip-link" href="#main-content">跳转到主要内容</a>

<NoJsNotice />

<Navbar
  user={user}
  {unread}
  notifications={recentNotifications}
  onlogout={handleLogout}
  onreadall={handleMarkAllRead}
/>

<div class="page-wrapper">
  <main id="main-content" tabindex="-1">
    {@render children()}
  </main>
</div>
<!-- 原型无站点页脚（prototype/assets/page-chrome.js 仅注入顶栏/底部导航/Toast），
     故不渲染 site-footer；移动端安全区余量由 .page-wrapper 自身的 padding 兜底。 -->

<!-- 全局壳：移动端底部导航（≤768px）+ 全局 Toast 容器 -->
<BottomNav user={user} />
<ToastHost />
