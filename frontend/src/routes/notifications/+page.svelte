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
  import Icon from '$lib/components/ui/Icon.svelte';
  import { formatRelative } from '$lib/utils';
  import PageTitle from '$lib/components/PageTitle.svelte';

  let items = $state<Notification[]>([]);
  let unreadCount = $state(0);
  let loading = $state(true);
  let tab = $state('all');
  let prefs = $state<NotificationPreference[]>([]);
  let prefsError = $state<string | null>(null);
  let actionError = $state<string | null>(null);

  const tabs = [
    { key: 'all', label: '全部' },
    { key: 'unread', label: '未读' },
    { key: 'reply', label: '回复' },
    { key: 'reaction', label: '点赞' },
    { key: 'system', label: '系统' }
  ];

  /** 类型图标（对齐原型）：回复、点赞、系统铃铛。 */
  function typeIcon(cat: string | null | undefined): string {
    if (cat === 'reply') return 'message-square';
    if (cat === 'reaction') return 'heart';
    return 'bell';
  }

  const categoryLabels: Record<string, string> = {
    reply: '回复',
    reaction: '点赞',
    activity: '互动',
    moderation: '审核',
    system: '系统',
    security: '安全',
    digest: '摘要'
  };

  /** 类别说明（偏好矩阵行内第二行）：先看懂这一类是什么，再决定收到哪个渠道。 */
  const categoryDescriptions: Record<string, string> = {
    activity: '他人的点赞、回复和提及',
    moderation: '内容被审核处理与申诉结果',
    system: '账号与站点运营相关提醒',
    security: '登录与账号安全提醒',
    digest: '周期性的动态摘要'
  };

  async function load() {
    loading = true;
    actionError = null;
    try {
      const isUnread = tab === 'unread';
      const category = tab !== 'all' && tab !== 'unread' ? tab : null;
      const result = await listNotifications(fetch, isUnread, category);
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

  <PageTitle title="通知中心" />

<div class="container page-content">
  <!-- 原型对齐（prototype/pages/notifications.html）：app-route-head，无面包屑。 -->
  <div class="app-route-head">
    <div class="app-route-head__copy">
      <span class="app-kicker">INBOX / NOTIFICATIONS</span>
      <h1 tabindex="-1">通知中心</h1>
      <p>最近 30 天的互动、关注和系统提醒</p>
    </div>
  </div>

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
              <!-- 原型同款类型图标卡（圆角底） -->
              <div
                class="notify-icon-box"
                style="width:36px;height:36px;border-radius:var(--radius-md);background:var(--color-bg-subtle, rgba(0,0,0,0.04));display:flex;align-items:center;justify-content:center;color:var(--color-brand);flex-shrink:0;"
                aria-hidden="true"
              >
                <Icon name={typeIcon(item.category)} size={18} />
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

  <!-- 通知偏好：类别 × 渠道矩阵。桌面三列对齐（列头承载渠道名），
       移动端隐藏列头、渠道标签随行内显示。 -->
  <div class="card" style="margin-top:var(--space-5);">
    <div class="card-header">
      <div class="np-head-copy">
        <span class="card-title">通知偏好</span>
        <span class="np-subtitle">选择每类通知的接收渠道</span>
      </div>
    </div>
    <div class="np-body">
      {#if prefsError}<p class="form-error" role="alert">{prefsError}</p>{/if}
      <div class="np-matrix">
        <div class="np-row np-row-head" aria-hidden="true">
          <span class="np-cat-head">类别</span>
          <div class="np-channels">
            <span class="np-cell np-cell-head">邮件</span>
            <span class="np-cell np-cell-head">站内</span>
            <span class="np-cell np-cell-head">推送</span>
          </div>
        </div>
        {#each prefs as p}
          {@const label = categoryLabels[p.category] ?? p.category}
          <div class="np-row" role="group" aria-label={`${label} 通知偏好`}>
            <div class="np-cat">
              <span class="np-cat-name">{label}</span>
              <span class="np-cat-desc">{categoryDescriptions[p.category] ?? ''}</span>
            </div>
            <div class="np-channels">
              <label class="np-cell">
                <input class="np-check" type="checkbox" checked={p.email_enabled} onchange={() => togglePref(p, 'email_enabled')} />
                <span class="np-cell-label">邮件</span>
              </label>
              <label class="np-cell">
                <input class="np-check" type="checkbox" checked={p.in_app_enabled} onchange={() => togglePref(p, 'in_app_enabled')} />
                <span class="np-cell-label">站内</span>
              </label>
              <label class="np-cell">
                <input class="np-check" type="checkbox" checked={p.push_enabled} onchange={() => togglePref(p, 'push_enabled')} />
                <span class="np-cell-label">推送</span>
              </label>
            </div>
          </div>
        {/each}
      </div>
    </div>
    <div class="np-foot">
      <Icon name="shield" size={13} />
      <span>安全通知至少保留一个接收渠道</span>
    </div>
  </div>
</div>

<style>
  /* ---- 通知偏好矩阵 ---- */
  .np-head-copy {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .np-subtitle {
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
  }

  .np-body {
    padding: 0;
  }

  .np-body .form-error {
    margin: var(--space-4) var(--space-5) 0;
  }

  .np-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) repeat(3, 64px);
    align-items: center;
    gap: var(--space-3);
    /* 负边距让 hover/分隔线通到卡片边缘（与列表行同款手法） */
    margin-inline: calc(-1 * var(--space-5));
    padding: var(--space-3) var(--space-5);
    transition: background-color var(--duration-fast) var(--ease-out);
  }

  .np-row-head {
    padding-block: var(--space-2);
  }

  /* 行分隔用发丝线（0.5px，chinese-elegance 同款 token） */
  .np-row:not(.np-row-head) {
    border-top: var(--border-thin);
  }

  .np-row:not(.np-row-head):hover,
  .np-row:not(.np-row-head):focus-within {
    background: var(--color-bg-subtle);
  }

  /* 渠道列在行内“散开”参与栅格，与列头同宽对齐 */
  .np-channels {
    display: contents;
  }

  .np-cell {
    position: relative;
    display: grid;
    place-items: center;
  }

  .np-cat-head,
  .np-cell-head {
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    color: var(--color-text-tertiary);
    letter-spacing: 0.04em;
  }

  .np-cell-head {
    text-align: center;
  }

  .np-cat {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .np-cat-name {
    font-size: var(--text-base);
    font-weight: var(--weight-medium);
    color: var(--color-text-primary);
  }

  .np-cat-desc {
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
  }

  .np-check {
    width: 16px;
    height: 16px;
    margin: 0;
    accent-color: var(--color-brand);
    cursor: pointer;
  }

  .np-check:focus-visible {
    outline: 2px solid var(--color-focus-ring);
    outline-offset: 2px;
  }

  /* 渠道文字标签：桌面隐藏（列头已承载），移动端随行内显示 */
  .np-cell-label {
    position: absolute;
    width: 1px;
    height: 1px;
    margin: -1px;
    padding: 0;
    border: 0;
    overflow: hidden;
    clip: rect(0 0 0 0);
    clip-path: inset(50%);
    white-space: nowrap;
  }

  .np-foot {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-5);
    border-top: var(--border-default);
    font-size: var(--text-xs);
    color: var(--color-text-tertiary);
  }

  @media (max-width: 640px) {
    .np-row-head {
      display: none;
    }

    .np-row:not(.np-row-head) {
      grid-template-columns: 1fr;
      align-items: start;
      padding-block: var(--space-4);
    }

    .np-channels {
      display: flex;
      justify-content: flex-end;
      gap: var(--space-5);
    }

    .np-cell {
      display: inline-flex;
      align-items: center;
      gap: 6px;
    }

    .np-cell-label {
      position: static;
      width: auto;
      height: auto;
      margin: 0;
      overflow: visible;
      clip: auto;
      clip-path: none;
      white-space: normal;
      font-size: var(--text-sm);
      color: var(--color-text-secondary);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .np-row {
      transition: none;
    }
  }
</style>
