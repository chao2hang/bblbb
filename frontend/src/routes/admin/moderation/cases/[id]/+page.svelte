<script lang="ts">
  // M18-ADMIN-REPORTS：案件详情页（对齐原型 #admin-report:id 5 大卡片结构）。
  // 操作约定：处理动作统一为「开始处理」按钮 → Dialog 弹层内完成（选择处罚 + 原因必填）。
  import { enhance } from '$app/forms';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import Dialog from '$lib/components/ui/Dialog.svelte';
  import { formatRelative } from '$lib/utils';
  import type { PageData } from './$types';
  import { toastActionResult, withActionToast } from '$lib/ui/action-toast';

  let { data, form }: { data: PageData; form: any } = $props();

  const caseItem = $derived(data.caseItem);
  const okMessage = $derived(form?.ok as string | undefined);

  // JS 启用：动作结果走全局 Toast 浮窗（成功绿/失败红）；顶部内联横幅仅保留为
  // 无 JS 回退（SSR HTML 仍渲染，见 hasJs）。
  let hasJs = $state(false);
  $effect(() => {
    hasJs = true;
  });

  let transitionOpen = $state(false);
  let penaltyAction = $state<'hide' | 'mute' | 'dismiss'>('mute');
  let reasonText = $state('');
  let isSubmitting = $state(false);

  let assignOpen = $state(false);
  let assigneeId = $state('');
  let assignNote = $state('');

  const statusLabels: Record<string, string> = {
    open: '待处理',
    triaged: '处理中',
    investigating: '调查中',
    resolved: '已解决',
    rejected: '已驳回',
    reopened: '已重开'
  };

  const priorityLabels: Record<string, string> = {
    low: '低',
    normal: '普通',
    high: '高',
    urgent: '紧急'
  };

  const penaltyLabels: Record<'hide' | 'mute' | 'dismiss', string> = {
    hide: '隐藏内容',
    mute: '禁言 7 天',
    dismiss: '驳回举报'
  };
</script>

<svelte:head>
  <title>案件详情 · {caseItem?.id ?? ''} — BBLBB Admin</title>
</svelte:head>

<div style="margin-bottom:14px;">
  <a href="/admin/moderation/cases" class="btn ghost sm" style="text-decoration:none;">
    ← 返回案件队列
  </a>
</div>

