<script lang="ts">
  import { enhance } from '$app/forms';
  import type { PageData } from './$types';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import { formatRelative } from '$lib/utils';

  let { data, form }: { data: PageData; form: any } = $props();

  const submitted = $derived(form?.submitted as import('$lib/api/types').OwnAppeal | null | undefined);

  const statusLabels: Record<string, string> = {
    submitted: '待复核',
    reviewing: '复核中',
    upheld: '已支持',
    partially_upheld: '部分支持',
    rejected: '已驳回',
    withdrawn: '已撤回'
  };
</script>

<svelte:head>
  <title>申诉 — BBLBB</title>
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">申诉</span>
  </nav>

  <h1 class="page-title">处罚申诉</h1>

  <div class="card">
    <div class="card-header"><span class="card-title">提交申诉</span></div>
    <div class="card-body">
      <form method="POST" action="?/create" use:enhance class="stack">
        <label>
          <span class="field-label">处罚 ID（sanction_id）</span>
          <input name="sanction_id" type="text" required placeholder="处罚通知中的 ID" />
        </label>
        <label>
          <span class="field-label">申诉内容（1–5000 字，禁止附件引用）</span>
          <textarea name="content" rows="5" maxlength="5000" required></textarea>
        </label>
        {#if form?.message}
          <p class="form-error" role="alert" data-testid="appeal-error">{form.message}</p>
        {/if}
        <button type="submit" class="btn btn-primary">提交申诉</button>
      </form>
    </div>
  </div>

  {#if submitted}
    <div class="card" style="margin-top:var(--space-5);" role="status" data-testid="appeal-success">
      <div class="card-header"><span class="card-title">申诉已提交</span></div>
      <div class="card-body">
        <p>申诉 <a href={`/moderation/appeals/${submitted.id}`} class="link">{submitted.id}</a> 已提交，可随时查看进度或撤回。</p>
      </div>
    </div>
  {/if}

  <div class="card" style="margin-top:var(--space-5);">
    <div class="card-header"><span class="card-title">我的申诉</span></div>
    <div class="card-body" style="padding:0;">
      {#if data.items.length === 0}
        <EmptyState icon="scale" title="暂无申诉" desc="你对处罚的申诉会显示在这里" />
      {:else}
        <div style="display:flex;flex-direction:column;">
          {#each data.items as item}
            <a
              href={`/moderation/appeals/${item.id}`}
              class="post-row"
              style="padding:var(--space-4);border-bottom:var(--border-default);display:flex;gap:var(--space-3);align-items:center;justify-content:space-between;text-decoration:none;"
            >
              <div style="min-width:0;">
                <div><span class="badge">{statusLabels[item.status] ?? item.status}</span> <code>{item.sanction_id}</code></div>
                <div class="text-secondary" style="font-size:var(--text-sm);margin-top:2px;">提交于 {formatRelative(item.submitted_at)}</div>
              </div>
              <span class="text-tertiary" style="font-size:var(--text-xs);">{item.status === 'submitted' || item.status === 'reviewing' ? '可撤回' : ''}</span>
            </a>
          {/each}
        </div>
      {/if}
    </div>
  </div>
</div>
