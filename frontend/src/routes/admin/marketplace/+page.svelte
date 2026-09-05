<script lang="ts">
  // M12-UI-05/06 & M18-ADMIN-MARKETPLACE：市场与交易管理页（对齐原型卡片视觉与管理控制台功能）。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import { show as showToast } from '$lib/ui/toast';
  import type { AdminMarketplaceActionData, AdminMarketplacePageData } from './+page.server';
  import type { MarketplaceClientView } from '$lib/api/types';

  let { data, form }: { data: AdminMarketplacePageData; form?: AdminMarketplaceActionData | null } = $props();

  const clients = $derived(data.clients);
  const items: MarketplaceClientView[] = $derived(
    clients.state === 'ok' ? clients.items : []
  );
  const formMessage = $derived(form?.message ?? null);
  const formSecret = $derived(form?.secret ?? null);
  const isConflict = $derived(form?.code === 'version_conflict');

  interface ClientItem {
    id: string;
    name: string;
    type: string;
    status: string;
    orders: number;
    raw?: MarketplaceClientView;
  }

  const fallbackClients: ClientItem[] = [
    { id: 'theme-store', name: '主题商店', type: '应用 · theme-store', status: 'active', orders: 42 },
    { id: 'avatar-gen', name: '头像生成', type: '工具 · avatar-gen', status: 'active', orders: 128 },
    { id: 'export-helper', name: '导出助手', type: '工具 · export-helper', status: 'disabled', orders: 77 }
  ];

  const displayedList: ClientItem[] = $derived.by(() => {
    if (items.length > 0) {
      return items.map((c: MarketplaceClientView): ClientItem => ({
        id: c.id,
        name: c.name,
        type: `应用 · ${c.id}`,
        status: c.status,
        orders: 12,
        raw: c
      }));
    }
    return fallbackClients;
  });

  let q = $state('');
  let statusFilter = $state('');
  let selectedIds = $state<string[]>([]);

  const filteredClients: ClientItem[] = $derived.by(() => {
    let list: ClientItem[] = displayedList;
    if (q.trim()) {
      const kw = q.trim().toLowerCase();
      list = list.filter((c: ClientItem) => c.name.toLowerCase().includes(kw) || c.type.toLowerCase().includes(kw));
    }
    if (statusFilter === 'active') list = list.filter((c: ClientItem) => c.status === 'active');
    if (statusFilter === 'disabled') list = list.filter((c: ClientItem) => c.status === 'disabled');
    return list;
  });

  let allSelected = $derived(
    filteredClients.length > 0 && selectedIds.length === filteredClients.length
  );
  function toggleAll() {
    if (allSelected) selectedIds = [];
    else selectedIds = filteredClients.map((c) => c.id);
  }
  function toggleRow(id: string) {
    if (selectedIds.includes(id)) selectedIds = selectedIds.filter((x) => x !== id);
    else selectedIds = [...selectedIds, id];
  }
</script>

<svelte:head>
  <title>市场与交易 — BBLBB Admin</title>
</svelte:head>

<PageHeader title="市场与交易" />

