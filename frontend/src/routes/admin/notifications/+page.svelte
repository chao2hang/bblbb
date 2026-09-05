<script lang="ts">
  // M18-ADMIN-NOTIFICATIONS：通知与邮件管理页（对齐原型 #admin-notifications）。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { show as showToast } from '$lib/ui/toast';
  import type { AdminNotificationsPageData, AdminNotificationsActionData } from './+page.server';

  let { data, form }: { data: AdminNotificationsPageData; form?: AdminNotificationsActionData | null } = $props();

  const mockTemplates = [
    { id: 'verify_email', name: '验证邮件', trigger: '事件触发', queue: 1, failed: 2 },
    { id: 'digest', name: '通知摘要', trigger: '每日 08:00', queue: 0, failed: 0 },
    { id: 'security_alert', name: '安全提醒', trigger: '事件触发', queue: 0, failed: 0 },
    { id: 'welcome', name: '欢迎邮件', trigger: '注册触发', queue: 0, failed: 0 }
  ];

  let q = $state('');
  let statusFilter = $state('');
  let selectedIds = $state<string[]>([]);

  let broadcastTitle = $state('');
  let broadcastContent = $state('');
  let broadcastTarget = $state('all');
  let sending = $state(false);

  const displayedTemplates = $derived.by(() => {
    let list = mockTemplates;
    if (q.trim()) {
      const kw = q.trim().toLowerCase();
      list = list.filter((t) => t.name.toLowerCase().includes(kw) || t.id.toLowerCase().includes(kw));
    }
    return list;
  });

  let allSelected = $derived(
    displayedTemplates.length > 0 && selectedIds.length === displayedTemplates.length
  );
  function toggleAll() {
    if (allSelected) selectedIds = [];
    else selectedIds = displayedTemplates.map((t) => t.id);
  }
  function toggleRow(id: string) {
    if (selectedIds.includes(id)) selectedIds = selectedIds.filter((x) => x !== id);
    else selectedIds = [...selectedIds, id];
  }
</script>

<svelte:head>
  <title>通知与邮件 — BBLBB Admin</title>
</svelte:head>

<PageHeader title="通知与邮件" />

