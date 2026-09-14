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
  import ListRow from '$lib/components/ui/ListRow.svelte';
  import Meta from '$lib/components/ui/Meta.svelte';
  import Panel from '$lib/components/ui/Panel.svelte';
  import { formatRelative } from '$lib/utils';
  import PageTitle from '$lib/components/PageTitle.svelte';
  import { setBellUnread, decrementBellUnread } from '$lib/notifications/bellState.svelte';

  let items = $state<Notification[]>([]);
  let unreadCount = $state(0);
  let loading = $state(true);
  let tab = $state('all');
  let actionError = $state<string | null>(null);

  const tabs = [
    { key: 'all', label: '全部' },
    { key: 'unread', label: '未读' },
    { key: 'reply', label: '回复' },
    { key: 'mention', label: '提及' },
    { key: 'reaction', label: '点赞' },
    { key: 'system', label: '系统' }
  ];

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
      if (unreadCount > 0) unreadCount -= 1;
      // 同步铃铛角标：停留在本页不触发路由变化，layout 不会自动重取。
      decrementBellUnread(1);
    } catch {
      actionError = '标记已读失败，请稍后重试';
    }
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
        <div style="display:flex;flex-direction:column;">
          {#each items as item}
            <ListRow class={!item.is_read ? 'post-row notify-unread' : 'post-row'}>
              <!-- 原型同款类型图标卡（圆角底） -->
              <div
                class="notify-icon-box"
                style="width:36px;height:36px;border-radius:var(--radius-md);background:var(--color-bg-subtle, rgba(0,0,0,0.04));display:flex;align-items:center;justify-content:center;color:var(--color-brand);flex-shrink:0;"
                aria-hidden="true"
              >
                <Icon name={typeIcon(item)} size={18} />
              </div>
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
                  <div style="font-weight:var(--weight-medium);display:flex;align-items:center;gap:6px;">
                    {#if !item.is_read}
                      <span class="nav-dot" style="width:6px;height:6px;border-radius:50%;background:var(--color-brand);display:inline-block;flex-shrink:0;" aria-label="未读"></span>
                    {/if}
                    {item.title}
                  </div>
                  {#if item.body}<div class="text-secondary" style="font-size:var(--text-sm);margin-top:2px;">{item.body}</div>{/if}
                </a>
              {/if}
              <!-- 右侧操作区：「标为已读」与时间同行顶对齐（对齐标题行/图标中心），
                   避免按钮悬在时间上方造成的错位。 -->
              <div style="display:flex;align-items:center;gap:var(--space-2);flex-shrink:0;padding-top:4px;">
                {#if !item.unavailable && !item.is_read}
                  <button
                    type="button"
                    class="btn btn-secondary btn-sm"
                    onclick={() => onRead(item)}
                    data-testid={`read-${item.id}`}
                  >标为已读</button>
                {/if}
                <Meta items={[formatRelative(item.created_at)]} />
              </div>
            </ListRow>
          {/each}
        </div>
      {/if}
    </div>
  </div>

  <!-- 通知偏好已迁移至 /settings#settings-notifications -->
  <p class="input-hint" style="margin-top:var(--space-4);text-align:center;">
    如需调整通知接收渠道，请前往<a href="/settings#settings-notifications" class="text-link">账号设置 → 通知设置</a>
  </p>
</div>
