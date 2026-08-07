<script lang="ts">
  import { enhance } from '$app/forms';
  import type { PageData } from './$types';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import { formatRelative } from '$lib/utils';

  let { data, form }: { data: PageData; form: any } = $props();

  const appeal = $derived(data.appeal);
  const withdrawn = $derived(form?.withdrawn as boolean | undefined);

  const statusLabels: Record<string, string> = {
    submitted: '待复核',
    reviewing: '复核中',
    upheld: '已支持',
    partially_upheld: '部分支持',
    rejected: '已驳回',
    withdrawn: '已撤回'
  };

  const canWithdraw = $derived(appeal?.status === 'submitted' || appeal?.status === 'reviewing');
</script>

<svelte:head>
  <title>申诉详情 — BBLBB</title>
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <a href="/moderation/appeals" class="breadcrumb-link">申诉</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">详情</span>
  </nav>

  {#if withdrawn}
    <div class="card" role="status" data-testid="appeal-withdrawn">
      <div class="card-header"><span class="card-title">申诉已撤回</span></div>
      <div class="card-body"><p>该申诉已撤回。重新处理请针对新处罚另行申诉。</p></div>
    </div>
  {:else if !appeal}
    <div class="card">
      <div class="card-body">
        {#if form?.message}<p class="form-error" role="alert">{form.message}</p>{/if}
        <EmptyState icon="scale" title="未找到申诉" desc="该申诉不存在或不属于当前账号" />
      </div>
    </div>
  {:else}
    <div class="card">
      <div class="card-header">
        <span class="card-title">申诉 <code>{appeal.id}</code></span>
        <span class="badge {appeal.status === 'upheld' ? 'badge-success' : ''}">{statusLabels[appeal.status] ?? appeal.status}</span>
      </div>
      <div class="card-body stack">
        <div class="text-secondary">处罚：<code>{appeal.sanction_id}</code></div>
        <div><strong>我的说明</strong></div>
        <p style="white-space:pre-wrap;">{appeal.message}</p>
        <div class="text-tertiary" style="font-size:var(--text-sm);">
          提交于 {formatRelative(appeal.submitted_at)}
          {#if appeal.decided_at != null} · 决定于 {formatRelative(appeal.decided_at)}{/if}
        </div>
        {#if canWithdraw}
          <form method="POST" action="?/withdraw" use:enhance>
            <button type="submit" class="btn btn-secondary" data-testid="withdraw-appeal">撤回申诉</button>
          </form>
        {/if}
      </div>
    </div>
  {/if}
</div>
