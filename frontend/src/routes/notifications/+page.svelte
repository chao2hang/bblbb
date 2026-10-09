<script lang="ts">
  import { onMount } from 'svelte';
  import {
    listNotifications,
    markAllNotificationsRead,
    markNotificationRead,
    type Notification,
  } from '$lib/api/client';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { formatRelative, formatDate } from '$lib/utils';
  import PageTitle from '$lib/components/PageTitle.svelte';
  import { setBellUnread, decrementBellUnread } from '$lib/notifications/bellState.svelte';

  let items = $state<Notification[]>([]);
  let unreadCount = $state(0);
  let loading = $state(true);
  let tab = $state('all');
  let actionError = $state<string | null>(null);
  /** 展开的详情通知 id（点行切换展开/收起）。 */
  let expandedId = $state<string | null>(null);

  const tabs = [
    { key: 'all', label: '全部' },
    { key: 'unread', label: '未读' },
    { key: 'reply', label: '回复' },
    { key: 'mention', label: '提及' },
    { key: 'reaction', label: '点赞' },
    { key: 'system', label: '系统' }
  ];

  /** 类别中文名（详情面板展示）。 */
  const categoryLabels: Record<string, string> = {
    activity: '互动',
    reply: '回复',
    mention: '提及',
    reaction: '点赞',
    moderation: '审核',
    system: '系统',
    security: '安全',
    digest: '摘要'
  };

  function categoryLabel(item: Notification): string {
    const cat = item.category ?? 'system';
    return categoryLabels[cat] ?? cat;
  }

  /** 类型图标（对齐原型）：提及@、回复、点赞、系统铃铛。 */
  function typeIcon(item: Notification): string {
    // mention 通知的遗留 type='mention'（category=activity），优先按 type 识别；
    // 失效资源（unavailable）投影不带 type，回退 category/默认图标。
    if (item.type === 'mention') return 'at-sign';
    if (item.category === 'reply') return 'message-square';
    if (item.category === 'reaction') return 'heart';
    return 'bell';
  }

  async function load() {
    loading = true;
    actionError = null;
    try {
      const isUnread = tab === 'unread';
      const category = tab !== 'all' && tab !== 'unread' ? tab : null;
      const result = await listNotifications(fetch, isUnread, category);
      items = result.items;
      unreadCount = result.unread_count;
      // unread_count 为服务端全局值（与当前 tab 过滤无关）：同步铃铛角标，
      // 保证停留在本页时 navbar 徽标也是权威值。
      setBellUnread(result.unread_count);
    } catch {
      items = [];
    }
    loading = false;
  }

  async function onRead(item: Notification) {
    if (item.is_read) return;
    try {
      await markNotificationRead(fetch, item.id);
      item.is_read = true;
      item.read_at = Date.now();
      if (unreadCount > 0) unreadCount -= 1;
      // 同步铃铛角标：停留在本页不触发路由变化，layout 不会自动重取。
      decrementBellUnread(1);
    } catch {
      actionError = '标记已读失败，请稍后重试';
    }
  }

  /** 点击行：展开/收起详情；展开的同时自动标已读（幂等，重复点不报错）。 */
  function toggle(item: Notification) {
    expandedId = expandedId === item.id ? null : item.id;
    if (expandedId === item.id) void onRead(item);
  }

  async function onReadAll() {
    try {
      const result = await markAllNotificationsRead(fetch);
      items.forEach((i) => { i.is_read = true; });
      unreadCount = Math.max(0, unreadCount - result.updated);
      // 同步铃铛角标（同 onRead：无路由变化，layout 不会自动重取）。
      setBellUnread(unreadCount);
    } catch {
      actionError = '批量已读失败，请稍后重试';
    }
  }

  onMount(() => { load(); });
</script>

  <PageTitle title="通知中心" />

