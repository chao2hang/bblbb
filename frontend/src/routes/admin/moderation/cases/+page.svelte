<script lang="ts">
  // M18-ADMIN-REPORTS：举报案件队列（表格列表布局）。
  // M18-ADMIN-BATCH：原「标记处理中/批量关闭/批量驳回」死按钮接通——
  // BatchBar + 两个批量 Dialog + 两个 DangerConfirm（关闭/驳回需原因写审计），
  // 服务端循环调用案件状态更新端点（与详情页 transition 同端点）。
  import { page } from '$app/state';
  import { enhance } from '$app/forms';
  import { goto } from '$app/navigation';
  import BatchBar from '$lib/components/admin/BatchBar.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import DangerConfirm from '$lib/components/ui/DangerConfirm.svelte';
  import Dialog from '$lib/components/ui/Dialog.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import TablePagination from '$lib/components/admin/TablePagination.svelte';
  import { formatRelative } from '$lib/utils';
  import { toastActionResult } from '$lib/ui/action-toast';
  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();

  function getStatus(): string {
    try {
      return page.url.searchParams.get('status') ?? '';
    } catch {
      return '';
    }
  }

  const currentStatus = $derived(getStatus());

  // P0 整改：只展示服务端返回的案件；空列表 = 真实空态，403/错误渲染提示。
  const allCases = $derived(data.items ?? []);

  const counts = $derived({
    all: allCases.length,
    open: allCases.filter((c) => c.status === 'open').length,
    triaged: allCases.filter((c) => c.status === 'triaged' || c.status === 'investigating').length,
    resolved: allCases.filter((c) => c.status === 'resolved').length,
    rejected: allCases.filter((c) => c.status === 'rejected').length
  });

  const statusTabs = $derived([
    { key: '', label: '全部', count: counts.all },
    { key: 'open', label: '待处理', count: counts.open },
    { key: 'triaged', label: '处理中', count: counts.triaged },
    { key: 'resolved', label: '已处理', count: counts.resolved },
    { key: 'rejected', label: '已驳回', count: counts.rejected }
  ]);

  const displayedCases = $derived.by(() => {
    if (!currentStatus) return allCases;
    if (currentStatus === 'triaged') {
      return allCases.filter((c) => c.status === 'triaged' || c.status === 'investigating');
    }
    return allCases.filter((c) => c.status === currentStatus);
  });

  let currentPage = $state(1);
  let pageSize = $state(10);

  $effect(() => {
    void currentStatus;
    currentPage = 1;
  });

  const pagedCases = $derived(
    displayedCases.slice((currentPage - 1) * pageSize, currentPage * pageSize)
  );

  let selectedIds = $state<string[]>([]);
  let allSelected = $derived(
    pagedCases.length > 0 && pagedCases.every((i) => selectedIds.includes(i.id))
  );
  function toggleAll() {
    if (allSelected) {
      const pagedSet = new Set(pagedCases.map((i) => i.id));
      selectedIds = selectedIds.filter((id) => !pagedSet.has(id));
    } else {
      const set = new Set([...selectedIds, ...pagedCases.map((i) => i.id)]);
      selectedIds = Array.from(set);
    }
  }
  function toggleRow(id: string) {
    if (selectedIds.includes(id)) selectedIds = selectedIds.filter((x) => x !== id);
    else selectedIds = [...selectedIds, id];
  }

  function handleRowClick(e: MouseEvent, id: string) {
    const target = e.target as HTMLElement | null;
    if (target?.closest('input, button, a, label')) return;
    goto(`/admin/moderation/cases/${id}`);
  }

  // ── 批量操作弹层（M18-ADMIN-BATCH） ──
  // 标记处理中：Dialog 内选择目标状态（triaged/investigating）。
  let batchStatusOpen = $state(false);
  let batchStatus = $state('triaged');

  // 批量关闭 / 批量驳回：DangerConfirm + reason（写审计），确认后提交隐藏表单。
  let isSubmitting = $state(false);
  let batchCloseOpen = $state(false);
  let batchCloseReason = $state('');
  let batchCloseError = $state('');
  let batchCloseForm: HTMLFormElement | undefined = $state();

  let batchRejectOpen = $state(false);
  let batchRejectReason = $state('');
  let batchRejectError = $state('');
  let batchRejectForm: HTMLFormElement | undefined = $state();

  function clearSelection(): void {
    selectedIds = [];
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

<!-- 统一工作台 Tab -->
<div class="tabs" role="tablist" aria-label="审核工作台" style="margin-bottom:12px;background:var(--color-bg-subtle);border-radius:var(--radius-sm);padding:2px;display:inline-flex;">
  <a
    role="tab"
    aria-selected={false}
    href="/admin/moderation?tab=content"
    class="tab"
    style="padding:6px 14px;font-size:13px;text-decoration:none;"
  >
    待发内容审核
  </a>
  <a
    role="tab"
    aria-selected={true}
    href="/admin/moderation/cases"
    class="tab is-active"
    style="padding:6px 14px;font-size:13px;text-decoration:none;"
  >
    用户举报案件 {allCases.length > 0 ? `(${allCases.length})` : ''}
  </a>
</div>

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

<!-- P0 整改：403/接口错误必须优先渲染，禁止被空列表/Mock 掩盖 -->
{#if data.forbidden}
  <section class="app-card" style="margin-bottom:14px;">
    <div class="app-card__body">
      <p class="input-hint is-error" role="alert">没有审核权限（moderation.review required）。</p>
    </div>
  </section>
{:else if data.error}
  <section class="app-card" style="margin-bottom:14px;">
    <div class="app-card__body">
      <p class="input-hint is-error" role="alert">加载失败：{data.error}</p>
    </div>
  </section>
{/if}

<!-- 批量工具条（选中 > 0 时渲染） -->
<BatchBar count={selectedIds.length} noun="个案件" onclear={clearSelection}>
  <Button text="标记处理中" variant="secondary" size="sm" onclick={() => (batchStatusOpen = true)} />
  <Button text="批量关闭" variant="danger" size="sm" onclick={() => (batchCloseOpen = true)} />
  <Button text="批量驳回" variant="danger" size="sm" onclick={() => (batchRejectOpen = true)} />
</BatchBar>

<!-- 案件表格列表 -->
{#if displayedCases.length === 0}
  <div class="app-card">
    <div class="app-card__body">
      <EmptyState icon="inbox" title="暂无案件" desc="当前筛选下没有待处理的案件" />
    </div>
  </div>
{:else}
  <section class="app-card">
    <header class="app-card__head" style="display:flex;align-items:center;justify-content:space-between;padding:14px 16px;border-bottom:1px solid var(--color-border);">
      <div style="display:flex;align-items:center;gap:10px;">
        <h2 style="font-size:15px;margin:0;font-weight:600;">案件列表</h2>
        <span class="app-muted" style="font-size:12px;">共 {displayedCases.length} 个案件{#if selectedIds.length > 0} · 已选 {selectedIds.length} 项{/if}</span>
      </div>
    </header>
    <div class="app-card__body" style="padding:0;">
      <div class="app-table-wrap">
        <table class="app-table moderation-table" aria-label="案件列表">
          <thead>
            <tr>
              <th class="th-select" style="width:40px;text-align:center;">
                <input
                  type="checkbox"
                  checked={allSelected}
                  onchange={toggleAll}
                  aria-label="全选当前列表"
                  title="全选当前列表"
                />
              </th>
              <th style="width:140px;">案件编号</th>
              <th style="min-width:260px;">案件标题</th>
              <th style="width:100px;">优先级</th>
              <th style="width:90px;">状态</th>
              <th style="width:140px;">负责人</th>
              <th style="width:110px;">提交时间</th>
              <th style="width:110px;text-align:right;">操作</th>
            </tr>
          </thead>
          <tbody>
            {#each pagedCases as item (item.id)}
              {@const p = priorityBadge(item.priority)}
              {@const s = statusBadge(item.status)}
              <tr
                class="case-row"
                class:is-selected={selectedIds.includes(item.id)}
                onclick={(e) => handleRowClick(e, item.id)}
              >
                <td class="td-select" style="text-align:center;">
                  <input
                    type="checkbox"
                    checked={selectedIds.includes(item.id)}
                    onchange={() => toggleRow(item.id)}
                    aria-label="选择案件 {item.id}"
                  />
                </td>
                <td>
                  <a
                    href="/admin/moderation/cases/{item.id}"
                    class="text-link"
                    style="font-family:var(--font-mono, monospace);font-size:12px;font-weight:600;"
                    title={item.id}
                  >
                    {item.id.length > 16 ? item.id.slice(0, 8) + '…' + item.id.slice(-4) : item.id}
                  </a>
                </td>
                <td>
                  <a
                    href="/admin/moderation/cases/{item.id}"
                    class="text-link"
                    style="font-weight:600;font-size:13px;line-height:1.4;color:var(--color-text-primary);display:inline-block;"
                  >
                    {item.title || '（无标题案件）'}
                  </a>
                </td>
                <td>
                  <span class={p.cls} style="padding:2px 6px;border-radius:4px;font-size:11px;">{p.label}</span>
                </td>
                <td>
                  <span class={s.cls} style="padding:2px 6px;border-radius:4px;font-size:11px;">{s.label}</span>
                </td>
                <td>
                  <span
                    class="text-secondary"
                    style="font-size:12px;"
                    title={item.assigned_to ?? '未指派'}
                  >
                    {#if !item.assigned_to}
                      未指派
                    {:else if item.assigned_to.length > 16}
                      {item.assigned_to.slice(0, 8)}…{item.assigned_to.slice(-4)}
                    {:else}
                      {item.assigned_to}
                    {/if}
                  </span>
                </td>
                <td>
                  <span class="text-secondary" style="font-size:12px;white-space:nowrap;">
                    {formatRelative(item.created_at)}
                  </span>
                </td>
                <td style="text-align:right;">
                  <a
                    href="/admin/moderation/cases/{item.id}"
                    class="btn secondary sm"
                    style="text-decoration:none;font-size:12px;white-space:nowrap;"
                  >
                    进入管理
                  </a>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </div>
    <TablePagination
      bind:currentPage
      bind:pageSize
      totalItems={displayedCases.length}
      noun="个案件"
    />
  </section>
{/if}

<!-- 批量标记处理中：Dialog 内选目标状态（端点不要求原因） → POST batchStatus。 -->
<Dialog
  open={batchStatusOpen}
  title="批量标记处理中"
  description={`将把 ${selectedIds.length} 个案件迁移至所选状态；状态迁移由后端审计。`}
  onclose={() => (batchStatusOpen = false)}
>
  <form
    method="POST"
    action="?/batchStatus"
    use:enhance={() => {
      return async ({ result, update }) => {
        toastActionResult(result, {
          message: (d) => (d?.message as string | null) ?? (result.type === 'success' ? '批量标记完成' : '批量标记失败')
        });
        await update();
        clearSelection();
        batchStatusOpen = false;
      };
    }}
  >
    <input type="hidden" name="ids" value={selectedIds.join(',')} />
    <div class="input-wrapper" style="margin-bottom:var(--space-3);">
      <label class="input-label" for="batch-case-status">目标状态</label>
      <select id="batch-case-status" name="status" class="input-field" bind:value={batchStatus}>
        <option value="triaged">处理中（已分类，待调查）</option>
        <option value="investigating">调查中</option>
      </select>
    </div>
    <Button text="确认批量标记" variant="secondary" size="sm" type="submit" />
  </form>
</Dialog>

<!-- 批量关闭：DangerConfirm + reason（写审计），确认后提交隐藏表单。
     隐藏表单常驻 DOM（ids/reason 审计要素）。 -->
<form
  method="POST"
  action="?/batchClose"
  bind:this={batchCloseForm}
  use:enhance={() => {
    isSubmitting = true;
    return async ({ result, update }) => {
      isSubmitting = false;
      toastActionResult(result, {
        message: (d) => (d?.message as string | null) ?? (result.type === 'success' ? '批量关闭完成' : '批量关闭失败')
      });
      await update();
      if (result.type === 'success') {
        clearSelection();
        batchCloseOpen = false;
        batchCloseReason = '';
      }
    };
  }}
>
  <input type="hidden" name="ids" value={selectedIds.join(',')} />
  <input type="hidden" name="reason" value={batchCloseReason} />
</form>

<DangerConfirm
  open={batchCloseOpen}
  title="批量关闭案件"
  description={`将把 ${selectedIds.length} 个案件标记为已处理（resolved）；原因作为处理结论存档并写审计。`}
  confirmText="确认批量关闭"
  busy={isSubmitting}
  error={batchCloseError}
  oncancel={() => {
    batchCloseOpen = false;
    batchCloseError = '';
  }}
  onconfirm={() => {
    if (!batchCloseReason.trim()) {
      batchCloseError = '关闭原因必填（写审计）';
      return;
    }
    batchCloseError = '';
    batchCloseForm?.requestSubmit();
  }}
>
  <label class="input-label" for="batch-close-reason">关闭原因（写审计）</label>
  <input id="batch-close-reason" class="input-field" bind:value={batchCloseReason} placeholder="必填" required />
</DangerConfirm>

<!-- 批量驳回：DangerConfirm + reason（写审计），确认后提交隐藏表单。 -->
<form
  method="POST"
  action="?/batchReject"
  bind:this={batchRejectForm}
  use:enhance={() => {
    isSubmitting = true;
    return async ({ result, update }) => {
      isSubmitting = false;
      toastActionResult(result, {
        message: (d) => (d?.message as string | null) ?? (result.type === 'success' ? '批量驳回完成' : '批量驳回失败')
      });
      await update();
      if (result.type === 'success') {
        clearSelection();
        batchRejectOpen = false;
        batchRejectReason = '';
      }
    };
  }}
>
  <input type="hidden" name="ids" value={selectedIds.join(',')} />
  <input type="hidden" name="reason" value={batchRejectReason} />
</form>

<DangerConfirm
  open={batchRejectOpen}
  title="批量驳回案件"
  description={`将把 ${selectedIds.length} 个案件标记为已驳回（rejected）；原因作为处理结论存档并写审计。`}
  confirmText="确认批量驳回"
  busy={isSubmitting}
  error={batchRejectError}
  oncancel={() => {
    batchRejectOpen = false;
    batchRejectError = '';
  }}
  onconfirm={() => {
    if (!batchRejectReason.trim()) {
      batchRejectError = '驳回原因必填（写审计）';
      return;
    }
    batchRejectError = '';
    batchRejectForm?.requestSubmit();
  }}
>
  <label class="input-label" for="batch-reject-reason">驳回原因（写审计）</label>
  <input id="batch-reject-reason" class="input-field" bind:value={batchRejectReason} placeholder="必填" required />
</DangerConfirm>

<style>
  .case-row {
    cursor: pointer;
    transition: background-color 0.12s ease;
  }
  .case-row:hover td {
    background: var(--color-bg-subtle, rgba(0, 0, 0, 0.02));
  }
  .case-row.is-selected td {
    background: var(--color-brand-soft, rgba(46, 117, 246, 0.08)) !important;
  }
</style>