{#if data.forbidden}
  <div class="app-card">
    <div class="app-card__body" role="alert">
      <p class="form-error">无权访问该案件：{data.message ?? '需要 moderation.review 权限'}</p>
    </div>
  </div>
{:else if !caseItem}
  <div class="app-card">
    <div class="app-card__body">
      {#if form?.message && !hasJs}<p class="form-error" role="alert">{form.message}</p>{/if}
      <EmptyState icon="inbox" title="未找到案件" desc="该案件不存在或当前角色无权查看" />
    </div>
  </div>
{:else}
  {#if form?.message && !hasJs}<div class="alert alert-danger" role="alert" style="margin-bottom:12px;">{form.message}</div>{/if}
  {#if okMessage && !hasJs}<div class="alert alert-success" role="status" style="margin-bottom:12px;">{okMessage}</div>{/if}

  <!-- 卡片 1：案件标题（后端投影仅含 title；正文/目标内容需后续契约扩展） -->
  <section class="app-card" style="margin-bottom:14px;">
    <header class="app-card__head">
      <h2>案件内容</h2>
    </header>
    <div class="app-card__body">
      <div style="background:var(--color-bg-subtle, rgba(0,0,0,0.03));padding:14px 16px;border-left:3px solid var(--color-text-secondary);border-radius:0 var(--radius-sm) var(--radius-sm) 0;">
        <p style="margin:0;font-size:14px;line-height:1.6;color:var(--color-text-primary);">
          {caseItem.title}
        </p>
      </div>
    </div>
  </section>

  <!-- 卡片 2：处理动作（操作入口 = 按钮，弹层内完成具体处理） -->
  <section class="app-card" style="margin-bottom:14px;">
    <header class="app-card__head">
      <h2>处理动作</h2>
    </header>
    <div class="app-card__body">
      <p style="margin:0 0 12px;font-size:13px;color:var(--color-text-secondary);">
        处理动作（隐藏内容 / 禁言 7 天 / 驳回举报）需填写处理原因并写入审计日志。
      </p>
      <Button text="开始处理" variant="primary" size="sm" icon="shield" onclick={() => (transitionOpen = true)} />
    </div>
  </section>

  <!-- 卡片 3：处理时间线 -->
  <section class="app-card" style="margin-bottom:14px;">
    <header class="app-card__head">
      <h2>处理时间线</h2>
    </header>
    <div class="app-card__body" style="padding:14px 16px;">
      <ul style="list-style:none;padding:0;margin:0;display:flex;flex-direction:column;gap:10px;font-size:13px;">
        <li style="display:flex;align-items:center;gap:8px;">
          <span style="display:inline-block;width:8px;height:8px;border-radius:2px;background:var(--color-text-secondary);"></span>
          <span>提交举报 · {formatRelative(caseItem.created_at)}</span>
        </li>
        <li style="display:flex;align-items:center;gap:8px;">
          <span style="display:inline-block;width:8px;height:8px;border-radius:2px;background:var(--color-brand);"></span>
          <span>
            {caseItem.assigned_to ? `已指派给 ${caseItem.assigned_to}` : '最近更新'} · {formatRelative(caseItem.updated_at)}
          </span>
        </li>
        {#if caseItem.resolved_at}
          <li style="display:flex;align-items:center;gap:8px;">
            <span style="display:inline-block;width:8px;height:8px;border-radius:2px;background:var(--color-success);"></span>
            <span>已处理 · {formatRelative(caseItem.resolved_at)}</span>
          </li>
        {/if}
      </ul>
    </div>
  </section>

  <!-- 卡片 4：案件信息 -->
  <section class="app-card" style="margin-bottom:14px;">
    <header class="app-card__head">
      <h2>案件信息</h2>
    </header>
    <div class="app-card__body" style="padding:14px 16px;display:flex;flex-direction:column;gap:8px;font-size:13px;">
      <div style="display:flex;gap:8px;align-items:baseline;">
        <span class="text-secondary">举报单号</span>
        <code style="padding:2px 6px;background:var(--color-bg-subtle);border-radius:3px;">{caseItem.id}</code>
      </div>
      <div style="display:flex;gap:8px;">
        <span class="text-secondary">优先级</span>
        <span>{priorityLabels[caseItem.priority] ?? caseItem.priority}</span>
      </div>
      <div style="display:flex;gap:8px;align-items:center;">
        <span class="text-secondary">负责人</span>
        <span>{caseItem.assigned_to ?? '未指派'}</span>
        <button type="button" class="btn ghost sm" style="padding:2px 8px;font-size:12px;margin-left:auto;" onclick={() => (assignOpen = true)}>
          指派负责人
        </button>
      </div>
      <div style="display:flex;gap:8px;align-items:center;">
        <span class="text-secondary">状态：</span>
        <span class="badge badge-warning">{statusLabels[caseItem.status] ?? caseItem.status}</span>
      </div>
    </div>
  </section>

  <!-- 卡片 5：申诉状态 -->
  <section class="app-card">
    <header class="app-card__head">
      <h2>申诉状态</h2>
    </header>
    <div class="app-card__body" style="padding:14px 16px;">
      <p style="margin:0;font-size:13px;line-height:1.6;color:var(--color-text-secondary);">
        处罚后用户可从申诉中心提交复核；申诉结果必须追加到审计时间线。
      </p>
    </div>
  </section>

  <!-- 处理案件：Dialog 内表单（处罚选择 + 原因必填 → ?/transition）。 -->
  <Dialog
    open={transitionOpen}
    title="处理案件"
    description="选择处理动作并填写原因（必填，写入审计日志）。"
    onclose={() => (transitionOpen = false)}
  >
    <form
      method="POST"
      action="?/transition"
      use:enhance={() => {
        isSubmitting = true;
        return async ({ result, update }) => {
          isSubmitting = false;
          toastActionResult(result, { message: (d) => (d?.message ?? d?.ok) as string | null });
          await update();
          if (result.type === 'success') {
            transitionOpen = false;
            reasonText = '';
          }
        };
      }}
      style="display:flex;flex-direction:column;gap:12px;"
    >
      <!-- 隐式映射到后端的 status 与 resolution -->
      <input type="hidden" name="status" value={penaltyAction === 'dismiss' ? 'rejected' : 'resolved'} />
      <input
        type="hidden"
        name="resolution"
        value={penaltyAction === 'dismiss' ? reasonText : `[${penaltyLabels[penaltyAction]}] ${reasonText}`}
      />

      <div>
        <span class="input-label" style="font-size:13px;margin-bottom:6px;display:block;">处理动作</span>
        <div style="display:grid;grid-template-columns:repeat(3, 1fr);gap:8px;">
          {#each Object.entries(penaltyLabels) as [key, label] (key)}
            <button
              type="button"
              class="btn sm {penaltyAction === key ? 'secondary' : 'ghost'}"
              style={penaltyAction === key ? 'border:1px solid var(--color-brand);font-weight:600;' : ''}
              onclick={() => (penaltyAction = key as 'hide' | 'mute' | 'dismiss')}
            >
              {label}
            </button>
          {/each}
        </div>
      </div>

      <div>
        <label class="input-label" for="case-reason" style="font-size:13px;margin-bottom:6px;display:block;">
          处理原因 <span style="color:var(--color-danger);">*</span>
        </label>
        <textarea
          id="case-reason"
          class="input-field"
          rows="3"
          placeholder="必填，写入审计日志"
          required
          bind:value={reasonText}
          style="width:100%;font-size:13px;"
        ></textarea>
      </div>

      <Button text={isSubmitting ? '提交中...' : '提交处理'} variant="primary" type="submit" block disabled={isSubmitting} />
    </form>
  </Dialog>

  <!-- 指派负责人 Dialog：?/assign -->
  <Dialog
    open={assignOpen}
    title="指派案件负责人"
    description="指派给特定审核员（输入审核员 ID 或用户名），操作记录写入审计日志。"
    onclose={() => (assignOpen = false)}
  >
    <form
      method="POST"
      action="?/assign"
      use:enhance={() => {
        isSubmitting = true;
        return async ({ result, update }) => {
          isSubmitting = false;
          toastActionResult(result, { message: (d) => (d?.message ?? d?.ok) as string | null });
          await update();
          if (result.type === 'success') {
            assignOpen = false;
            assigneeId = '';
            assignNote = '';
          }
        };
      }}
      style="display:flex;flex-direction:column;gap:12px;"
    >
      <div>
        <label class="input-label" for="case-assignee" style="font-size:13px;margin-bottom:6px;display:block;">
          复核人 ID / 用户名 <span style="color:var(--color-danger);">*</span>
        </label>
        <input
          id="case-assignee"
          name="assignee_id"
          class="input-field"
          required
          bind:value={assigneeId}
          placeholder="输入审核员 user_id 或用户名"
          style="width:100%;font-size:13px;"
        />
      </div>
      <div>
        <label class="input-label" for="case-assign-note" style="font-size:13px;margin-bottom:6px;display:block;">
          指派说明（可选）
        </label>
        <input
          id="case-assign-note"
          name="note"
          class="input-field"
          bind:value={assignNote}
          placeholder="如：转由法务/安全组专员处理"
          style="width:100%;font-size:13px;"
        />
      </div>
      <Button text={isSubmitting ? '指派中...' : '确认指派'} variant="primary" type="submit" block disabled={isSubmitting} />
    </form>
  </Dialog>
{/if}
