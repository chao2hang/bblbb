<script lang="ts">
  // M18-ADMIN-LEVELS：等级管理页（对齐原型 #admin-levels）。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { show as showToast } from '$lib/ui/toast';
  import type { AdminLevelsActionData, AdminLevelsPageData } from './+page.server';

  let { data, form }: { data: AdminLevelsPageData; form?: AdminLevelsActionData | null } = $props();

  const mockLevels = [
    { level: 'Lv.0', req: '回帖 0 · 发帖 0 · 送赞 0 · 获赞 0 · 登录 0 天', count: '0 名' },
    { level: 'Lv.1', req: '回帖 3 · 发帖 1 · 送赞 1 · 获赞 1 · 登录 3 天', count: '1 名' },
    { level: 'Lv.2', req: '回帖 10 · 发帖 3 · 送赞 5 · 获赞 3 · 登录 7 天', count: '3 名' },
    { level: 'Lv.3', req: '回帖 30 · 发帖 8 · 送赞 15 · 获赞 10 · 登录 30 天', count: '4 名' },
    { level: 'Lv.4', req: '回帖 80 · 发帖 20 · 送赞 40 · 获赞 30 · 登录 90 天', count: '2 名' }
  ];

  let q = $state('');
  let statusFilter = $state('');
  let selectedIds = $state<string[]>([]);

  const displayedLevels = $derived.by(() => {
    let list = mockLevels;
    if (q.trim()) {
      const kw = q.trim().toLowerCase();
      list = list.filter((l) => l.level.toLowerCase().includes(kw) || l.req.toLowerCase().includes(kw));
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
    <button type="button" class="btn secondary sm" onclick={() => showToast('已发起用户数实时联查', 'info')}>
      用户数实时联查
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
          aria-label="状态筛选"
          style="min-width:140px;"
        >
          <option value="">全部状态</option>
          <option value="enabled">已启用</option>
          <option value="disabled">已停用</option>
        </select>
        {#if q || statusFilter}
          <button type="button" class="btn ghost sm" onclick={() => { q = ''; statusFilter = ''; }}>
            清除
          </button>
        {/if}
      </div>
    </div>

    <!-- 统计行 -->
    <div style="font-size:12px;color:var(--color-text-secondary);margin-bottom:12px;line-height:1.6;">
      <div>共 5 个等级 · 满足活跃条件后自动进入对应等级</div>
      <div>共 10 名用户</div>
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
            <th>等级</th>
            <th>活跃度门槛</th>
            <th>对应用户</th>
          </tr>
        </thead>
        <tbody>
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
              <td><b style="color:var(--color-brand);">{item.level}</b></td>
              <td><span style="font-size:12px;color:var(--color-text-secondary);line-height:1.5;">{item.req}</span></td>
              <td><b>{item.count}</b></td>
            </tr>
          {/each}
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
