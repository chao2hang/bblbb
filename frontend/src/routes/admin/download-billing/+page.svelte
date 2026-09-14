<script lang="ts">
  // M18-ADMIN-DOWNLOAD-BILLING：下载计费管理页（对齐原型 #admin-download-billing）。
  // M18-ADMIN-BATCH：常驻「保存配置」表单收敛为页头按钮 + Dialog（?/save）；
  // 下载记录为计费流水（download_authorizations，无删除/退款端点），按规范
  // 移除无对应端点的选择列与「已选」条，避免死 UI。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { enhance } from '$app/forms';
  import { invalidateAll } from '$app/navigation';
  import { untrack } from 'svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import Dialog from '$lib/components/ui/Dialog.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { show as showToast } from '$lib/ui/toast';
  import type { AdminDownloadBillingActionData, AdminDownloadBillingPageData } from './+page.server';

  let { data, form }: { data: AdminDownloadBillingPageData; form?: AdminDownloadBillingActionData } = $props();

  const message = $derived(form?.message ?? null);

  let isEnabled = $state(untrack(() => data.config?.is_enabled ?? true));
  let amount = $state(untrack(() => data.config?.amount ?? 10));
  let ttl = $state(untrack(() => String(data.config?.authorization_ttl_seconds ?? 86400)));
  let dailyLimit = $state(untrack(() => data.config?.daily_user_limit ?? 20));
  let dismissBillingMsg = $state(false);

  // ── 编辑计费策略（Dialog 版）：打开时以当前配置预填（行/配置状态预选）。 ──
  let configOpen = $state(false);

  function openConfig(): void {
    isEnabled = data.config?.is_enabled ?? true;
    amount = data.config?.amount ?? 10;
    ttl = String(data.config?.authorization_ttl_seconds ?? 86400);
    dailyLimit = data.config?.daily_user_limit ?? 20;
    configOpen = true;
  }

  $effect(() => {
    if (form?.input) {
      if (form.input.is_enabled !== undefined) isEnabled = Boolean(form.input.is_enabled);
      if (form.input.amount !== undefined) amount = Number(form.input.amount);
      if (form.input.authorization_ttl_seconds !== undefined) ttl = String(form.input.authorization_ttl_seconds);
      if (form.input.daily_user_limit !== undefined) dailyLimit = Number(form.input.daily_user_limit);
    } else if (form?.config) {
      isEnabled = form.config.is_enabled ?? true;
      amount = form.config.amount ?? 10;
      ttl = String(form.config.authorization_ttl_seconds ?? 86400);
      dailyLimit = form.config.daily_user_limit ?? 20;
    }
  });


  const recordsList = $derived.by(() => {
    const raw = data.transactions?.items;
    if (Array.isArray(raw) && raw.length > 0) {
      return raw.map((r) => {
        const amount = typeof (r as any).amount === 'number' ? (r as any).amount : 0;
        const status = (r as any).status || (amount > 0 ? 'success' : amount < 0 ? 'refunded' : 'free');
        return {
          id: r.id,
          filename: r.filename,
          user: r.username,
          amount,
          status,
          size: `${amount} 积分`,
          created_at: r.created_at
        };
      });
    }
    // P0 整改：无交易渲染真实空态，不回退演示账单。
    return [];
  });

  let q = $state('');
  let statusFilter = $state('');
  let refreshing = $state(false);

  const displayedRecords = $derived.by(() => {
    let list = recordsList;
    if (q.trim()) {
      const kw = q.trim().toLowerCase();
      list = list.filter((r) => r.filename.toLowerCase().includes(kw) || r.user.toLowerCase().includes(kw) || r.id.toLowerCase().includes(kw));
    }
    if (statusFilter) {
      list = list.filter((r) => r.status === statusFilter);
    }
    return list;
  });

  const totalAmount = $derived(
    displayedRecords.reduce((sum, r) => sum + (r.amount || 0), 0)
  );

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

  /** 授权有效期选项反查文案（配置摘要展示用）。 */
  function ttlLabel(seconds: number | null | undefined): string {
    switch (seconds) {
      case 3600: return '1 小时';
      case 21600: return '6 小时';
      case 604800: return '7 天';
      case 86400: return '24 小时';
      default: return seconds != null ? `${seconds} 秒` : '—';
    }
  }
</script>

<svelte:head>
  <title>下载计费 — BBLBB Admin</title>
</svelte:head>

<PageHeader title="下载计费" />

