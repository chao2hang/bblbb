<script lang="ts">
  // M18-ADMIN-OAUTH：OAuth 客户端管理页（对齐原型 #admin-oauth）。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { enhance } from '$app/forms';
  import type { AdminOAuthActionData, AdminOAuthPageData } from './+page.server';

  let { data, form }: { data: AdminOAuthPageData; form?: AdminOAuthActionData | null } = $props();

  const mockClients = [
    { id: 'cli_***8f2a', name: 'BBLBB CLI', type: 'Confidential', status: 'active' },
    { id: 'demo_***3c1d', name: '第三方 Demo', type: 'Public', status: 'active' }
  ];

  let q = $state('');
  let statusFilter = $state('');
  let selectedIds = $state<string[]>([]);

  const displayedClients = $derived.by(() => {
    let list = mockClients;
    if (q.trim()) {
      const kw = q.trim().toLowerCase();
      list = list.filter((c) => c.name.toLowerCase().includes(kw) || c.id.toLowerCase().includes(kw));
    }
    return list;
  });

  let allSelected = $derived(
    displayedClients.length > 0 && selectedIds.length === displayedClients.length
  );
  function toggleAll() {
    if (allSelected) selectedIds = [];
    else selectedIds = displayedClients.map((c) => c.id);
  }
  function toggleRow(id: string) {
    if (selectedIds.includes(id)) selectedIds = selectedIds.filter((x) => x !== id);
    else selectedIds = [...selectedIds, id];
  }
</script>

<svelte:head>
  <title>OAuth 客户端 — BBLBB Admin</title>
</svelte:head>

<PageHeader title="OAuth 客户端" />

<!-- 顶部提示框（原型同款） -->
<div style="background:var(--color-bg-subtle, rgba(0,0,0,0.03));padding:14px 16px;border-radius:var(--radius-sm);font-size:13px;line-height:1.5;color:var(--color-text-secondary);margin-bottom:14px;">
  所有 redirect_uri 必须 HTTPS；Client Secret 仅在创建/重置时显示一次。
</div>

<section class="app-card">
  <header class="app-card__head">
    <h2>客户端列表</h2>
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
          <option value="active">已启用</option>
          <option value="disabled">已禁用</option>
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
      <table class="app-table" aria-label="客户端列表">
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
            <th>名称</th>
            <th>Client ID</th>
            <th>类型</th>
          </tr>
        </thead>
        <tbody>
          {#each displayedClients as client (client.id)}
            <tr>
              <td style="text-align:center;">
                <input
                  type="checkbox"
                  checked={selectedIds.includes(client.id)}
                  onchange={() => toggleRow(client.id)}
                  aria-label="选择此项"
                />
              </td>
              <td><b>{client.name}</b></td>
              <td><code style="padding:2px 6px;background:var(--color-bg-subtle);border-radius:3px;">{client.id}</code></td>
              <td><span class="text-secondary" style="font-size:13px;">{client.type}</span></td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </div>
</section>
