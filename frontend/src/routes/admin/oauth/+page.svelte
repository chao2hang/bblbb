<script lang="ts">
  // M18-ADMIN-OAUTH：OAuth 客户端管理页（对齐原型 #admin-oauth）。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { enhance } from '$app/forms';
  import { show as showToast } from '$lib/ui/toast';
  import type { AdminOAuthActionData, AdminOAuthPageData } from './+page.server';

  let { data, form }: { data: AdminOAuthPageData; form?: AdminOAuthActionData | null } = $props();

  const mockClients = [
    { id: 'cli_8f2a9c1d', name: 'BBLBB CLI', type: 'Confidential', status: 'active' },
    { id: 'demo_3c1db8f2', name: '第三方接入应用 Demo', type: 'Public', status: 'active' }
  ];

  const clientsList = $derived.by(() => {
    const raw = form?.clients ?? data.clients;
    if (raw && raw.length > 0) {
      return raw.map((c) => ({
        id: c.client_id || c.id,
        name: c.name,
        type: c.client_type || 'Confidential',
        status: c.status || 'active'
      }));
    }
    return mockClients;
  });

  let q = $state('');
  let statusFilter = $state('');
  let selectedIds = $state<string[]>([]);

  const displayedClients = $derived.by(() => {
    let list = clientsList;
    if (q.trim()) {
      const kw = q.trim().toLowerCase();
      list = list.filter((c) => c.name.toLowerCase().includes(kw) || c.id.toLowerCase().includes(kw));
    }
    if (statusFilter === 'active') list = list.filter((c) => c.status === 'active');
    if (statusFilter === 'disabled') list = list.filter((c) => c.status === 'disabled');
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

{#if form?.message}
  <div class="alert alert-info" role="status" style="margin-bottom:12px;padding:10px 14px;background:var(--color-bg-subtle);border-radius:var(--radius-sm);font-size:13px;">
    {form.message}
  </div>
{/if}

<!-- 顶部提示框（原型同款） -->
<div style="background:var(--color-bg-subtle, rgba(0,0,0,0.03));padding:14px 16px;border-radius:var(--radius-sm);font-size:13px;line-height:1.5;color:var(--color-text-secondary);margin-bottom:14px;">
  所有 redirect_uri 必须使用 HTTPS；Client Secret 仅在创建或重置凭证时显示一次。
</div>

<section class="app-card">
  <header class="app-card__head" style="display:flex;align-items:center;justify-content:space-between;">
    <h2>已注册 OAuth 客户端列表</h2>
    <span class="text-secondary" style="font-size:12px;">共 {displayedClients.length} 个客户端</span>
  </header>
  <div class="app-card__body">
    <!-- 工具栏 -->
    <div style="display:flex;flex-direction:column;gap:8px;margin-bottom:14px;">
      <input
        type="search"
        bind:value={q}
        class="app-field"
        placeholder="按名称或 Client ID 搜索..."
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
            <th>应用名称</th>
            <th>Client ID</th>
            <th>客户端类型</th>
            <th>状态</th>
            <th>操作</th>
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
              <td>
                <span class="sbadge {client.status === 'active' ? 'sb-success' : 'sb-gray'}">
                  {client.status === 'active' ? '已启用' : '已禁用'}
                </span>
              </td>
              <td>
                <form
                  method="POST"
                  action="?/toggle"
                  use:enhance={() => {
                    return async ({ update }) => {
                      await update();
                    };
                  }}
                  style="margin:0;"
                >
                  <input type="hidden" name="id" value={client.id} />
                  <input type="hidden" name="status" value={client.status === 'active' ? 'disabled' : 'active'} />
                  <input type="hidden" name="reason" value={`管理员切换客户端状态为 ${client.status === 'active' ? '已禁用' : '已启用'}`} />
                  <button type="submit" class="btn sm {client.status === 'active' ? 'ghost' : 'secondary'}">
                    {client.status === 'active' ? '禁用' : '启用'}
                  </button>
                </form>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </div>
</section>
