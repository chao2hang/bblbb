<script lang="ts">
  // M03-UI-07：管理后台布局——板块/标签/角色/Assignment 导航。
  // 所有数据与授权裁决均来自后端 API；401 → 跳登录、403 → 无权限态。
  import type { Snippet } from 'svelte';
  import { page } from '$app/state';

  let { children }: { children?: Snippet } = $props();

  const path = $derived(page.url.pathname);

  const navItems = [
    { label: '板块', href: '/admin/boards' },
    { label: '标签', href: '/admin/tags' },
    { label: '角色', href: '/admin/roles' },
    { label: 'Assignment', href: '/admin/assignments' },
    { label: '商城', href: '/admin/shop' },
    { label: '存储', href: '/admin/storage' },
    { label: '活跃', href: '/admin/activity' },
    { label: 'AI', href: '/admin/ai' },
    { label: 'Video', href: '/admin/video' }
  ];
</script>

<svelte:head>
  <title>管理后台 — BBLBB</title>
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">管理后台</span>
  </nav>

  <nav class="admin-nav" aria-label="管理后台导航" style="display:flex;gap:var(--space-2);margin-bottom:var(--space-5);flex-wrap:wrap;">
    {#each navItems as item}
      <a
        href={item.href}
        class="btn {path === item.href ? 'btn-primary' : 'btn-secondary'} btn-sm"
        aria-current={path === item.href ? 'page' : undefined}
      >{item.label}</a>
    {/each}
  </nav>

  {@render children?.()}
</div>
