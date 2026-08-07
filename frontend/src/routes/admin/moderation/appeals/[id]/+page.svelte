<script lang="ts">
  import { enhance } from '$app/forms';
  import type { PageData } from './$types';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import { formatRelative } from '$lib/utils';

  let { data, form }: { data: PageData; form: any } = $props();

  const appeal = $derived(data.appeal);
  const okMessage = $derived(form?.ok as string | undefined);

  const statusLabels: Record<string, string> = {
    submitted: '待复核',
    reviewing: '复核中',
    upheld: '已支持',
    partially_upheld: '部分支持',
    rejected: '已驳回',
    withdrawn: '已撤回'
  };

  const decisions = [
    { value: 'upheld', label: '支持（撤销处罚）' },
    { value: 'partially_upheld', label: '部分支持（补偿记录）' },
    { value: 'rejected', label: '驳回' }
  ];

  const isDecided = $derived(
    appeal?.status === 'upheld' ||
      appeal?.status === 'partially_upheld' ||
      appeal?.status === 'rejected' ||
      appeal?.status === 'withdrawn'
  );
</script>

<svelte:head>
  <title>复核申诉 — BBLBB</title>
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">复核申诉</span>
  </nav>

  {#if data.forbidden}
    <div class="card">
      <div class="card-body" role="alert" data-testid="admin-appeal-forbidden">
        <p class="form-error">无权复核该申诉：{data.message ?? '需要 moderation.sanction 权限'}</p>
      </div>
    </div>
  {:else if !appeal}
    <div class="card">
      <div class="card-body">
        {#if form?.message}<p class="form-error" role="alert">{form.message}</p>{/if}
        {#if okMessage}<p class="form-success" role="status">{okMessage}</p>{/if}
        <EmptyState icon="scale" title="未找到申诉" desc="该申诉不存在或当前角色无权查看" />
      </div>
    </div>
  {:else}
    {#if form?.message}<p class="form-error" role="alert">{form.message}</p>{/if}
    {#if okMessage}<p class="form-success" role="status" data-testid="admin-appeal-ok">{okMessage}</p>{/if}

    <div class="card">
      <div class="card-header">
        <span class="card-title">申诉 <code>{appeal.id}</code></span>
        <span class="badge">{statusLabels[appeal.status] ?? appeal.status}</span>
      </div>
      <div class="card-body stack">
        <div class="text-secondary">
          用户 <code>{appeal.user_id}</code> · 处罚 <code>{appeal.sanction_id}</code> · 提交于 {formatRelative(appeal.submitted_at)}
          {#if appeal.reviewed_by} · 复核人 <code>{appeal.reviewed_by}</code>{/if}
        </div>
        <div><strong>申诉内容</strong></div>
        <p style="white-space:pre-wrap;">{appeal.message}</p>
        {#if appeal.decisions.length > 0}
          <div><strong>历史决定</strong></div>
          {#each appeal.decisions as d}
            <div class="text-secondary" style="font-size:var(--text-sm);">
              {d.decision} · {d.reviewer_id} · {formatRelative(d.created_at)}
              {#if d.decision_note}：{d.decision_note}{/if}
            </div>
          {/each}
        {/if}
      </div>
    </div>

    {#if !isDecided}
      <div class="card" style="margin-top:var(--space-4);">
        <div class="card-header"><span class="card-title">作出决定</span></div>
        <div class="card-body">
          <form method="POST" action="?/decide" use:enhance class="stack">
            <label>
              <span class="field-label">决定</span>
              <select name="decision" required>
                {#each decisions as d}
                  <option value={d.value}>{d.label}</option>
                {/each}
              </select>
            </label>
            <label>
              <span class="field-label">理由（必填，1–2000 字）</span>
              <textarea name="reason" rows="4" maxlength="2000" required></textarea>
            </label>
            <input type="hidden" name="expected_version" value={appeal?.updated_at ?? ''} />
            <button type="submit" class="btn btn-primary" data-testid="decide-appeal">提交决定</button>
          </form>
        </div>
      </div>
    {/if}
  {/if}
</div>
