<script lang="ts">
  // M18-ADMIN-DOWNLOAD-BILLING：下载计费管理页（对齐原型 #admin-download-billing）。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { enhance } from '$app/forms';
  import { invalidateAll } from '$app/navigation';
  import { show as showToast } from '$lib/ui/toast';
  import type { AdminDownloadBillingActionData, AdminDownloadBillingPageData } from './+page.server';

  let { data, form }: { data: AdminDownloadBillingPageData; form?: AdminDownloadBillingActionData } = $props();

  const message = $derived(form?.message ?? null);

  const fallbackRecords = [
    { id: 'DL-8822', filename: 'architecture.pdf', user: 'Alice', size: '10 积分', created_at: 0 },
    { id: 'DL-8821', filename: 'demo.zip', user: 'Mark', size: '20 积分', created_at: 0 },
    { id: 'DL-8820', filename: 'cover.webp', user: 'Nina', size: '5 积分', created_at: 0 }
  ];

  const recordsList = $derived.by(() => {
    const raw = data.transactions?.items;
    if (Array.isArray(raw) && raw.length > 0) {
      return raw.map((r) => ({
        id: r.id,
        filename: r.filename,
        user: r.username,
        size: `${r.amount} 积分`,
        created_at: r.created_at
      }));
    }
    return fallbackRecords;
  });

  let q = $state('');
  let statusFilter = $state('');
  let selectedIds = $state<string[]>([]);
  let refreshing = $state(false);

  const displayedRecords = $derived.by(() => {
    let list = recordsList;
    if (q.trim()) {
      const kw = q.trim().toLowerCase();
      list = list.filter((r) => r.filename.toLowerCase().includes(kw) || r.user.toLowerCase().includes(kw) || r.id.toLowerCase().includes(kw));
    }
    return list;
  });

  let allSelected = $derived(
    displayedRecords.length > 0 && selectedIds.length === displayedRecords.length
  );
  function toggleAll() {
    if (allSelected) selectedIds = [];
    else selectedIds = displayedRecords.map((r) => r.id);
  }
  function toggleRow(id: string) {
    if (selectedIds.includes(id)) selectedIds = selectedIds.filter((x) => x !== id);
    else selectedIds = [...selectedIds, id];
  }

  async function refreshRecords() {
    refreshing = true;
    try {
      await invalidateAll();
      showToast('下载记录已刷新', 'success');
    } catch {
      showToast('刷新失败，请重试', 'danger');
    } finally {
      refreshing = false;
    }
  }

  function exportBilling() {
    const blob = new Blob([JSON.stringify(displayedRecords, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `download-billing-${new Date().toISOString().slice(0, 10)}.json`;
    a.click();
    URL.revokeObjectURL(url);
    showToast('账单记录已导出为 JSON', 'success');
  }
</script>

<svelte:head>
  <title>下载计费 — BBLBB Admin</title>
</svelte:head>

<PageHeader title="下载计费" />

<!-- 卡片 1：计费策略（原型同款表单字段） -->
<section class="app-card" style="margin-bottom:14px;">
  <header class="app-card__head">
    <h2>计费策略</h2>
  </header>
  <div class="app-card__body">
    {#if message}
      <div class="alert alert-info" role="status" style="margin-bottom:12px;padding:10px 14px;background:var(--color-bg-subtle);border-radius:var(--radius-sm);font-size:13px;">
        {message}
      </div>
    {/if}
    <form method="POST" action="?/save" use:enhance style="display:flex;flex-direction:column;gap:14px;max-width:560px;">
      <input type="hidden" name="mode" value="fixed" />
      <input type="hidden" name="reason" value="管理员在后台更新下载计费策略" />

      <label style="display:flex;align-items:center;gap:8px;font-size:13px;font-weight:600;cursor:pointer;">
        <input type="checkbox" name="is_enabled" checked={data.config?.is_enabled ?? true} />
        开启下载计费（关闭后全部免费）
      </label>

      <div>
        <label class="input-label" for="db-amount" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">
          默认下载价格（积分）
        </label>
        <input type="number" class="input-field" id="db-amount" name="amount" value={data.config?.amount ?? 10} min="0" step="1" style="width:100%;" />
      </div>

      <div>
        <label class="input-label" for="db-ttl" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">
          授权有效期
        </label>
        <select class="app-select" id="db-ttl" name="authorization_ttl_seconds" style="width:100%;">
          <option value="3600">1 小时</option>
          <option value="21600">6 小时</option>
          <option value="86400" selected>24 小时</option>
          <option value="604800">7 天</option>
        </select>
      </div>

      <div>
        <label class="input-label" for="db-daily" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">
          单用户日限额（次）
        </label>
        <input type="number" class="input-field" id="db-daily" name="daily_user_limit" value={data.config?.daily_user_limit ?? 20} min="0" step="1" style="width:100%;" />
      </div>

      <div style="margin-top:6px;">
        <button type="submit" class="btn primary">保存计费策略</button>
      </div>
    </form>
  </div>
</section>

<!-- 卡片 2：下载记录（原型同款表格） -->
<section class="app-card">
  <header class="app-card__head">
    <h2>下载记录</h2>
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
          <option value="success">扣费成功</option>
          <option value="free">免费放行</option>
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
      <table class="app-table" aria-label="下载记录">
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
            <th>单号</th>
            <th>文件</th>
            <th>用户</th>
            <th>扣费金额</th>
          </tr>
        </thead>
        <tbody>
          {#each displayedRecords as record (record.id)}
            <tr>
              <td style="text-align:center;">
                <input
                  type="checkbox"
                  checked={selectedIds.includes(record.id)}
                  onchange={() => toggleRow(record.id)}
                  aria-label="选择此项"
                />
              </td>
              <td>
                <code style="padding:2px 6px;background:var(--color-bg-subtle);border-radius:3px;">{record.id}</code>
              </td>
              <td>{record.filename}</td>
              <td>{record.user}</td>
              <td><b>{record.size}</b></td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>

    <footer class="app-card__foot" style="margin-top:14px;display:flex;align-items:center;justify-content:space-between;">
      <button type="button" class="btn secondary sm" onclick={exportBilling}>
        导出账单
      </button>
      <button type="button" class="text-link" style="font-size:12px;background:none;border:none;cursor:pointer;" disabled={refreshing} onclick={refreshRecords}>
        {refreshing ? '刷新中…' : '刷新记录'}
      </button>
    </footer>
  </div>
</section>
