<script lang="ts">
  import { enhance } from '$app/forms';
  import type { PageData } from './$types';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import { formatRelative } from '$lib/utils';

  let { data, form }: { data: PageData; form: any } = $props();

  const caseItem = $derived(data.caseItem);
  const okMessage = $derived(form?.ok as string | undefined);

  const statusLabels: Record<string, string> = {
    open: '待处理',
    triaged: '已分级',
    investigating: '调查中',
    resolved: '已解决',
    rejected: '已驳回',
    reopened: '已重开'
  };

  const nextStatuses = ['triaged', 'investigating', 'resolved', 'rejected', 'reopened'];
</script>

<svelte:head>
  <title>案件详情 — BBLBB</title>
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <a href="/admin/moderation/cases" class="breadcrumb-link">案件队列</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">详情</span>
  </nav>

  {#if data.forbidden}
    <div class="card">
      <div class="card-body" role="alert" data-testid="case-forbidden">
        <p class="form-error">无权访问该案件：{data.message ?? '需要 moderation.review 权限'}</p>
      </div>
    </div>
  {:else if !caseItem}
    <div class="card">
      <div class="card-body">
        {#if form?.message}<p class="form-error" role="alert">{form.message}</p>{/if}
        {#if okMessage}<p class="form-success" role="status" data-testid="case-ok">{okMessage}</p>{/if}
        <EmptyState icon="inbox" title="未找到案件" desc="该案件不存在或当前角色无权查看" />
      </div>
    </div>
  {:else}
    {#if form?.message}<p class="form-error" role="alert">{form.message}</p>{/if}
    {#if okMessage}<p class="form-success" role="status" data-testid="case-ok">{okMessage}</p>{/if}

    <div class="card">
      <div class="card-header">
        <span class="card-title">{caseItem.title || '未命名案件'}</span>
        <span class="badge">{statusLabels[caseItem.status] ?? caseItem.status} · {caseItem.priority}</span>
      </div>
      <div class="card-body stack">
        <div class="text-secondary">
          指派：{caseItem.assigned_to ?? '未指派'} · 创建于 {formatRelative(caseItem.created_at)}
          {#if caseItem.resolved_at != null} · 解决于 {formatRelative(caseItem.resolved_at)}{/if}
        </div>
        {#if caseItem.resolution}
          <div><strong>处理结论</strong><p style="white-space:pre-wrap;">{caseItem.resolution}</p></div>
        {/if}
      </div>
    </div>

    <div class="card" style="margin-top:var(--space-4);">
      <div class="card-header"><span class="card-title">状态迁移</span></div>
      <div class="card-body">
        <form method="POST" action="?/transition" use:enhance class="stack">
          <label>
            <span class="field-label">目标状态</span>
            <select name="status" required>
              {#each nextStatuses as s}
                <option value={s}>{statusLabels[s]}</option>
              {/each}
            </select>
          </label>
          <label>
            <span class="field-label">处理结论（可选）</span>
            <textarea name="resolution" rows="3"></textarea>
          </label>
          <button type="submit" class="btn btn-primary" data-testid="case-transition">保存状态</button>
        </form>
      </div>
    </div>

    <div class="card" style="margin-top:var(--space-4);">
      <div class="card-header"><span class="card-title">指派复核人</span></div>
      <div class="card-body">
        <form method="POST" action="?/assign" use:enhance class="stack">
          <label>
            <span class="field-label">复核人 ID</span>
            <input name="assignee_id" type="text" required placeholder="用户 ID" />
          </label>
          <label>
            <span class="field-label">备注（可选）</span>
            <input name="note" type="text" />
          </label>
          <button type="submit" class="btn btn-primary" data-testid="case-assign">指派</button>
        </form>
      </div>
    </div>
  {/if}
</div>
