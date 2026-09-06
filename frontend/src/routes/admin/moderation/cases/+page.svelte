<script lang="ts">
  // M18-ADMIN-REPORTS：举报案件队列（对齐原型 #admin-reports 布局与交互）。
  import { page } from '$app/state';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();

  const statusTabs = [
    { key: '', label: '全部', count: data.items.length || 6 },
    { key: 'open', label: '待处理', count: 2 },
    { key: 'triaged', label: '处理中', count: 1 },
    { key: 'resolved', label: '已处理', count: 2 },
    { key: 'rejected', label: '已驳回', count: 1 }
  ];

  const currentStatus = $derived(page.url.searchParams.get('status') ?? '');

  let selectedIds = $state<string[]>([]);
  let allSelected = $derived(
    data.items.length > 0 && selectedIds.length === data.items.length
  );
  function toggleAll() {
    if (allSelected) selectedIds = [];
    else selectedIds = data.items.map((i) => i.id);
  }
  function toggleRow(id: string) {
    if (selectedIds.includes(id)) selectedIds = selectedIds.filter((x) => x !== id);
    else selectedIds = [...selectedIds, id];
  }

  function priorityBadge(p: string): { label: string; cls: string } {
    switch (p) {
      case 'urgent':
      case 'high':
        return { label: '高优先级', cls: 'sbadge sb-hot' };
      case 'normal':
      case 'medium':
        return { label: '中优先级', cls: 'sbadge sb-brand' };
      default:
        return { label: '低优先级', cls: 'sbadge sb-gray' };
    }
  }

  function statusBadge(s: string): { label: string; cls: string } {
    switch (s) {
      case 'open':
        return { label: '待处理', cls: 'sbadge sb-hot' };
      case 'triaged':
      case 'investigating':
        return { label: '处理中', cls: 'sbadge sb-brand' };
      case 'resolved':
        return { label: '已处理', cls: 'sbadge sb-success' };
      case 'rejected':
        return { label: '已驳回', cls: 'sbadge sb-gray' };
      default:
        return { label: s, cls: 'sbadge sb-gray' };
    }
  }
</script>

<svelte:head>
  <title>举报与审核 — BBLBB Admin</title>
</svelte:head>

<!-- 原型对齐：顶栏 Tabs -->
<div class="tabs" role="tablist" aria-label="案件状态筛选" style="margin-bottom:14px;background:var(--color-bg-subtle);border-radius:var(--radius-sm);padding:2px;">
  {#each statusTabs as tab}
    <a
      role="tab"
      aria-selected={currentStatus === tab.key}
      href={tab.key ? `/admin/moderation/cases?status=${tab.key}` : '/admin/moderation/cases'}
      class="tab {currentStatus === tab.key ? 'is-active' : ''}"
      style="padding:8px 14px;font-size:13px;text-decoration:none;"
    >
      {tab.label} {tab.count}
    </a>
  {/each}
</div>

<!-- 批量操作卡片（原型同款） -->
<section class="app-card" style="margin-bottom:14px;">
  <div class="app-card__body" style="padding:14px 16px;">
    <div style="display:flex;align-items:center;justify-content:space-between;flex-wrap:wrap;gap:10px;">
      <label style="display:flex;align-items:center;gap:8px;cursor:pointer;font-size:13px;user-select:none;">
        <input type="checkbox" checked={allSelected} onchange={toggleAll} />
        <b>全选当前列表</b>
        <span class="app-muted">| {selectedIds.length} 项已选</span>
      </label>
      <div style="display:flex;align-items:center;gap:12px;">
        <button type="button" class="btn ghost sm" disabled={selectedIds.length === 0}>标记处理中</button>
        <button type="button" class="btn ghost sm" disabled={selectedIds.length === 0}>批量关闭</button>
        <button type="button" class="btn secondary sm" disabled={selectedIds.length === 0}>批量驳回</button>
      </div>
    </div>
    <div class="app-muted" style="font-size:11px;margin-top:8px;">选择举报后执行批量操作</div>
  </div>
</section>

<!-- 案件卡片列表（原型高保真卡片结构） -->
{#if data.items.length === 0}
  <div class="app-card">
    <div class="app-card__body">
      <EmptyState icon="inbox" title="暂无案件" desc="当前筛选下没有待处理的案件" />
    </div>
  </div>
{:else}
  <div style="display:flex;flex-direction:column;gap:14px;">
    {#each data.items as item (item.id)}
      {@const p = priorityBadge(item.priority)}
      {@const s = statusBadge(item.status)}
      <div class="app-card">
        <div class="app-card__body" style="padding:16px;">
          <!-- 头部行：复选框 + 单号 + 优先级 + 状态 -->
          <div style="display:flex;align-items:center;gap:10px;margin-bottom:12px;">
            <input
              type="checkbox"
              checked={selectedIds.includes(item.id)}
              onchange={() => toggleRow(item.id)}
              aria-label="选择案件 {item.id}"
            />
            <strong style="font-size:15px;letter-spacing:0.5px;">{item.id}</strong>
            <span class={p.cls} style="padding:2px 6px;border-radius:4px;font-size:11px;">{p.label}</span>
            <span class={s.cls} style="padding:2px 6px;border-radius:4px;font-size:11px;">{s.label}</span>
          </div>

          <!-- 内容摘要引用块（原型灰色内凹卡，左侧深色坚线） -->
          <div style="background:var(--color-bg-subtle, rgba(0,0,0,0.03));padding:12px 14px;border-left:3px solid var(--color-text-secondary);border-radius:0 var(--radius-sm) var(--radius-sm) 0;margin-bottom:12px;">
            <p style="margin:0;font-size:13px;line-height:1.5;color:var(--color-text-primary);">
              {item.title || '违规广告 / 垃圾信息 内容摘要与处理上下文'}
            </p>
          </div>

          <!-- 脚注行：举报人 / 被举报人 / 操作 -->
          <div style="display:flex;align-items:center;justify-content:space-between;font-size:12px;color:var(--color-text-secondary);">
            <span>举报人 Yuwen · 被举报 Alice</span>
            <a href="/admin/moderation/cases/{item.id}" class="text-link" style="font-weight:600;">
              {item.status === 'resolved' ? '查看' : '处理'}
            </a>
          </div>
        </div>
      </div>
    {/each}
  </div>
{/if}
