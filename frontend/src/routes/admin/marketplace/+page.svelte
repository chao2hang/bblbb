<script lang="ts">
  // M12-UI-05/06 & M18-ADMIN-MARKETPLACE & M18-ADMIN-BATCH：市场与交易管理页。
  // 约定 A：所有写操作 = 按钮 → Dialog/DangerConfirm（表单在弹层内，成功后
  // toastActionResult → update → 关闭弹层并清理 target）；无 JS 回退横幅保留。
  // 约定 B：Client 表格选择列 + BatchBar →「批量设置状态」Dialog（循环单条端点）。
  // 约定 D：Client 表格行内「状态」入口改为「⋮」三点菜单——单项「设置状态」
  // 打开既有状态 Dialog；表格行外的管理面板（编辑配置/发起对账/轮换/紧急停用）
  // 与页脚操作保持原按钮不动。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { enhance } from '$app/forms';
  import type { SubmitFunction } from '@sveltejs/kit';
  import Button from '$lib/components/ui/Button.svelte';
  import Dialog from '$lib/components/ui/Dialog.svelte';
  import DangerConfirm from '$lib/components/ui/DangerConfirm.svelte';
  import BatchBar from '$lib/components/admin/BatchBar.svelte';
  import RowActionsMenu from '$lib/components/admin/RowActionsMenu.svelte';
  import MetricGroup from '$lib/components/ui/MetricGroup.svelte';
  import { toastActionResult } from '$lib/ui/action-toast';
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

  // JS 启用：动作结果走全局 Toast 浮窗（成功绿/失败红）；顶部内联横幅仅保留为
  // 无 JS 回退（SSR HTML 仍渲染，见 hasJs）。「新密钥（仅显示一次）」横幅不受
  // 影响照常渲染；密钥在 server 的 secret 字段（与 message 分离），toast 只读
  // message，不含 secret。
  let hasJs = $state(false);
  $effect(() => {
    hasJs = true;
  });

  // —— step-up 重新验证（M02-MFA-07）：高敏操作命中 403 step_up_required 时展示 ——
  let reauthLoading = $state(false);
  let reauthCancelled = $state(false);
  let reauthError = $state<string | null>(null);

  $effect(() => {
    if (form?.stepUpRequired) reauthCancelled = false;
  });

  interface ClientItem {
    id: string;
    name: string;
    type: string;
    status: string;
    orders: number;
    raw?: MarketplaceClientView;
  }

  const displayedList: ClientItem[] = $derived(
    items.map((c: MarketplaceClientView): ClientItem => ({
      id: c.id,
      name: c.name,
      type: `应用 · ${c.id}`,
      status: c.status,
      orders: 0,
      raw: c
    }))
  );

  const overviewMetrics = $derived([
    { label: '启用应用', value: displayedList.filter((client) => client.status === 'active').length, note: `共 ${displayedList.length} 个` },
    { label: '待审批', value: displayedList.filter((client) => client.status === 'pending').length, note: '待审批商户' },
    { label: '待对账交易', value: '暂无统计', note: '请通过商户对账操作查看' },
    { label: '紧急开关', value: '按选中商户', note: '需要单选后操作' }
  ]);

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

  /** 行数据（含 version）查找：批量 versions 隐藏域与弹层预填都用它。 */
  function clientById(id: string): MarketplaceClientView | undefined {
    return items.find((c) => c.id === id);
  }

  // ── 弹层 target 状态（约定 A：一个 Dialog 服务一类操作，target 区分行）──
  /** 单行状态切换（?/setStatus）。 */
  let statusTarget = $state<MarketplaceClientView | null>(null);
  let statusValue = $state('');
  let statusReason = $state('');
  function openStatus(c: MarketplaceClientView) {
    statusTarget = c;
    statusValue = c.status;
    statusReason = '';
  }
  function closeStatus() {
    statusTarget = null;
  }

  /** 行「⋮」菜单项（约定 D：单一「设置状态」动作 → 既有状态 Dialog）。 */
  function clientRowActions(clientId: string) {
    return [
      {
        label: '设置状态',
        run: () => {
          const c = clientById(clientId);
          if (c) openStatus(c);
        }
      }
    ];
  }

  /** 批量设置状态（?/batchSetStatus）。 */
  let batchStatusOpen = $state(false);
  let batchStatusValue = $state('');
  let batchReason = $state('');
  function openBatchStatus() {
    batchStatusValue = '';
    batchReason = '';
    batchStatusOpen = true;
  }
  function closeBatchStatus() {
    batchStatusOpen = false;
  }

  /** 全站对账（?/runReconciliationAll）。 */
  let reconcileAllOpen = $state(false);
  let reconcileAllReason = $state('');
  function openReconcileAll() {
    reconcileAllReason = '';
    reconcileAllOpen = true;
  }
  function closeReconcileAll() {
    reconcileAllOpen = false;
  }

  /** 单商户对账（?/runReconciliation）。 */
  let reconcileTarget = $state<MarketplaceClientView | null>(null);
  let reconcileReason = $state('');
  function openReconcile(c: MarketplaceClientView) {
    reconcileTarget = c;
    reconcileReason = '';
  }
  function closeReconcile() {
    reconcileTarget = null;
  }

  /** 退款重试（?/retryRefund）：处理 requested 态退款。 */
  let retryRefundOpen = $state(false);
  let refundIdInput = $state('');
  let refundReason = $state('');
  function openRetryRefund() {
    refundIdInput = '';
    refundReason = '';
    retryRefundOpen = true;
  }
  function closeRetryRefund() {
    retryRefundOpen = false;
  }

  /** 编辑 Client 配置（?/upsertClient）。 */
  let editTarget = $state<MarketplaceClientView | null>(null);
  let editName = $state('');
  let editOwner = $state('');
  let editTerms = $state('');
  let editPrivacy = $state('');
  let editWebhook = $state('');
  let editRedirects = $state('');
  let editFeeBps = $state('');
  let editStatus = $state('');
  let editReason = $state('');
  function openEdit(c: MarketplaceClientView) {
    editTarget = c;
    editName = c.name;
    editOwner = c.owner_user_id ?? '';
    editTerms = c.terms_url ?? '';
    editPrivacy = c.privacy_url ?? '';
    editWebhook = c.webhook_url ?? '';
    editRedirects = (c.redirect_uris ?? []).join('\n');
    editFeeBps = String(c.fee_bps ?? 0);
    editStatus = c.status;
    editReason = '';
  }
  function closeEdit() {
    editTarget = null;
  }

  /** 轮换 Webhook Secret（?/rotateWebhook）：DangerConfirm + 隐藏表单 requestSubmit。 */
  let rotateTarget = $state<MarketplaceClientView | null>(null);
  let rotateReason = $state('');
  let rotateForm: HTMLFormElement | undefined = $state();
  function openRotate(c: MarketplaceClientView) {
    rotateTarget = c;
    rotateReason = '';
  }

  /** 紧急停用（?/emergencyDisable）：DangerConfirm + 隐藏表单 requestSubmit。
   *  触发入口：概览表选中单一商户的页脚按钮 / 详情卡内行按钮。 */
  let emergencyTarget = $state<MarketplaceClientView | null>(null);
  let emergencyReason = $state('');
  let emergencyForm: HTMLFormElement | undefined = $state();
  const selectedEmergency = $derived(
    selectedIds.length === 1 ? (clientById(selectedIds[0]) ?? null) : null
  );
  function openEmergency(c: MarketplaceClientView) {
    emergencyTarget = c;
    emergencyReason = '';
  }

  /** 弹层表单共用结果处理：toast → update → 成功才关弹层（失败留在弹层改）。 */
  const dialogEnhance = (onSuccess: () => void): SubmitFunction =>
    () => async ({ result, update }) => {
      toastActionResult(result);
      await update();
      if (result.type === 'success') onSuccess();
    };

  function exportMarketplaceData() {
    const dataToExport = {
      clients: items,
      offers: data.offers && 'items' in data.offers ? data.offers.items : [],
      deliveries: data.deliveries && 'items' in data.deliveries ? data.deliveries.items : [],
      balances: data.balances ?? [],
      exported_at: new Date().toISOString()
    };
    const blob = new Blob([JSON.stringify(dataToExport, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `marketplace-data-${new Date().toISOString().slice(0, 10)}.json`;
    a.click();
    URL.revokeObjectURL(url);
    showToast('市场数据已导出为 JSON', 'success');
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
  {#if formMessage && !hasJs}
    <div class="alert {isConflict ? 'alert-danger' : 'alert-info'}" role={isConflict ? 'alert' : 'status'} style="margin-bottom:14px;padding:10px 14px;background:var(--color-bg-subtle);border-radius:var(--radius-md);">
      <b>{isConflict ? '版本冲突：' : ''}{formMessage}</b>
    </div>
  {/if}

  {#if formSecret}
    <div class="alert alert-warning" role="status" style="margin-bottom:14px;padding:10px 14px;background:var(--color-warning-soft);border:1px solid var(--color-warning);border-radius:var(--radius-md);">
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
      <MetricGroup items={overviewMetrics} />
    </div>
  </section>

  <!-- 卡片 2：应用 Client（原型同款表格） -->
  <section class="app-card" style="margin-bottom:14px;">
    <header class="app-card__head">
      <h2>应用 Client</h2>
    </header>
    <div class="app-card__body">
      <!-- 工具栏：单行 flex（窄屏自动换行；修复全宽 select 挤压清除按钮的问题） -->
      <div style="display:flex;align-items:center;flex-wrap:wrap;gap:8px;margin-bottom:14px;">
        <input
          type="search"
          bind:value={q}
          class="app-field"
          placeholder="搜索当前列表..."
          aria-label="搜索当前列表"
          style="flex:1 1 220px;min-width:0;"
        />
        <select
          class="app-select"
          bind:value={statusFilter}
          aria-label="状态筛选"
          style="flex:0 0 auto;width:168px;"
        >
          <option value="">全部状态</option>
          <option value="active">已启用</option>
          <option value="disabled">已禁用</option>
        </select>
        {#if q || statusFilter}
          <button type="button" class="btn ghost sm" style="flex:0 0 auto;" onclick={() => { q = ''; statusFilter = ''; }}>
            清除
          </button>
        {/if}
      </div>

      <!-- 批量工具条（约定 B：选中后渲染，按钮打开批量 Dialog） -->
      <BatchBar count={selectedIds.length} noun="个商户" onclear={() => (selectedIds = [])}>
        <Button text="批量设置状态" variant="secondary" size="sm" onclick={openBatchStatus} />
      </BatchBar>

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
              <th>操作</th>
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
                    aria-label="选择 {client.name}"
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
                <td>
                  <!-- 写操作：每行一个「⋮」菜单（约定 D）→ 状态 Dialog（表单在弹层内） -->
                  <RowActionsMenu
                    label="更多操作：商户 {client.name}"
                    actions={clientRowActions(client.id)}
                  />
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>

      <footer class="app-card__foot" style="margin-top:14px;display:flex;align-items:center;justify-content:space-between;flex-wrap:wrap;gap:8px;">
        <div style="display:flex;gap:8px;align-items:center;flex-wrap:wrap;">
          <Button text="开始对账" variant="secondary" size="sm" onclick={openReconcileAll} />
          <Button text="退款重试" variant="secondary" size="sm" onclick={openRetryRefund} />
        </div>
        <div style="display:flex;gap:8px;align-items:center;flex-wrap:wrap;">
          <Button text="导出交易" variant="secondary" size="sm" onclick={exportMarketplaceData} />
          {#if selectedEmergency}
            <Button text="紧急停用已选商户" variant="danger" size="sm" onclick={() => openEmergency(selectedEmergency)} />
          {:else}
            <button type="button" class="btn secondary sm" style="font-weight:700;" disabled title="请先在应用列表中选择一个商户">
              选择商户后停用
            </button>
          {/if}
        </div>
      </footer>
    </div>
  </section>

  <!-- 控制台管理与对账（折叠收纳）：写操作均为按钮 → 弹层 -->
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

        <div style="display:flex;gap:8px;flex-wrap:wrap;">
          <Button text="编辑配置" variant="primary" size="sm" onclick={() => openEdit(c)} />
          <Button text="发起对账" variant="secondary" size="sm" onclick={() => openReconcile(c)} />
          <Button text="轮换 Webhook Secret" variant="ghost" size="sm" onclick={() => openRotate(c)} />
          <Button text="紧急停用" variant="danger" size="sm" onclick={() => openEmergency(c)} />
        </div>
      </div>
    </details>
  {/each}

  <!-- 单行状态切换 Dialog（?/setStatus：If-Match + reason 审计） -->
  <Dialog
    open={statusTarget !== null}
    title="设置 Client 状态"
    description={statusTarget ? `将「${statusTarget.name}」切换到目标状态（If-Match 乐观锁，原因写审计）。` : ''}
    onclose={closeStatus}
  >
    <form
      method="POST"
      action="?/setStatus"
      use:enhance={dialogEnhance(closeStatus)}
    >
      <input type="hidden" name="client_id" value={statusTarget?.id ?? ''} />
      <input type="hidden" name="version" value={String(statusTarget?.version ?? '')} />
      <div class="input-wrapper" style="margin-bottom:var(--space-3);">
        <label class="input-label" for="mk-status">目标状态</label>
        <select id="mk-status" name="status" class="input-field" bind:value={statusValue}>
          <option value="pending">待审批（pending）</option>
          <option value="active">启用（active）</option>
          <option value="disabled">禁用（disabled）</option>
          <option value="emergency_disabled">紧急停用（emergency_disabled，需人工恢复）</option>
        </select>
      </div>
      <div class="input-wrapper" style="margin-bottom:var(--space-3);">
        <label class="input-label" for="mk-status-reason">操作原因（写审计）</label>
        <input id="mk-status-reason" name="reason" class="input-field" required bind:value={statusReason} placeholder="必填" />
      </div>
      <Button text="确认切换" variant="primary" size="sm" type="submit" />
    </form>
  </Dialog>

  <!-- 批量设置状态 Dialog（?/batchSetStatus：ids+versions 循环单条端点） -->
  <Dialog
    open={batchStatusOpen}
    title="批量设置状态"
    description={`将更新 ${selectedIds.length} 个商户的状态（逐条 If-Match 提交，原因写审计）。`}
    onclose={closeBatchStatus}
  >
    <form
      method="POST"
      action="?/batchSetStatus"
      use:enhance={() => async ({ result, update }) => {
        toastActionResult(result);
        await update();
        if (result.type === 'success') {
          selectedIds = [];
          closeBatchStatus();
        }
      }}
    >
      <input type="hidden" name="ids" value={selectedIds.join(',')} />
      <input
        type="hidden"
        name="versions"
        value={selectedIds.map((id) => String(clientById(id)?.version ?? '')).join(',')}
      />
      <div class="input-wrapper" style="margin-bottom:var(--space-3);">
        <label class="input-label" for="mk-batch-status">目标状态</label>
        <select id="mk-batch-status" name="status" class="input-field" bind:value={batchStatusValue} required>
          <option value="" disabled>请选择状态</option>
          <option value="pending">待审批（pending）</option>
          <option value="active">启用（active）</option>
          <option value="disabled">禁用（disabled）</option>
        </select>
      </div>
      <div class="input-wrapper" style="margin-bottom:var(--space-3);">
        <label class="input-label" for="mk-batch-reason">操作原因（写审计）</label>
        <input id="mk-batch-reason" name="reason" class="input-field" required bind:value={batchReason} placeholder="必填" />
      </div>
      <Button text={`批量更新 ${selectedIds.length} 项`} variant="primary" size="sm" type="submit" />
    </form>
  </Dialog>

  <!-- 全站对账 Dialog（?/runReconciliationAll）：说明影响 + reason -->
  <Dialog
    open={reconcileAllOpen}
    title="发起全站对账"
    description="将对全部商户执行增量对账（恒等式校验），结果写入审计与 Webhook 投递记录；交易量大时可能持续数分钟。"
    onclose={closeReconcileAll}
  >
    <form
      method="POST"
      action="?/runReconciliationAll"
      use:enhance={dialogEnhance(closeReconcileAll)}
    >
      <div class="input-wrapper" style="margin-bottom:var(--space-3);">
        <label class="input-label" for="mk-recon-all-reason">操作原因（写审计）</label>
        <input id="mk-recon-all-reason" name="reason" class="input-field" required bind:value={reconcileAllReason} placeholder="必填" />
      </div>
      <Button text="确认对账" variant="primary" size="sm" type="submit" />
    </form>
  </Dialog>

  <!-- 单商户对账 Dialog（?/runReconciliation） -->
  <Dialog
    open={reconcileTarget !== null}
    title="发起商户对账"
    description={reconcileTarget ? `对「${reconcileTarget.name}」执行增量对账（恒等式校验），结果写入审计。` : ''}
    onclose={closeReconcile}
  >
    <form
      method="POST"
      action="?/runReconciliation"
      use:enhance={dialogEnhance(closeReconcile)}
    >
      <input type="hidden" name="client_id" value={reconcileTarget?.id ?? ''} />
      <input type="hidden" name="after_cursor" value="0" />
      <div class="input-wrapper" style="margin-bottom:var(--space-3);">
        <label class="input-label" for="mk-recon-reason">对账原因（写审计）</label>
        <input id="mk-recon-reason" name="reason" class="input-field" required bind:value={reconcileReason} placeholder="必填" />
      </div>
      <Button text="确认对账" variant="primary" size="sm" type="submit" />
    </form>
  </Dialog>

  <!-- 退款重试 Dialog（?/retryRefund）：处理 requested 态退款 -->
  <Dialog
    open={retryRefundOpen}
    title="退款重试"
    description="对处于 requested 状态的退款单重新提交处理；请输入退款单 ID，处理结果写入审计。"
    onclose={closeRetryRefund}
  >
    <form
      method="POST"
      action="?/retryRefund"
      use:enhance={dialogEnhance(closeRetryRefund)}
    >
      <div class="input-wrapper" style="margin-bottom:var(--space-3);">
        <label class="input-label" for="mk-refund-id">退款单 ID</label>
        <input id="mk-refund-id" name="refund_id" class="input-field" required bind:value={refundIdInput} placeholder="refund id" />
      </div>
      <div class="input-wrapper" style="margin-bottom:var(--space-3);">
        <label class="input-label" for="mk-refund-reason">操作原因（写审计）</label>
        <input id="mk-refund-reason" name="reason" class="input-field" required bind:value={refundReason} placeholder="必填" />
      </div>
      <Button text="确认重试" variant="primary" size="sm" type="submit" />
    </form>
  </Dialog>

  <!-- 编辑 Client 配置 Dialog（?/upsertClient：If-Match + reason 审计） -->
  <Dialog
    open={editTarget !== null}
    title="编辑 Client 配置"
    description={editTarget ? `更新「${editTarget.name}」的商户资料与状态（If-Match 乐观锁，原因写审计）。` : ''}
    onclose={closeEdit}
  >
    <form
      method="POST"
      action="?/upsertClient"
      use:enhance={dialogEnhance(closeEdit)}
    >
      <input type="hidden" name="client_id" value={editTarget?.id ?? ''} />
      <input type="hidden" name="id" value={editTarget?.id ?? ''} />
      <input type="hidden" name="version" value={String(editTarget?.version ?? '')} />
      <div style="display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:var(--space-3);">
        <div class="input-wrapper">
          <label class="input-label" for="mk-edit-name">名称</label>
          <input id="mk-edit-name" name="name" class="input-field" bind:value={editName} />
        </div>
        <div class="input-wrapper">
          <label class="input-label" for="mk-edit-owner">Owner 用户 ID</label>
          <input id="mk-edit-owner" name="owner_user_id" class="input-field" bind:value={editOwner} />
        </div>
        <div class="input-wrapper">
          <label class="input-label" for="mk-edit-terms">条款 URL</label>
          <input id="mk-edit-terms" name="terms_url" class="input-field" bind:value={editTerms} />
        </div>
        <div class="input-wrapper">
          <label class="input-label" for="mk-edit-privacy">隐私 URL</label>
          <input id="mk-edit-privacy" name="privacy_url" class="input-field" bind:value={editPrivacy} />
        </div>
        <div class="input-wrapper">
          <label class="input-label" for="mk-edit-webhook">Webhook URL</label>
          <input id="mk-edit-webhook" name="webhook_url" class="input-field" bind:value={editWebhook} />
        </div>
        <div class="input-wrapper">
          <label class="input-label" for="mk-edit-fee">手续费（bps）</label>
          <input id="mk-edit-fee" name="fee_bps" type="number" min="0" class="input-field" bind:value={editFeeBps} />
        </div>
        <div class="input-wrapper" style="grid-column:1 / -1;">
          <label class="input-label" for="mk-edit-redirects">回调地址（每行一个）</label>
          <textarea id="mk-edit-redirects" name="redirect_uris" class="input-field" rows="2" bind:value={editRedirects}></textarea>
        </div>
        <div class="input-wrapper">
          <label class="input-label" for="mk-edit-status">状态</label>
          <select id="mk-edit-status" name="status" class="input-field" bind:value={editStatus}>
            <option value="pending">待审批（pending）</option>
            <option value="active">启用（active）</option>
            <option value="disabled">禁用（disabled）</option>
          </select>
        </div>
        <div class="input-wrapper">
          <label class="input-label" for="mk-edit-reason">操作原因（必填，写审计）</label>
          <input id="mk-edit-reason" name="reason" class="input-field" required bind:value={editReason} placeholder="必填" />
        </div>
      </div>
      <div style="margin-top:var(--space-3);">
        <Button text="保存设置" variant="primary" size="sm" type="submit" />
      </div>
    </form>
  </Dialog>
{/if}

<!-- 轮换 Webhook Secret：DangerConfirm + 隐藏表单 requestSubmit（reason 写审计）。 -->
{#if clients.state !== 'forbidden'}
  <form
    method="POST"
    action="?/rotateWebhook"
    bind:this={rotateForm}
    use:enhance={() => async ({ result, update }) => {
      toastActionResult(result);
      await update();
      rotateTarget = null;
    }}
  >
    <input type="hidden" name="client_id" value={rotateTarget?.id ?? ''} />
    <input type="hidden" name="reason" value={rotateReason} />
  </form>

  <DangerConfirm
    open={rotateTarget !== null}
    title="轮换 Webhook Secret"
    description={rotateTarget ? `确认轮换「${rotateTarget.name}」的 Webhook Secret？旧密钥立即失效，需在商户侧同步更新。` : ''}
    confirmText="确认轮换"
    oncancel={() => (rotateTarget = null)}
    onconfirm={() => rotateForm?.requestSubmit()}
  >
    <label class="input-label" for="mk-rotate-reason">操作原因（写审计）</label>
    <input id="mk-rotate-reason" class="input-field" bind:value={rotateReason} placeholder="必填" required />
  </DangerConfirm>

  <!-- 紧急停用：DangerConfirm + 隐藏表单 requestSubmit（If-Match + reason 审计）。 -->
  <form
    method="POST"
    action="?/emergencyDisable"
    bind:this={emergencyForm}
    use:enhance={() => async ({ result, update }) => {
      toastActionResult(result);
      await update();
      emergencyTarget = null;
    }}
  >
    <input type="hidden" name="client_id" value={emergencyTarget?.id ?? ''} />
    <input type="hidden" name="version" value={String(emergencyTarget?.version ?? '')} />
    <input type="hidden" name="reason" value={emergencyReason} />
  </form>

  <DangerConfirm
    open={emergencyTarget !== null}
    title="紧急停用 Client"
    description={emergencyTarget ? `确认紧急停用「${emergencyTarget.name}」？该商户将立即停止服务，需人工恢复。` : ''}
    confirmText="确认紧急停用"
    oncancel={() => (emergencyTarget = null)}
    onconfirm={() => emergencyForm?.requestSubmit()}
  >
    <label class="input-label" for="mk-emergency-reason">停用原因（写审计）</label>
    <input id="mk-emergency-reason" class="input-field" bind:value={emergencyReason} placeholder="必填" required />
  </DangerConfirm>

  <!-- step-up 重新验证（M02-MFA-07）：高敏操作命中 403 step_up_required 时展示。
       无 JS 时 Dialog 以固定层内联渲染，表单仍可用（SSR 基线保留）。 -->
  <Dialog
    open={Boolean(form?.stepUpRequired) && !reauthCancelled}
    title="需要重新验证身份"
    description="市场与客户端配置属于高风险管理操作，要求近期重新认证。输入当前账号密码完成重新验证后，可继续刚才的操作。"
    onclose={() => (reauthCancelled = true)}
  >
    {#if reauthError}
      <div class="alert alert-danger" role="alert" style="margin-bottom:10px;padding:8px 12px;font-size:12px;">
        {reauthError}
      </div>
    {/if}
    <form
      method="POST"
      action="?/reauth"
      use:enhance={() => {
        reauthLoading = true;
        reauthError = null;
        return async ({ result, update }) => {
          reauthLoading = false;
          if (result.type === 'failure') {
            reauthError = (result.data as unknown as AdminMarketplaceActionData | null)?.message ?? '密码验证失败，请重试';
            return;
          }
          toastActionResult(result);
          await update();
        };
      }}
      style="display:flex;flex-direction:column;gap:10px;"
    >
      <div>
        <label class="input-label" for="mk-reauth-password">当前账号密码</label>
        <input class="input-field" type="password" id="mk-reauth-password" name="password" autocomplete="current-password" required />
      </div>
      <div style="display:flex;gap:8px;">
        <Button text={reauthLoading ? '验证中…' : '重新验证'} variant="primary" type="submit" disabled={reauthLoading} />
        <button type="button" class="btn ghost sm" onclick={() => (reauthCancelled = true)}>取消</button>
      </div>
    </form>
  </Dialog>
{/if}
