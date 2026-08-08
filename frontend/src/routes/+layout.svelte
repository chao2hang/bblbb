<script lang="ts">
  import '../app.css';
  import { page } from '$app/state';
  import { getMe, logout, type User } from '$lib/api/client';
  import { goto } from '$app/navigation';
  import Navbar from '$lib/components/Navbar.svelte';
  import NoJsNotice from '$lib/components/ui/NoJsNotice.svelte';

  let { children } = $props();

  let user = $state<User | null>(null);
  let loading = $state(true);
  let unread = $state(0);

  // M14-A11Y/ROUTES 修复：会话态在客户端导航后保持同步。
  //
  // 原实现用 onMount 拉取 /me —— onMount 只在整页加载时执行一次，登录 action
  // 经 use:enhance 走 SPA 跳转时 layout 不会重挂载，navbar 会一直停留在未登录态。
  // 改为按路径变化重取 /me（轻量 GET，无 CSRF），登录/退出后导航即反映真实会话。
  let lastPath = '';
  $effect(() => {
    const path = page.url.pathname;
    if (path === lastPath) return;
    lastPath = path;
    loading = true;
    getMe(fetch)
      .then((me) => {
        user = me;
        loading = false;
      })
      .catch(() => {
        user = null;
        loading = false;
      });
  });

  async function handleLogout() {
    try {
      await logout(fetch);
    } finally {
      user = null;
      goto('/');
    }
  }
</script>

<svelte:head>
  <meta name="theme-color" content="#F5F3ED" />
  <meta name="description" content="BBLBB 社区论坛" />
</svelte:head>

<a class="skip-link" href="#main-content">跳转到主要内容</a>

<NoJsNotice />

<Navbar user={loading ? null : user} unread={unread} onlogout={handleLogout} />

<div class="page-wrapper">
  <main id="main-content" tabindex="-1">
    {@render children()}
  </main>
  <footer class="site-footer">
    <div class="container site-footer-inner">
      <span>BBLBB 社区论坛</span>
      <span class="text-secondary">· 自由讨论、友善交流</span>
    </div>
  </footer>
</div>
