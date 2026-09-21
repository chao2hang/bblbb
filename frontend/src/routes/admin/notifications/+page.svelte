<script lang="ts">
  // M18-ADMIN-NOTIFICATIONS：通知与邮件管理页（对齐原型 #admin-notifications）。
  // M18-ADMIN-BATCH：写操作全部收敛为「按钮 → 弹层」——
  // - 发送公告：页头按钮 + Dialog（?/broadcast）；
  // - 行内撤回：按钮 + DangerConfirm（?/recall，reason 必填写审计）；
  // - 批量撤回：已发送广播表选择列 + BatchBar + 批量 Dialog（?/batchRecall，
  //   循环单条 recall 端点；模板表为只读注册表、无写端点，不再提供选择列）。
  // M18-ADMIN-OPS（约定 D）：行内撤回入口改为「⋮」三点菜单——单项「撤回广播」
  // 打开既有撤回 DangerConfirm（SSR 隐藏表单 requestSubmit 机制不变）。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import BatchBar from '$lib/components/admin/BatchBar.svelte';
  import RowActionsMenu from '$lib/components/admin/RowActionsMenu.svelte';
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import DangerConfirm from '$lib/components/ui/DangerConfirm.svelte';
  import Dialog from '$lib/components/ui/Dialog.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { withActionToast, toastActionResult } from '$lib/ui/action-toast';
  import type { AdminNotificationsPageData, AdminNotificationsActionData } from './+page.server';
  import type { BroadcastItem } from '$lib/api/types';

  let { data, form }: { data: AdminNotificationsPageData; form?: AdminNotificationsActionData | null } = $props();

  const templatesList = $derived.by(() => {
    if (data.templates && data.templates.length > 0) {
      // P0 整改：模板状态只来自 API 字段；后端为常量注册表、无 per-template
      // 状态时如实展示 "—"，不再用 id/索引猜测 paused/active 或伪造 failed=0。
      return data.templates.map((t) => ({
        id: t.id,
        name: t.name,
        trigger: t.trigger || '事件触发',
        queue: t.queue === 'high' ? 1 : 0,
        failed: null as number | null,
        status: (t as { status?: string }).status ?? '—'
      }));
    }
    return [];
  });

  const broadcastsList = $derived(data.items ?? []);

  let q = $state('');
  let statusFilter = $state('');

  // ── 发送公告（Dialog 版）：表单在 Dialog 内，成功后关闭并清空草稿。 ──
  let broadcastOpen = $state(false);
  let broadcastTitle = $state('');
  let broadcastContent = $state('');
  let broadcastTarget = $state('all');
  let sending = $state(false);

  function openBroadcast(): void {
    broadcastTitle = '';
    broadcastContent = '';
    broadcastTarget = 'all';
    broadcastOpen = true;
  }

  // ── 行内撤回（DangerConfirm + 隐藏表单 requestSubmit）：reason 必填写审计。 ──
  let recallTarget: BroadcastItem | null = $state(null);
  let recallReason = $state('');
  let recallError = $state('');
  let isRecalling = $state(false);
  let recallForm: HTMLFormElement | undefined = $state();

  function openRecall(item: BroadcastItem): void {
    recallTarget = item;
    recallReason = '';
    recallError = '';
  }

  /** 行「⋮」菜单项（约定 D：单一「撤回广播」动作 → 既有撤回 DangerConfirm 流）。 */
  function rowActions(b: BroadcastItem) {
    return [
      {
        label: '撤回广播',
        danger: true,
        run: () => openRecall(b)
      }
    ];
  }

  // ── 批量撤回（已发送广播表）：仅未撤回的行可选；批量 Dialog 填公共原因。 ──
  let batchRecallOpen = $state(false);
  let batchRecallReason = $state('');
  let selectedIds = $state<string[]>([]);

  /** 可撤回（未撤回）的广播行——选择列只覆盖这些行。 */
  const recallableBroadcasts = $derived(broadcastsList.filter((b) => !b.recalled));

  let allSelected = $derived(
    recallableBroadcasts.length > 0 && selectedIds.length === recallableBroadcasts.length
  );
  function toggleAll() {
    if (allSelected) selectedIds = [];
    else selectedIds = recallableBroadcasts.map((b) => b.id);
  }
  function toggleRow(id: string) {
    if (selectedIds.includes(id)) selectedIds = selectedIds.filter((x) => x !== id);
    else selectedIds = [...selectedIds, id];
  }

  // JS 启用：动作结果走全局 Toast 浮窗（成功绿/失败红）；顶部内联横幅仅保留为
  // 无 JS 回退（SSR HTML 仍渲染，见 hasJs）。
  let hasJs = $state(false);
  $effect(() => {
    hasJs = true;
  });

  const displayedTemplates = $derived.by(() => {
    let list = templatesList;
    if (q.trim()) {
      const kw = q.trim().toLowerCase();
      list = list.filter((t) => t.name.toLowerCase().includes(kw) || t.id.toLowerCase().includes(kw));
    }
    if (statusFilter === 'active') {
      list = list.filter((t) => t.status === 'active');
    } else if (statusFilter === 'paused') {
      list = list.filter((t) => t.status === 'paused');
    }
    return list;
  });

  function formatTimestamp(ts: number): string {
    if (!ts) return '-';
    const date = new Date(ts > 1e11 ? ts : ts * 1000);
    return date.toLocaleString('zh-CN', {
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit'
    });
  }
