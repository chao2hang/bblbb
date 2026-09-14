<script lang="ts">
  import '../app.css';
  import { onMount, untrack } from 'svelte';
  import { page } from '$app/state';
  import {
    getMe,
    logout,
    listNotifications,
    markAllNotificationsRead,
    type User
  } from '$lib/api/client';
  import { getBellUnread, getBellRecent, syncBell, setBellUnread, setBellRecent } from '$lib/notifications/bellState.svelte';
  import { goto } from '$app/navigation';
  import Navbar from '$lib/components/Navbar.svelte';
  import BottomNav from '$lib/components/BottomNav.svelte';
  import ToastHost from '$lib/components/ui/ToastHost.svelte';
  import NoJsNotice from '$lib/components/ui/NoJsNotice.svelte';
  import { show as showToast } from '$lib/ui/toast';
  import { applyThemeTokens, clearThemeTokens, resolveLayoutMode, type ActiveThemeView } from '$lib/theme/projection';
  import { registerAdminElements } from '@chaos_team/blbui-core/register';
  import type { LayoutData } from './$types';
  import type { Snippet } from 'svelte';

  onMount(() => {
    registerAdminElements();
  });

  // data 在 SvelteKit 运行时恒有（LayoutData）；测试隔离渲染时可缺省。
  let { data, children }: { data?: LayoutData; children: Snippet } = $props();

  // 首帧会话态来自 +layout.server.ts 的服务端 /me（登录用户 SSR 即渲染登录态
  // navbar，无 JS 基线一致）；匿名/后端不可达为 null。
  let user = $state<User | null>(untrack(() => data?.user ?? null));

  // 通知徽标：共享铃铛态（bellState.svelte）在此渲染。SSR/整页加载由
  // +layout.server.ts 提供（登录用户 limit=3 + unread_count），每次 init 无条件
  // 播种（模块态跨请求共享，必须重新播种防泄漏；客户端 SPA 内 init 仅整页
  // 首帧执行一次），服务端即渲染真实徽标（无 JS 也可见）；客户端导航后由
  // 会话刷新 / 登出 / 通知页已读操作两条写入路径维护，铃铛角标不依赖路由变化。
  syncBell(
    untrack(() => data?.notifications?.unreadCount ?? 0),
    untrack(() => data?.notifications?.recent ?? [])
  );

  const unread = $derived(getBellUnread() ?? data?.notifications?.unreadCount ?? 0);
  const recentNotifications = $derived(getBellRecent() ?? data?.notifications?.recent ?? []);
  const activeTheme = $derived<ActiveThemeView | null>(data?.activeTheme ?? null);

  // 全站文案（0065）：站点名/描述来自后台系统设置，后端不可达时为内置兜底。
  const siteName = $derived(data?.site?.siteName ?? 'BBLBB');
  const siteDescription = $derived(data?.site?.siteDescription ?? '');

  // 主题声明的页面结构预设（layout.mode → classic/sidebar/wide）。
  // SSR 首帧即写入 .app-shell[data-theme-layout]，结构无闪烁；
  // 浏览器端 applyThemeTokens/previewThemeTokens 在预览与切换时同步该属性。
  const shellLayout = $derived(resolveLayoutMode(activeTheme?.tokens ?? null));
  const isAdmin = $derived(page.url.pathname.startsWith('/admin'));
  const isAuthStandalone = $derived(page.url.pathname === '/login');

  // 全站生效主题 Token 动态应用
  $effect(() => {
    if (typeof document !== 'undefined') {
      if (document.documentElement.dataset.themePreview === 'true') {
        return;
      }
      if (activeTheme && activeTheme.name !== 'default') {
        applyThemeTokens(activeTheme);
      } else {
        clearThemeTokens();
      }
    }
  });

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
          syncBell(result.unread_count ?? 0, result.items.slice(0, 3));
        } catch {
          /* 保留现有徽标值 */
        }
      } else {
        syncBell(0, []);
      }
    } catch {
      user = null;
      syncBell(0, []);
    }
  }

  async function handleLogout() {
    try {
      await logout(fetch);
    } finally {
      user = null;
      syncBell(0, []);
      goto('/');
    }
  }

  // 铃铛下拉「全部已读」：调 read-all → 更新徽标/速览态 + toast 反馈。
  async function handleMarkAllRead() {
    try {
      const result = await markAllNotificationsRead(fetch);
      const current = getBellUnread() ?? 0;
      setBellUnread(Math.max(0, current - (result?.updated ?? current)));
      setBellRecent((getBellRecent() ?? []).map((n) => ({ ...n, is_read: true })));
      showToast('已将全部通知标记为已读', 'success');
    } catch {
      showToast('操作失败，请稍后重试', 'danger');
    }
  }
</script>

<svelte:head>
  <meta name="description" content={siteDescription} />
</svelte:head>

<a class="skip-link" href="#main-content">跳转到主要内容</a>

<NoJsNotice />

<!-- .app-shell：主题结构预设作用域（data-theme-layout 由服务端数据解析，
     classic 为缺省；预览/切换由 projection 写入同一属性保持一致） -->
<div class="app-shell" class:app-shell--admin={isAdmin} class:app-shell--auth={isAuthStandalone} data-theme-layout={shellLayout}>
  {#if isAdmin}
    <main id="main-content" tabindex="-1" class="admin-viewport-main">
      {@render children()}
    </main>
  {:else if isAuthStandalone}
    <main id="main-content" tabindex="-1" class="auth-viewport-main">
      {@render children()}
    </main>
  {:else}
    <div class="aui-root">
      <Navbar
        user={user}
        siteName={siteName}
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
      <BottomNav user={user} {unread} />
    </div>
  {/if}
</div>
<ToastHost />
