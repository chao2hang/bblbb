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

<Navbar user={loading ? null : user} unread={unread} onlogout={handleLogout} />

<div class="page-wrapper">
  <main>
    {@render children()}
  </main>
</div>