<!-- 卡片 1：模板与队列 -->
<section class="app-card" style="margin-bottom:14px;">
  <header class="app-card__head">
    <h2>模板与队列</h2>
  </header>
  <div class="app-card__body">
    <!-- 工具栏 -->
    <div style="display:flex;flex-direction:column;gap:8px;margin-bottom:14px;">
      <input
        type="search"
        bind:value={q}
        class="app-field"
        placeholder="搜索当前列表..."
        aria-label="搜索当前列表"
      />
      <div style="display:flex;align-items:center;justify-content:space-between;gap:8px;">
        <select
          class="app-select"
          bind:value={statusFilter}
          aria-label="状态筛选"
          style="min-width:140px;"
        >
          <option value="">全部状态</option>
          <option value="active">启用中</option>
          <option value="paused">已暂停</option>
        </select>
        {#if q || statusFilter}
          <button type="button" class="btn ghost sm" onclick={() => { q = ''; statusFilter = ''; }}>
            清除
          </button>
        {/if}
      </div>
    </div>

    {#if selectedIds.length > 0}
      <div class="app-notice" style="display:flex;align-items:center;justify-content:space-between;padding:8px 12px;margin-bottom:10px;background:var(--color-bg-subtle);border-radius:var(--radius-sm);">
        <span style="font-size:var(--text-xs);font-weight:600;">{selectedIds.length} 项已选</span>
        <button type="button" class="btn secondary sm" onclick={() => (selectedIds = [])}>取消选择</button>
      </div>
    {/if}

    <div class="app-table-wrap">
      <table class="app-table" aria-label="通知模板列表">
        <thead>
          <tr>
            <th style="width:40px;text-align:center;">
              <input
                type="checkbox"
                checked={allSelected}
                onchange={toggleAll}
                aria-label="全选当前列表"
              />
            </th>
            <th>模板</th>
            <th>触发方式</th>
            <th>队列</th>
            <th>失败</th>
          </tr>
        </thead>
        <tbody>
          {#each displayedTemplates as tpl (tpl.id)}
            <tr>
              <td style="text-align:center;">
                <input
                  type="checkbox"
                  checked={selectedIds.includes(tpl.id)}
                  onchange={() => toggleRow(tpl.id)}
                  aria-label="选择此项"
                />
              </td>
              <td>
                <b>{tpl.name}</b>
                <span class="sub" style="display:block;font-size:11px;color:var(--color-text-secondary);">{tpl.trigger}</span>
              </td>
              <td><span class="text-secondary" style="font-size:12px;">{tpl.trigger}</span></td>
              <td><span style="font-size:13px;font-weight:500;">{tpl.queue}</span></td>
              <td><span style="font-size:13px;color:{tpl.failed > 0 ? 'var(--color-danger)' : 'inherit'};font-weight:500;">{tpl.failed}</span></td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </div>
</section>

<!-- 卡片 2：全员广播（原型同款字段顺序） -->
<section class="app-card" style="margin-bottom:14px;">
  <header class="app-card__head">
    <h2>全员广播</h2>
  </header>
  <div class="app-card__body">
    <form
      method="POST"
      action="?/broadcast"
      use:enhance={() => {
        sending = true;
        return async ({ result, update }) => {
          sending = false;
          if (result.type === 'success') {
            showToast('全员广播已发送', 'success');
            broadcastTitle = '';
            broadcastContent = '';
            await update();
          } else {
            await update();
          }
        };
      }}
      class="stack"
      style="gap:12px;"
    >
      <label>
        <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">
          标题 <span style="color:var(--color-danger);">*</span>
        </span>
        <input
          type="text"
          name="title"
          class="input-field"
          placeholder="例如：本周末例行维护"
          required
          bind:value={broadcastTitle}
          style="width:100%;"
        />
      </label>

      <label>
        <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">
          内容 <span style="color:var(--color-danger);">*</span>
        </span>
        <textarea
          name="body"
          class="input-field"
          rows="4"
          placeholder="输入广播正文内容…"
          required
          bind:value={broadcastContent}
          style="width:100%;"
        ></textarea>
      </label>

      <label>
        <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">目标</span>
        <select name="target" class="app-select" bind:value={broadcastTarget} style="width:100%;">
          <option value="all">全体成员</option>
          <option value="active">活跃用户</option>
          <option value="admins">管理员与版主</option>
        </select>
      </label>

      <div>
        <Button text={sending ? '发送中…' : '发送广播'} variant="primary" type="submit" disabled={sending} />
      </div>
    </form>
  </div>
</section>

<!-- 卡片 3：失败队列（原型同款） -->
<section class="app-card" style="margin-bottom:14px;">
  <header class="app-card__head">
    <h2>失败队列</h2>
  </header>
  <div class="app-card__body" style="display:flex;align-items:center;justify-content:space-between;flex-wrap:wrap;gap:12px;">
    <div style="font-size:13px;color:var(--color-text-primary);">
      <b>2</b> 条待重试 · 最近失败：验证邮件（SMTP 超时）
    </div>
    <button
      type="button"
      class="btn secondary sm"
      onclick={() => showToast('重试任务已提交', 'success')}
    >
      重试失败
    </button>
  </div>
</section>

<!-- 卡片 4：已发送广播（原型同款虚线空态卡片） -->
<section class="app-card">
  <header class="app-card__head">
    <h2>已发送广播</h2>
  </header>
  <div class="app-card__body">
    <div style="padding:28px 16px;text-align:center;border:1px dashed var(--color-border);border-radius:var(--radius-md);background:var(--color-bg-subtle, rgba(0,0,0,0.02));display:flex;flex-direction:column;align-items:center;gap:8px;">
      <Icon name="bell" size={24} />
      <strong style="font-size:14px;color:var(--color-text-primary);">还没有已发送的广播</strong>
      <span class="text-secondary" style="font-size:12px;">使用上方表单发送第一条全员广播。</span>
    </div>
  </div>
</section>
