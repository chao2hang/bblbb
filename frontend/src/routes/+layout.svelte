<script lang="ts">
  import '../app.css';
  import { onMount } from 'svelte';
  import { getMe, logout, type User } from '$lib/api/client';
  import { goto } from '$app/navigation';
  import Navbar from '$lib/components/Navbar.svelte';

  let { children } = $props();

  let user = $state<User | null>(null);
  let loading = $state(true);
  let unread = $state(0);

  onMount(async () => {
    user = await getMe(fetch);
    loading = false;
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
