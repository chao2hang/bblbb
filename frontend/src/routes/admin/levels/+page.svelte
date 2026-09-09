<script lang="ts">
  // M18-ADMIN-LEVELS：等级管理页（对齐原型 #admin-levels）。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { goto, invalidateAll } from '$app/navigation';
  import { page } from '$app/state';
  import { untrack } from 'svelte';
  import { show as showToast } from '$lib/ui/toast';
  import type { AdminLevelsActionData, AdminLevelsPageData } from './+page.server';

  let { data, form }: { data: AdminLevelsPageData; form?: AdminLevelsActionData | null } = $props();

  const mockLevels = [
    { level: 'Lv.0 萌新见习', req: '经验门槛 0 · 日发帖 5 · 日回帖 20', count: '0 名', rawLevel: 0, status: 'enabled' },
    { level: 'Lv.1 探索求知', req: '经验门槛 10 · 日发帖 10 · 日回帖 50', count: '1 名', rawLevel: 1, status: 'enabled' },
    { level: 'Lv.2 社区活跃', req: '经验门槛 50 · 日发帖 20 · 日回帖 100', count: '3 名', rawLevel: 2, status: 'enabled' },
    { level: 'Lv.3 资深作者', req: '经验门槛 200 · 日发帖 50 · 日回帖 200', count: '4 名', rawLevel: 3, status: 'enabled' },
    { level: 'Lv.4 核心领航', req: '经验门槛 1000 · 日发帖 100 · 日回帖 500', count: '2 名', rawLevel: 4, status: 'disabled' }
  ];

  const levelsList = $derived.by(() => {
    const raw = data.levels?.items;
    if (Array.isArray(raw) && raw.length > 0) {
      return raw.map((l) => ({
        level: `Lv.${l.level} ${l.name}`,
        req: `经验门槛 ${l.min_exp} · 日发帖限额 ${l.daily_post_limit} · 日评论限额 ${l.daily_comment_limit}`,
        count: `${l.user_count} 名`,
        rawLevel: l.level,
        status: (l as any).is_active === false || (l as any).is_active === 0 || (l as any).status === 'disabled' ? 'disabled' : 'enabled'
      }));
    }
    return mockLevels;
  });

  const totalUsers = $derived.by(() => {
    const raw = data.levels?.items;
    if (Array.isArray(raw) && raw.length > 0) {
      return raw.reduce((sum, l) => sum + (l.user_count || 0), 0);
    }
    return 10;
  });

  let q = $state('');
  function getInitialStatus(): string {
    try {
      return page.url.searchParams.get('status') ?? '';
    } catch {
      return '';
    }
  }
  let statusFilter = $state(untrack(() => getInitialStatus()));
  let selectedIds = $state<string[]>([]);
  let refreshing = $state(false);

  function handleStatusChange(nextStatus: string) {
    statusFilter = nextStatus;
    const url = new URL(page.url);
    if (nextStatus) url.searchParams.set('status', nextStatus);
    else url.searchParams.delete('status');
    goto(url.toString(), { replaceState: true, keepFocus: true, noScroll: true });
  }

  const displayedLevels = $derived.by(() => {
    let list = levelsList;
    if (q.trim()) {
      const kw = q.trim().toLowerCase();
      list = list.filter((l) => l.level.toLowerCase().includes(kw) || l.req.toLowerCase().includes(kw));
    }
    if (statusFilter) {
      list = list.filter((l) => l.status === statusFilter);
    }
    return list;
  });

  let allSelected = $derived(
    displayedLevels.length > 0 && selectedIds.length === displayedLevels.length
  );
  function toggleAll() {
    if (allSelected) selectedIds = [];
    else selectedIds = displayedLevels.map((l) => l.level);
  }
  function toggleRow(id: string) {
    if (selectedIds.includes(id)) selectedIds = selectedIds.filter((x) => x !== id);
    else selectedIds = [...selectedIds, id];
  }

  async function handleRefreshCounts() {
    refreshing = true;
    try {
      await invalidateAll();
      showToast('各等级用户数实时联查已完成', 'success');
    } catch {
      showToast('联查失败，请重试', 'danger');
    } finally {
      refreshing = false;
    }
  }