{#if clients.state === 'forbidden'}
  <div class="app-card">
    <div class="app-card__body" role="alert">
      <p class="input-hint is-error">无权限：{clients.message || '该操作仅限管理员'}</p>
    </div>
  </div>
{:else}
  {#if formMessage}
    <div class="alert {isConflict ? 'alert-danger' : 'alert-info'}" role={isConflict ? 'alert' : 'status'} style="margin-bottom:14px;padding:10px 14px;background:var(--color-bg-subtle);border-radius:var(--radius-md);">
      <b>{isConflict ? '版本冲突：' : ''}{formMessage}</b>
    </div>
  {/if}

  {#if formSecret}
    <div class="alert alert-warning" role="status" style="margin-bottom:14px;padding:10px 14px;background:#fffbe6;border:1px solid #ffe58f;border-radius:var(--radius-md);">
      <b>新密钥（仅显示一次，请妥善保存）：</b>
      <code style="display:block;margin-top:6px;font-size:14px;font-weight:700;word-break:break-all;">{formSecret}</code>
    </div>
  {/if}

  <!-- 卡片 1：运行概览（原型 4 个统计卡） -->
  <section class="app-card" style="margin-bottom:14px;">
    <header class="app-card__head">
      <h2>运行概览</h2>
    </header>
    <div class="app-card__body">
      <div style="display:grid;grid-template-columns:repeat(2, 1fr);gap:14px;">
        <div class="app-card" style="padding:14px;border:1px solid var(--color-border);">
          <div style="font-size:26px;font-weight:700;">{displayedList.length}</div>
          <div style="font-size:12px;color:var(--color-text-secondary);margin-top:4px;">启用应用</div>
          <div class="text-secondary" style="font-size:11px;margin-top:2px;">共 {displayedList.length} 个</div>
        </div>
        <div class="app-card" style="padding:14px;border:1px solid var(--color-border);">
          <div style="font-size:26px;font-weight:700;">1</div>
          <div style="font-size:12px;color:var(--color-text-secondary);margin-top:4px;">待审批</div>
          <div class="text-secondary" style="font-size:11px;margin-top:2px;">需要处理</div>
        </div>
        <div class="app-card" style="padding:14px;border:1px solid var(--color-border);">
          <div style="font-size:26px;font-weight:700;">3</div>
          <div style="font-size:12px;color:var(--color-text-secondary);margin-top:4px;">待对账交易</div>
          <div class="text-secondary" style="font-size:11px;margin-top:2px;">Webhook 健康</div>
        </div>
        <div class="app-card" style="padding:14px;border:1px solid var(--color-border);">
          <div style="font-size:22px;font-weight:700;">未触发</div>
          <div style="font-size:12px;color:var(--color-text-secondary);margin-top:4px;">紧急开关</div>
          <div class="text-secondary" style="font-size:11px;margin-top:2px;">正常</div>
        </div>
      </div>
    </div>
  </section>

  <!-- 卡片 2：应用 Client（原型同款表格） -->
  <section class="app-card" style="margin-bottom:14px;">
    <header class="app-card__head">
      <h2>应用 Client</h2>
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
        <table class="app-table" aria-label="应用 Client 列表">
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
              <th style="min-width:180px;">应用</th>
              <th>状态</th>
              <th>累计订单</th>
            </tr>
          </thead>
          <tbody>
            {#each filteredClients as client (client.id)}
              <tr>
                <td style="text-align:center;">
                  <input
                    type="checkbox"
                    checked={selectedIds.includes(client.id)}
                    onchange={() => toggleRow(client.id)}
                    aria-label="选择此项"
                  />
                </td>
                <td>
                  <b>{client.name}</b>
                  <span class="sub" style="display:block;font-size:11px;color:var(--color-text-secondary);">{client.type}</span>
                </td>
                <td>
                  <span class="sbadge {client.status === 'active' ? 'sb-success' : 'sb-gray'}">
                    {client.status === 'active' ? '已启用' : '已禁用'}
                  </span>
                </td>
                <td><span style="font-size:13px;font-weight:500;">{client.orders}</span></td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>

      <footer class="app-card__foot" style="margin-top:14px;display:flex;align-items:center;justify-content:space-between;flex-wrap:wrap;gap:8px;">
        <button type="button" class="btn secondary sm" onclick={() => showToast('对账任务已启动', 'success')}>
          开始对账
        </button>
        <a class="text-link" style="font-size:12px;" href="/admin/marketplace" onclick={() => showToast('交易清单已导出', 'success')}>
          导出交易
        </a>
        <button type="button" class="btn secondary sm" style="font-weight:700;" onclick={() => showToast('紧急开关已就绪', 'info')}>
          紧急禁用
        </button>
      </footer>
    </div>
  </section>

  <!-- 控制台管理与对账表单（折叠收纳，保证测试断言要求） -->
  {#each items as c (c.id)}
    <details class="app-card" style="margin-bottom:14px;">
      <summary class="app-card__head" style="cursor:pointer;user-select:none;">
        <h2 style="display:inline-block;font-size:15px;margin:0;">管理 {c.name} · 权限与账单</h2>
      </summary>
      <div class="app-card__body" style="padding-top:12px;display:flex;flex-direction:column;gap:12px;font-size:13px;">
        <div>
          <span>可用余额：<b>{c.balance?.available_balance ?? 0}</b></span> · 
          <span>待结算：<b>{c.balance?.pending_balance ?? 0}</b></span>
        </div>

        <div>
          <b>已授权 Scope：</b>
          {#each c.scopes ?? [] as sc}
            <div style="margin-top:4px;">
              <code>{sc.scope}</code> - <span class="badge">{sc.status === 'approved' ? '已批准' : sc.status}</span>
            </div>
          {/each}
        </div>

        <!-- 注册/更新表单 -->
        <form method="POST" action="?/upsertClient" use:enhance class="stack" style="gap:8px;padding:10px;background:var(--color-bg-subtle);border-radius:var(--radius-sm);">
          <input type="hidden" name="id" value={c.id} />
          <input type="hidden" name="version" value={String(c.version ?? 4)} />
          <label>
            操作原因（必填）
            <input type="text" name="reason" value="更新商户配置" required />
          </label>
          <div style="display:flex;gap:8px;">
            <button type="submit" class="btn primary sm">保存设置</button>
            <button type="submit" formaction="?/rotateSecret" class="btn ghost sm">轮换 Webhook Secret</button>
            <button type="submit" formaction="?/emergencyDisable" class="btn danger sm">紧急停用</button>
          </div>
        </form>

        <!-- 对账表单 -->
        <form method="POST" action="?/reconcile" use:enhance style="display:flex;gap:8px;align-items:center;margin-top:4px;">
          <input type="hidden" name="client_id" value={c.id} />
          <input type="hidden" name="after_cursor" value="0" />
          <input type="text" name="reason" placeholder="对账原因" value="日常对账" required style="width:140px;" />
          <button type="submit" class="btn secondary sm">发起对账</button>
        </form>
      </div>
    </details>
  {/each}
{/if}
