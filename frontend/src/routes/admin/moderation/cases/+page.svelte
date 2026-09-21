<script lang="ts">
  // M18-ADMIN-REPORTS：举报案件队列（对齐原型 #admin-reports 布局与交互）。
  // M18-ADMIN-BATCH：原「标记处理中/批量关闭/批量驳回」死按钮接通——
  // BatchBar + 两个批量 Dialog + 两个 DangerConfirm（关闭/驳回需原因写审计），
  // 服务端循环调用案件状态更新端点（与详情页 transition 同端点）。
  import { page } from '$app/state';
  import { enhance } from '$app/forms';
  import BatchBar from '$lib/components/admin/BatchBar.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import DangerConfirm from '$lib/components/ui/DangerConfirm.svelte';
  import Dialog from '$lib/components/ui/Dialog.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
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

  let selectedIds = $state<string[]>([]);
  let allSelected = $derived(
    displayedCases.length > 0 && selectedIds.length === displayedCases.length
  );
  function toggleAll() {
    if (allSelected) selectedIds = [];
    else selectedIds = displayedCases.map((i) => i.id);
  }
  function toggleRow(id: string) {
    if (selectedIds.includes(id)) selectedIds = selectedIds.filter((x) => x !== id);
    else selectedIds = [...selectedIds, id];
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

<!-- 批量操作卡片（原型同款；死按钮已接通为 BatchBar + 批量弹层） -->
<section class="app-card" style="margin-bottom:14px;">
  <div class="app-card__body" style="padding:14px 16px;">
    <div style="display:flex;align-items:center;justify-content:space-between;flex-wrap:wrap;gap:10px;">
      <label style="display:flex;align-items:center;gap:8px;cursor:pointer;font-size:13px;user-select:none;">
        <input type="checkbox" checked={allSelected} onchange={toggleAll} aria-label="全选当前列表案件" />
        <b>全选当前列表</b>
        <span class="app-muted">| {selectedIds.length} 项已选</span>
      </label>
    </div>
    <BatchBar count={selectedIds.length} noun="个案件" onclear={clearSelection}>
      <Button text="标记处理中" variant="secondary" size="sm" onclick={() => (batchStatusOpen = true)} />
      <Button text="批量关闭" variant="danger" size="sm" onclick={() => (batchCloseOpen = true)} />
      <Button text="批量驳回" variant="danger" size="sm" onclick={() => (batchRejectOpen = true)} />
    </BatchBar>
    <div class="app-muted" style="font-size:11px;margin-top:8px;">选择举报后执行批量操作</div>
  </div>
</section>

<!-- 案件卡片列表（原型高保真卡片结构） -->
{#if displayedCases.length === 0}
  <div class="app-card">
    <div class="app-card__body">
      <EmptyState icon="inbox" title="暂无案件" desc="当前筛选下没有待处理的案件" />
    </div>
  </div>
{:else}
  <div style="display:flex;flex-direction:column;gap:14px;">
    {#each displayedCases as item (item.id)}
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
              {item.title || '（无标题案件）'}
            </p>
          </div>

          <!-- 脚注行：负责人 / 操作（后端投影无举报人字段，不伪造展示） -->
          <div style="display:flex;align-items:center;justify-content:space-between;font-size:12px;color:var(--color-text-secondary);">
            <span>负责人 {item.assigned_to ?? '未指派'}</span>
            <a href="/admin/moderation/cases/{item.id}" class="text-link" style="font-weight:600;">
              {item.status === 'resolved' ? '查看' : '处理'}
            </a>
          </div>
        </div>
      </div>
    {/each}
  </div>
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