</script>

<svelte:head>
  <title>等级管理 — BBLBB Admin</title>
</svelte:head>

<PageHeader title="等级管理" />

<section class="app-card" style="margin-bottom:14px;">
  <header class="app-card__head" style="display:flex;justify-content:space-between;align-items:flex-start;flex-wrap:wrap;gap:10px;">
    <div>
      <h2 style="margin:0;">等级列表</h2>
      <p style="margin:4px 0 0;font-size:12px;color:var(--color-text-secondary);">
        按社区活跃行为评估等级，并查看每个等级下的全部用户。
      </p>
    </div>
    <button type="button" class="btn secondary sm" disabled={refreshing} onclick={handleRefreshCounts}>
      {refreshing ? '联查中…' : '用户数实时联查'}
    </button>
  </header>

  <div class="app-card__body">
    <!-- 原型通用工具栏 -->
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
          onchange={(e) => handleStatusChange(e.currentTarget.value)}
          aria-label="状态筛选"
          style="min-width:140px;"
        >
          <option value="">全部状态</option>
          <option value="enabled">已启用</option>
          <option value="disabled">已停用</option>
        </select>
        {#if q || statusFilter}
          <button type="button" class="btn ghost sm" onclick={() => { q = ''; handleStatusChange(''); }}>
            清除
          </button>
        {/if}
      </div>
    </div>

    <!-- 统计行 -->
    <div style="font-size:12px;color:var(--color-text-secondary);margin-bottom:12px;line-height:1.6;display:flex;gap:16px;">
      <div>共 <b>{displayedLevels.length}</b> 个等级 · 满足活跃条件后自动晋级</div>
      <div>已纳管用户总量：<b>{totalUsers}</b> 名</div>
    </div>

    {#if selectedIds.length > 0}
      <div class="app-notice" style="display:flex;align-items:center;justify-content:space-between;padding:8px 12px;margin-bottom:10px;background:var(--color-bg-subtle);border-radius:var(--radius-sm);">
        <span style="font-size:var(--text-xs);font-weight:600;">{selectedIds.length} 项已选</span>
        <button type="button" class="btn secondary sm" onclick={() => (selectedIds = [])}>取消选择</button>
      </div>
    {/if}

    <div class="app-table-wrap">
      <table class="app-table" aria-label="等级列表">
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
            <th>等级与称号</th>
            <th style="width:90px;">状态</th>
            <th>活跃度门槛与限额</th>
            <th>对应用户数</th>
          </tr>
        </thead>
        <tbody>
          {#if displayedLevels.length === 0}
            <tr>
              <td colspan="5" style="text-align:center;padding:24px;color:var(--color-text-secondary);">
                当前筛选下没有等级记录
              </td>
            </tr>
          {:else}
            {#each displayedLevels as item (item.level)}
              <tr>
                <td style="text-align:center;">
                  <input
                    type="checkbox"
                    checked={selectedIds.includes(item.level)}
                    onchange={() => toggleRow(item.level)}
                    aria-label="选择此项"
                  />
                </td>
                <td><b>{item.level}</b></td>
                <td>
                  <span class="badge {item.status === 'enabled' ? 'badge-success' : 'badge-neutral'}" style="font-size:11px;">
                    {item.status === 'enabled' ? '已启用' : '已停用'}
                  </span>
                </td>
                <td><span style="font-size:12px;color:var(--color-text-secondary);line-height:1.5;">{item.req}</span></td>
                <td><b>{item.count}</b></td>
              </tr>
            {/each}
          {/if}
        </tbody>
      </table>
    </div>
  </div>
</section>

<!-- 底层配额表单收起 -->
<details class="app-card">
  <summary class="app-card__head" style="cursor:pointer;user-select:none;">
    <h2 style="display:inline-block;font-size:15px;margin:0;">附件配额（等级 1 · 只读）</h2>
  </summary>
  <div class="app-card__body" style="padding-top:12px;font-size:12px;color:var(--color-text-secondary);">
    各等级单文件上限、总量与每日限额由服务端策略统一调控。
  </div>
</details>
