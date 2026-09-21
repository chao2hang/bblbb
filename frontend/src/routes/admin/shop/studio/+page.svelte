<script lang="ts">
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';
  onMount(() => {
    goto('/admin/shop', { replaceState: true });
  });
</script>

<div class="studio-redirect p-6 text-center">
  <h2>装扮工作台已整合至商城管理</h2>
  <p class="mt-2 text-muted">正在跳转至商城管理后台...</p>
  <div class="mt-4">
    <a href="/admin/shop" class="btn btn-primary">进入商城管理</a>
  </div>
</div>