<!-- 卡片 1：计费策略（常驻表单已收敛为摘要 + 编辑按钮 → Dialog） -->
<section class="app-card" style="margin-bottom:14px;">
  <header class="app-card__head" style="display:flex;align-items:center;justify-content:space-between;">
    <h2>计费策略</h2>
    {#if data.state === 'ok'}
      <Button text="编辑计费策略" variant="primary" size="sm" onclick={openConfig} />
    {/if}
  </header>
  <div class="app-card__body">
    {#if message && !dismissBillingMsg}
      <div
        class="alert {form?.error ? 'alert-danger' : 'alert-success'}"
        role="status"
        style="margin-bottom:12px;padding:10px 14px;background:var(--color-bg-subtle);border-radius:var(--radius-sm);font-size:13px;display:flex;justify-content:space-between;align-items:center;"
      >
        <div>
          <span>{message}</span>
          {#if form?.code}
            <span class="app-muted" style="font-size:11px;margin-left:6px;">({form.code})</span>
          {/if}
        </div>
        <div style="display:flex;gap:6px;">
          <button type="button" class="btn ghost sm" onclick={() => (dismissBillingMsg = true)}>关闭</button>
        </div>
      </div>
    {/if}
    {#if data.state === 'ok'}
      <div style="display:flex;gap:18px;flex-wrap:wrap;font-size:13px;align-items:center;">
        <span>
          状态：
          {#if data.config?.is_enabled}
            <span class="badge badge-success">已开启</span>
          {:else if data.config}
            <span class="badge badge-neutral">已关闭（全部免费）</span>
          {:else}
            <span class="text-secondary">未加载到当前配置</span>
          {/if}
        </span>
        <span class="text-secondary">默认下载价格：<b style="color:var(--color-text-primary);">{data.config?.amount ?? '—'}</b> 积分</span>
        <span class="text-secondary">授权有效期：<b style="color:var(--color-text-primary);">{ttlLabel(data.config?.authorization_ttl_seconds)}</b></span>
        <span class="text-secondary">单用户日限额：<b style="color:var(--color-text-primary);">{data.config?.daily_user_limit ?? '—'}</b> 次</span>
      </div>
    {:else}
      <p class="input-hint is-error" role="alert">
        <Icon name="lock" size={14} />
        {data.error || '计费策略接口不可用'}
      </p>
    {/if}
  </div>
</section>

<!-- 编辑计费策略：Dialog 内表单（mode/reason 审计隐藏域保留 → PATCH config）。 -->
<Dialog
  open={configOpen}
  title="编辑计费策略"
  description="保存后立即生效；变更原因写入审计日志。"
  onclose={() => (configOpen = false)}
>
  <form
    method="POST"
    action="?/save"
    use:enhance={() => {
      dismissBillingMsg = false;
      return async ({ result, update }) => {
        await update();
        if (result.type === 'success') {
          showToast('计费策略已保存', 'success');
          configOpen = false;
        } else if (result.type === 'failure') {
          showToast((result.data as any)?.message ?? '保存失败，请检查参数', 'danger');
        }
      };
    }}
    style="display:flex;flex-direction:column;gap:14px;"
  >
    <input type="hidden" name="mode" value="fixed" />
    <input type="hidden" name="reason" value="管理员在后台更新下载计费策略" />

    <label style="display:flex;align-items:center;gap:8px;font-size:13px;font-weight:600;cursor:pointer;">
      <input type="checkbox" name="is_enabled" bind:checked={isEnabled} />
      开启下载计费（关闭后全部免费）
    </label>

    <div>
      <label class="input-label" for="db-amount" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">
        默认下载价格（积分）
      </label>
      <input type="number" class="input-field" id="db-amount" name="amount" bind:value={amount} min="0" step="1" style="width:100%;" />
    </div>

    <div>
      <label class="input-label" for="db-ttl" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">
        授权有效期
      </label>
      <select class="app-select" id="db-ttl" name="authorization_ttl_seconds" bind:value={ttl} style="width:100%;">
        <option value="3600">1 小时</option>
        <option value="21600">6 小时</option>
        <option value="86400">24 小时</option>
        <option value="604800">7 天</option>
      </select>
    </div>

    <div>
      <label class="input-label" for="db-daily" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">
        单用户日限额（次）
      </label>
      <input type="number" class="input-field" id="db-daily" name="daily_user_limit" bind:value={dailyLimit} min="0" step="1" style="width:100%;" />
    </div>

    <div style="margin-top:6px;">
      <button type="submit" class="btn primary sm">保存计费策略</button>
    </div>
  </form>
</Dialog>

<!-- 卡片 2：下载记录（原型同款表格；计费流水无批量端点，不提供选择列） -->
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
          <option value="refunded">已退款</option>
          <option value="failed">失败</option>
        </select>
        {#if q || statusFilter}
          <button type="button" class="btn ghost sm" onclick={() => { q = ''; statusFilter = ''; }}>
            清除
          </button>
        {/if}
      </div>
    </div>

    <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:10px;">
      <span class="app-muted" style="font-size:12px;">共 {displayedRecords.length} 笔流水，合计金额：<b>{totalAmount} 积分</b></span>
    </div>

    <div class="app-table-wrap">
      <table class="app-table" aria-label="下载记录">
        <thead>
          <tr>
            <th>单号</th>
            <th>文件</th>
            <th>用户</th>
            <th style="width:100px;">状态</th>
            <th>扣费金额</th>
          </tr>
        </thead>
        <tbody>
          {#if displayedRecords.length === 0}
            <tr>
              <td colspan="5" style="text-align:center;padding:24px;color:var(--color-text-secondary);">
                当前筛选下没有计费记录
              </td>
            </tr>
          {:else}
            {#each displayedRecords as record (record.id)}
              <tr>
                <td>
                  <code style="padding:2px 6px;background:var(--color-bg-subtle);border-radius:3px;">{record.id}</code>
                </td>
                <td>{record.filename}</td>
                <td>{record.user}</td>
                <td>
                  <span class="sbadge {record.status === 'success' ? 'sb-success' : record.status === 'free' ? 'sb-brand' : record.status === 'refunded' ? 'sb-warning' : 'sb-gray'}">
                    {record.status === 'success' ? '扣费成功' : record.status === 'free' ? '免费放行' : record.status === 'refunded' ? '已退款' : '失败'}
                  </span>
                </td>
                <td><b>{record.size}</b></td>
              </tr>
            {/each}
          {/if}
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