</script>

<svelte:head>
  <title>通知与邮件 — BBLBB Admin</title>
</svelte:head>

<PageHeader title="通知与邮件" />

{#if form?.message && !hasJs}
  <div class="alert alert-info" role="status" style="margin-bottom:12px;padding:10px 14px;background:var(--color-bg-subtle);border-radius:var(--radius-sm);font-size:13px;">
    {form.message}
  </div>
{/if}

<!-- 卡片 1：模板与队列（只读注册表：无行级写操作，不提供选择列） -->
<section class="app-card" style="margin-bottom:14px;">
  <header class="app-card__head" style="display:flex;justify-content:space-between;align-items:center;flex-wrap:wrap;gap:8px;">
    <h2>模板与队列</h2>
    <Button text="发送公告" variant="primary" size="sm" onclick={openBroadcast} />
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
        <option value="active">启用中</option>
        <option value="paused">已暂停</option>
      </select>
      {#if q || statusFilter}
        <button type="button" class="btn ghost sm" style="flex:0 0 auto;" onclick={() => { q = ''; statusFilter = ''; }}>
          清除
        </button>
      {/if}
    </div>

    <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:10px;">
      <span class="app-muted" style="font-size:12px;">共 {displayedTemplates.length} 个模板</span>
    </div>

    <div class="app-table-wrap">
      <table class="app-table" aria-label="通知模板列表">
        <thead>
          <tr>
            <th>模板</th>
            <th style="width:100px;">状态</th>
            <th>触发时机</th>
            <th>待发队列</th>
            <th>最近失败</th>
          </tr>
        </thead>
        <tbody>
          {#if displayedTemplates.length === 0}
            <tr>
              <td colspan="5" style="text-align:center;padding:24px;color:var(--color-text-secondary);">
                当前筛选下没有通知模板
              </td>
            </tr>
          {:else}
            {#each displayedTemplates as t (t.id)}
              <tr>
                <td>
                  <b>{t.name}</b>
                  <code style="font-size:11px;color:var(--color-text-secondary);display:block;">{t.id}</code>
                </td>
                <td>
                  <span class="badge {t.status === 'paused' ? 'badge-neutral' : 'badge-success'}" style="font-size:11px;">
                    {t.status === 'paused' ? '已暂停' : t.status === 'active' ? '启用中' : t.status}
                  </span>
                </td>
                <td><span class="text-secondary" style="font-size:13px;">{t.trigger}</span></td>
                <td>
                  <span class="badge {t.queue > 0 ? 'badge-warning' : 'badge-gray'}">{t.queue}</span>
                </td>
                <td>
                  {#if t.failed === null}
                    <span class="text-secondary" style="font-size:12px;">—</span>
                  {:else if t.failed > 0}
                    <span class="badge badge-danger">{t.failed} 失败</span>
                  {:else}
                    <span class="text-secondary" style="font-size:12px;">无</span>
                  {/if}
                </td>
              </tr>
            {/each}
          {/if}
        </tbody>
      </table>
    </div>
  </div>
</section>

<!-- 卡片 2：队列状态 -->
<section class="app-card" style="margin-bottom:14px;">
  <header class="app-card__head">
    <h2>通知与邮件队列状态</h2>
  </header>
  <div class="app-card__body" style="display:flex;align-items:center;justify-content:space-between;flex-wrap:wrap;gap:12px;">
    <div style="font-size:13px;color:var(--color-text-primary);">
      <b>{data.queue?.failed ?? 0}</b> 条待重试 · 发件箱待发队列：<b>{data.queue?.outbox_count ?? 0}</b> 条 · 投递模式：<code>{data.queue?.mode ?? 'inline/outbox'}</code>
    </div>
    <!-- P0 整改：移除死 action「触发队列巡检」——服务端无 ?/retry，后端无对应端点。 -->
  </div>
</section>

<!-- 卡片 3：已发送广播（真实发件箱列表 + 批量撤回） -->
<section class="app-card">
  <header class="app-card__head" style="display:flex;align-items:center;justify-content:space-between;">
    <h2>已发送广播</h2>
    <span class="text-secondary" style="font-size:12px;">已记录 {broadcastsList.length} 条</span>
  </header>
  <div class="app-card__body">
    {#if broadcastsList.length > 0}
      <BatchBar count={selectedIds.length} noun="条广播" onclear={() => (selectedIds = [])}>
        <Button text="批量撤回" variant="danger" size="sm" onclick={() => (batchRecallOpen = true)} />
      </BatchBar>
      <div class="app-table-wrap">
        <table class="app-table" aria-label="已发送广播历史">
          <thead>
            <tr>
              <th style="width:40px;text-align:center;">
                <input
                  type="checkbox"
                  checked={allSelected}
                  onchange={toggleAll}
                  aria-label="全选可撤回广播"
                />
              </th>
              <th>标题与内容摘要</th>
              <th>受众目标</th>
              <th>发布者</th>
              <th>发送时间</th>
              <th>状态</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            {#each broadcastsList as b (b.id)}
              <tr>
                <td style="text-align:center;">
                  {#if !b.recalled}
                    <input
                      type="checkbox"
                      checked={selectedIds.includes(b.id)}
                      onchange={() => toggleRow(b.id)}
                      aria-label="选中广播 {b.title}"
                    />
                  {:else}
                    <span class="text-secondary" style="font-size:11px;">-</span>
                  {/if}
                </td>
                <td>
                  <b>{b.title}</b>
                  <p class="text-secondary" style="font-size:12px;margin:2px 0 0 0;line-height:1.4;">
                    {b.body.length > 60 ? b.body.slice(0, 60) + '...' : b.body}
                  </p>
                </td>
                <td>
                  <span class="badge badge-gray">
                    {b.target_type === 'admins' ? '管理员/版主' : '全体成员'}
                  </span>
                </td>
                <td>
                  <span style="font-size:13px;">{b.sender_username || '系统'}</span>
                </td>
                <td>
                  <span class="text-secondary" style="font-size:12px;">{formatTimestamp(b.created_at)}</span>
                </td>
                <td>
                  {#if b.recalled}
                    <span class="sbadge sb-gray">已撤回</span>
                  {:else}
                    <span class="sbadge sb-success">已发送</span>
                  {/if}
                </td>
                <td>
                  {#if !b.recalled}
                    <!-- 写操作：每行一个「⋮」菜单（约定 D）→ 撤回 DangerConfirm（隐藏表单机制不变） -->
                    <RowActionsMenu
                      label="更多操作：广播 {b.title}"
                      actions={rowActions(b)}
                    />
                  {:else}
                    <span class="text-secondary" style="font-size:11px;">-</span>
                  {/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
        {#if data.nextCursor}
          <div style="display:flex;justify-content:flex-end;margin-top:10px;">
            <a class="btn secondary sm" href={`/admin/notifications?after=${encodeURIComponent(data.nextCursor)}`}>
              下一页 →
            </a>
          </div>
        {/if}
      </div>
    {:else}
      <div style="padding:28px 16px;text-align:center;border:1px dashed var(--color-border);border-radius:var(--radius-md);background:var(--color-bg-subtle, rgba(0,0,0,0.02));display:flex;flex-direction:column;align-items:center;gap:8px;">
        <Icon name="bell" size={24} />
        <strong style="font-size:14px;color:var(--color-text-primary);">还没有已发送的广播</strong>
        <span class="text-secondary" style="font-size:12px;">使用页头「发送公告」按钮发送第一条全员广播，所有通知将记录并支持即时撤回。</span>
      </div>
    {/if}
  </div>
</section>

<!-- 发送公告：Dialog 内表单（title/body/target → POST broadcast）。 -->
<Dialog
  open={broadcastOpen}
  title="发送公告"
  description="向全体成员或管理员/版主发送站内广播；记录在发件箱并可撤回。"
  onclose={() => (broadcastOpen = false)}
>
  <form
    method="POST"
    action="?/broadcast"
    use:enhance={() => {
      sending = true;
      return async ({ result, update }) => {
        sending = false;
        // 动作结果 → 全局 Toast（成功“已发送给 N 位成员”/ 失败服务端文案）；
        // 顶部横幅为无 JS 回退，JS 模式下失败也靠 Toast 提示，避免结果不可见。
        toastActionResult(result);
        if (result.type === 'success') {
          broadcastTitle = '';
          broadcastContent = '';
          broadcastOpen = false;
        }
        await update();
      };
    }}
    class="stack"
    style="gap:12px;"
  >
    <label>
      <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">
        标题 <span style="color:var(--color-danger);">*</span>
      </span>
      <input
        type="text"
        name="title"
        class="input-field"
        placeholder="例如：本周末例行维护通知"
        required
        bind:value={broadcastTitle}
        style="width:100%;"
      />
    </label>

    <label>
      <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">
        内容 <span style="color:var(--color-danger);">*</span>
      </span>
      <textarea
        name="body"
        class="input-field"
        rows="4"
        placeholder="输入广播正文内容…"
        required
        bind:value={broadcastContent}
        style="width:100%;"
      ></textarea>
    </label>

    <label>
      <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">发送目标受众</span>
      <select name="target" class="app-select" bind:value={broadcastTarget} style="width:100%;">
        <option value="all">全体成员 (All members)</option>
        <option value="admins">仅管理员与版主 (Admins & Moderators)</option>
      </select>
    </label>

    <div>
      <Button text={sending ? '发送中…' : '立即发送广播'} variant="primary" size="sm" type="submit" disabled={sending} />
    </div>
  </form>
</Dialog>

<!-- 行内撤回：DangerConfirm + reason（写审计），确认后提交隐藏表单。
     隐藏表单常驻 DOM（id/version/reason 审计要素），SSR 基线可见。 -->
<form
  method="POST"
  action="?/recall"
  bind:this={recallForm}
  use:enhance={() => {
    isRecalling = true;
    return async ({ result, update }) => {
      isRecalling = false;
      // 结果 message 走全局 Toast（兜底文案与原先一致）
      toastActionResult(result, {
        message: (d) => (d?.message as string | null) ?? (result.type === 'success' ? '已撤回' : '撤回失败')
      });
      await update();
      if (result.type === 'success') {
        recallTarget = null;
        recallReason = '';
        recallError = '';
      }
    };
  }}
>
  <input type="hidden" name="id" value={recallTarget?.id ?? ''} />
  <input type="hidden" name="reason" value={recallReason} />
</form>

<DangerConfirm
  open={recallTarget !== null}
  title="撤回广播"
  description={recallTarget ? `确认撤回「${recallTarget.title}」？未读通知将删除，已读通知保留，不可恢复。` : ''}
  confirmText="确认撤回"
  busy={isRecalling}
  error={recallError}
  oncancel={() => {
    recallTarget = null;
    recallError = '';
  }}
  onconfirm={() => {
    if (!recallReason.trim()) {
      recallError = '撤回原因必填（写入审计日志）';
      return;
    }
    recallError = '';
    recallForm?.requestSubmit();
  }}
>
  <label class="input-label" for="recall-reason">撤回原因（写审计）</label>
  <input id="recall-reason" class="input-field" bind:value={recallReason} placeholder="必填" required />
</DangerConfirm>

<!-- 批量撤回：批量 Dialog 填公共原因 → POST batchRecall（循环单条 recall 端点）。 -->
<Dialog
  open={batchRecallOpen}
  title="批量撤回广播"
  description={`将撤回 ${selectedIds.length} 条广播；未读通知删除、已读保留，原因写审计。`}
  onclose={() => (batchRecallOpen = false)}
>
  <form
    method="POST"
    action="?/batchRecall"
    use:enhance={() => {
      return async ({ result, update }) => {
        toastActionResult(result, {
          message: (d) => (d?.message as string | null) ?? (result.type === 'success' ? '批量撤回完成' : '批量撤回失败')
        });
        await update();
        selectedIds = [];
        batchRecallOpen = false;
      };
    }}
  >
    <input type="hidden" name="ids" value={selectedIds.join(',')} />
    <div class="input-wrapper" style="margin-bottom:var(--space-3);">
      <label class="input-label" for="batch-recall-reason">撤回原因（写审计）</label>
      <input id="batch-recall-reason" name="reason" class="input-field" required bind:value={batchRecallReason} placeholder="必填" />
    </div>
    <Button text="确认批量撤回" variant="danger" size="sm" type="submit" />
  </form>
</Dialog>
