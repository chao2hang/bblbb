<script lang="ts">
  import { onMount } from 'svelte';
  import {
    listNotifications,
    markAllNotificationsRead,
    markNotificationRead,
    getNotificationPreferences,
    setNotificationPreference,
    type Notification,
    type NotificationPreference
  } from '$lib/api/client';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import { formatRelative } from '$lib/utils';

  let items = $state<Notification[]>([]);
  let unreadCount = $state(0);
  let loading = $state(true);
  let tab = $state('all');
  let prefs = $state<NotificationPreference[]>([]);
  let prefsError = $state<string | null>(null);
  let actionError = $state<string | null>(null);

  const tabs = [
    { key: 'all', label: '全部' },
    { key: 'unread', label: '未读' }
  ];

  const categoryLabels: Record<string, string> = {
    activity: '互动',
    moderation: '审核',
    system: '系统',
    security: '安全',
    digest: '摘要'
  };

  async function load() {
    loading = true;
    actionError = null;
    try {
      const result = await listNotifications(fetch, tab === 'unread');
      items = result.items;
      unreadCount = result.unread_count;
    } catch {
      items = [];
    }
    loading = false;
  }

  async function loadPrefs() {
    try {
      const result = await getNotificationPreferences(fetch);
      prefs = result.items;
      prefsError = null;
    } catch {
      prefsError = '偏好加载失败';
    }
  }

  async function onRead(item: Notification) {
    if (item.is_read) return;
    try {
      await markNotificationRead(fetch, item.id);
      item.is_read = true;
      if (unreadCount > 0) unreadCount -= 1;
    } catch {
      actionError = '标记已读失败，请稍后重试';
    }
  }

  async function onReadAll() {
    try {
      const result = await markAllNotificationsRead(fetch);
      items.forEach((i) => { i.is_read = true; });
      unreadCount = Math.max(0, unreadCount - result.updated);
    } catch {
      actionError = '批量已读失败，请稍后重试';
    }
  }

  async function togglePref(p: NotificationPreference, key: 'email_enabled' | 'in_app_enabled' | 'push_enabled') {
    const next = { ...p, [key]: !p[key] };
    try {
      await setNotificationPreference(fetch, next);
      Object.assign(p, next);
      prefsError = null;
    } catch {
      prefsError = '偏好保存失败（安全通知不可完全关闭）';
    }
  }

  onMount(() => { load(); loadPrefs(); });
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

  {#if actionError}
    <p class="form-error" role="alert" data-testid="notify-action-error">{actionError}</p>
  {/if}

  <div class="card">
    <div class="card-header" style="display:flex;align-items:center;justify-content:space-between;gap:var(--space-3);">
      <span class="card-title">通知</span>
      <div style="display:flex;gap:var(--space-2);align-items:center;">
        {#if unreadCount > 0}<span class="badge badge-warning">{unreadCount} 未读</span>{/if}
        {#if unreadCount > 0}
          <button type="button" class="btn btn-secondary btn-sm" onclick={onReadAll} data-testid="read-all">全部已读</button>
        {/if}
      </div>
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
            <div
              class="post-row"
              class:notify-unread={!item.is_read}
              style="padding:var(--space-4);border-bottom:var(--border-default);display:flex;gap:var(--space-3);align-items:flex-start;"
            >
              {#if item.unavailable}
                <div style="min-width:0;flex:1;">
                  <div style="font-weight:var(--weight-medium);">{item.title}</div>
                  <div class="text-secondary" style="font-size:var(--text-sm);margin-top:2px;">{item.body}</div>
                </div>
                <span class="badge badge-warning">已失效</span>
              {:else}
                <a
                  href={item.link ?? undefined}
                  onclick={() => onRead(item)}
                  class="post-row-link"
                  style="min-width:0;flex:1;text-decoration:none;display:block;"
                >
                  <div style="font-weight:var(--weight-medium);">{item.title}</div>
                  {#if item.body}<div class="text-secondary" style="font-size:var(--text-sm);margin-top:2px;">{item.body}</div>{/if}
                </a>
                {#if !item.is_read}
                  <button
                    type="button"
                    class="btn btn-secondary btn-sm"
                    onclick={() => onRead(item)}
                    data-testid={`read-${item.id}`}
                  >标为已读</button>
                {/if}
              {/if}
              <span class="text-tertiary" style="font-size:var(--text-xs);white-space:nowrap;align-self:center;">
                {formatRelative(item.created_at)}
              </span>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>

  <div class="card" style="margin-top:var(--space-5);">
    <div class="card-header"><span class="card-title">通知偏好</span></div>
    <div class="card-body">
      {#if prefsError}<p class="form-error" role="alert">{prefsError}</p>{/if}
      <div style="display:flex;flex-direction:column;gap:var(--space-3);">
        {#each prefs as p}
          <div style="display:flex;align-items:center;justify-content:space-between;gap:var(--space-3);">
            <span>{categoryLabels[p.category] ?? p.category}</span>
            <div style="display:flex;gap:var(--space-3);" role="group" aria-label={`{categoryLabels[p.category] ?? p.category} 偏好`}>
              <label><input type="checkbox" checked={p.email_enabled} onchange={() => togglePref(p, 'email_enabled')} /> 邮件</label>
              <label><input type="checkbox" checked={p.in_app_enabled} onchange={() => togglePref(p, 'in_app_enabled')} /> 站内</label>
              <label><input type="checkbox" checked={p.push_enabled} onchange={() => togglePref(p, 'push_enabled')} /> 推送</label>
            </div>
          </div>
        {/each}
      </div>
    </div>
  </div>
</div>