<div class="container page-content" id="page-notifications">
  <h1 class="u-visually-hidden">通知中心</h1>

  {#if actionError}
    <p class="form-error" role="alert" data-testid="notify-action-error">{actionError}</p>
  {/if}

  <div class="card notification-panel">
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
        <ul class="notif-list" role="list">
          {#each items as item (item.id)}
            {@const expanded = expandedId === item.id}
            <li class="notif-item {!item.is_read ? 'is-unread' : ''}">
              <button
                type="button"
                class="notif-row"
                aria-expanded={expanded ? 'true' : 'false'}
                onclick={() => toggle(item)}
                data-testid={`notif-row-${item.id}`}
              >
                <span
                  class="notif-icon-box"
                  class:is-unread={!item.is_read}
                  aria-hidden="true"
                >
                  <Icon name={typeIcon(item)} size={18} />
                </span>
                <span class="notif-main">
                  <span class="notif-title">
                    {#if !item.is_read}
                      <span class="nav-dot notif-unread-dot" aria-label="未读"></span>
                    {/if}
                    {item.title}
                  </span>
                  {#if item.body && !expanded}
                    <span class="notif-body-preview">{item.body}</span>
                  {/if}
                </span>
                <span class="notif-side">
                  <span class="notif-time">{formatRelative(item.created_at)}</span>
                  <span class="notif-chevron" class:is-open={expanded} aria-hidden="true">
                    <Icon name="chevron-down" size={16} />
                  </span>
                </span>
              </button>
              {#if expanded}
                <div class="notif-detail" data-testid={`notif-detail-${item.id}`}>
                  <div class="notif-detail-meta">
                    <span class="badge badge-secondary">{categoryLabel(item)}</span>
                    <span class="notif-detail-time" title={String(item.created_at)}>
                      {formatDate(item.created_at)}
                    </span>
                    <span class="notif-read-state {item.is_read ? 'is-read' : 'is-unread'}">
                      {item.is_read ? '已读' : '未读'}
                    </span>
                  </div>
                  {#if item.body}
                    <p class="notif-detail-body">{item.body}</p>
                  {/if}
                  <div class="notif-detail-actions">
                    {#if item.link && !item.unavailable}
                      <a href={item.link} class="btn btn-primary btn-sm">
                        <Icon name="link" size={14} />
                        查看来源
                      </a>
                    {/if}
                    {#if !item.is_read && !item.unavailable}
                      <button
                        type="button"
                        class="btn btn-secondary btn-sm"
                        onclick={() => onRead(item)}
                        data-testid={`read-${item.id}`}
                      >标为已读</button>
                    {/if}
                  </div>
                </div>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  </div>

  <!-- 通知偏好已迁移至 /settings#settings-notifications -->
  <p class="input-hint" style="margin-top:var(--space-4);text-align:center;">
    如需调整通知接收渠道，请前往<a href="/settings#settings-notifications" class="text-link">账号设置 → 通知设置</a>
  </p>
</div>

<style>
  /* 通知列表：行式布局（图标 | 标题+预览 | 时间+展开箭头），点击整行展开详情 */
  .notif-list {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .notif-item + .notif-item {
    border-top: 1px solid var(--color-border);
  }
  .notif-row {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    width: 100%;
    padding: var(--space-3) var(--space-4);
    background: transparent;
    border: none;
    text-align: left;
    cursor: pointer;
    color: inherit;
    font: inherit;
    transition: background-color var(--transition-fast, 0.15s) ease;
  }
  .notif-row:hover {
    background: var(--color-surface-hover);
  }
  .notif-row:focus-visible {
    outline: 2px solid var(--color-brand);
    outline-offset: -2px;
  }
  .notif-icon-box {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    border-radius: var(--radius-md);
    background: var(--color-bg-subtle);
    color: var(--color-text-secondary);
    flex-shrink: 0;
  }
  .notif-icon-box.is-unread {
    background: color-mix(in srgb, var(--color-brand) 12%, transparent);
    color: var(--color-brand);
  }
  .notif-main {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }
  .notif-title {
    display: flex;
    align-items: center;
    gap: 6px;
    font-weight: var(--weight-medium);
  }
  .notif-item.is-unread .notif-title {
    font-weight: var(--weight-semibold);
  }
  .notif-unread-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--color-brand);
    display: inline-block;
    flex-shrink: 0;
  }
  .notif-body-preview {
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
    overflow: hidden;
    display: -webkit-box;
    line-clamp: 1;
    -webkit-line-clamp: 1;
    -webkit-box-orient: vertical;
  }
  .notif-side {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    flex-shrink: 0;
    color: var(--color-text-tertiary, var(--color-text-secondary));
    padding-top: 2px;
  }
  .notif-time {
    font-size: var(--text-sm);
    white-space: nowrap;
  }
  .notif-chevron {
    display: inline-flex;
    transition: transform var(--transition-fast, 0.15s) ease;
  }
  .notif-chevron.is-open {
    transform: rotate(180deg);
  }

  /* 展开详情：缩进对齐标题列，浅底面板 */
  .notif-detail {
    padding: 0 var(--space-4) var(--space-4) calc(var(--space-4) + 36px + var(--space-3));
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    animation: notif-expand var(--transition-fast, 0.15s) ease;
  }
  @keyframes notif-expand {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
  .notif-detail-meta {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    flex-wrap: wrap;
  }
  .notif-detail-time {
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
  }
  .notif-read-state {
    font-size: var(--text-xs, 0.75rem);
    padding: 1px 8px;
    border-radius: 999px;
  }
  .notif-read-state.is-read {
    color: var(--color-success);
    background: var(--color-success-soft);
  }
  .notif-read-state.is-unread {
    color: var(--color-warning);
    background: var(--color-warning-soft);
  }
  .notif-detail-body {
    margin: 0;
    font-size: var(--text-sm);
    line-height: 1.7;
    color: var(--color-text-secondary);
    white-space: pre-wrap;
    word-break: break-word;
  }
  .notif-detail-actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }

  @media (max-width: 640px) {
    .notif-detail {
      padding-left: var(--space-4);
    }
  }
</style>
