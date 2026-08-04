<script lang="ts">
  import { onMount } from 'svelte';
  import { listNotifications, type Notification } from '$lib/api/client';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import { formatRelative } from '$lib/utils';

  let items = $state<Notification[]>([]);
  let unreadCount = $state(0);
  let loading = $state(true);
  let tab = $state('all');

  const tabs = [
    { key: 'all', label: '全部' },
    { key: 'unread', label: '未读' }
  ];

  async function load() {
    loading = true;
    try {
      const result = await listNotifications(fetch, tab === 'unread');
      items = result.items;
      unreadCount = result.unread_count;
    } catch {
      items = [];
    }
    loading = false;
  }

  onMount(load);
</script>

<svelte:head>
  <title>通知 — BBLBB</title>
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">通知</span>
  </nav>

  <div class="card">
    <div class="card-header" style="display:flex;align-items:center;justify-content:space-between;">
      <span class="card-title">通知</span>
      {#if unreadCount > 0}<span class="badge badge-warning">{unreadCount} 未读</span>{/if}
    </div>
    <div class="tabs" role="tablist">
      {#each tabs as t}
        <button
          type="button"
          role="tab"
          aria-selected={tab === t.key ? 'true' : 'false'}
          class="tab {tab === t.key ? 'is-active' : ''}"
          onclick={() => { tab = t.key; load(); }}
        >{t.label}</button>
      {/each}
    </div>
    <div class="card-body" style="padding:0;">
      {#if loading}
        <div class="empty-state"><div class="empty-state-title">加载中…</div></div>
      {:else if items.length === 0}
        <EmptyState icon="bell" title="暂无通知" desc="有新动态时会在这里提醒你" />
      {:else}
        <div style="display:flex;flex-direction:column;">
          {#each items as item}
            <a
              href={item.link ?? undefined}
              class="post-row"
              style="padding:var(--space-4);border-bottom:var(--border-default);display:flex;gap:var(--space-3);align-items:flex-start;text-decoration:none;"
            >
              <div style="min-width:0;flex:1;">
                <div style="font-weight:var(--weight-medium);">{item.title}</div>
                {#if item.body}<div class="text-secondary" style="font-size:var(--text-sm);margin-top:2px;">{item.body}</div>{/if}
              </div>
              <span class="text-tertiary" style="font-size:var(--text-xs);white-space:nowrap;">
                {formatRelative(item.created_at)}
                {#if !item.is_read}<span class="badge badge-warning" style="margin-left:var(--space-1);">新</span>{/if}
              </span>
            </a>
          {/each}
        </div>
      {/if}
    </div>
  </div>
</div>
