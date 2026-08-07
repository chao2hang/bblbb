<script lang="ts">
  import { page } from '$app/state';
  import type { PageData } from './$types';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import { formatRelative } from '$lib/utils';

  let { data }: { data: PageData } = $props();

  const statusLabels: Record<string, string> = {
    open: '待处理',
    triaged: '已分级',
    investigating: '调查中',
    resolved: '已解决',
    rejected: '已驳回',
    reopened: '已重开'
  };

  const filters = ['', 'open', 'triaged', 'investigating', 'resolved', 'rejected', 'reopened'];
  const current = $derived(page.url.searchParams.get('status') ?? '');
</script>

<svelte:head>
  <title>案件队列 — BBLBB</title>
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <a href="/admin" class="breadcrumb-link">管理后台</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">审核案件</span>
  </nav>

  <h1 class="page-title">审核案件队列</h1>

  {#if data.forbidden}
    <div class="card">
      <div class="card-body" role="alert" data-testid="cases-forbidden">
        <p class="form-error">无权访问案件队列：{data.error ?? '需要 moderation.review 权限'}</p>
      </div>
    </div>
  {:else}
    <div class="tabs" role="tablist" aria-label="状态筛选" style="margin-bottom:var(--space-4);">
      {#each filters as f}
        <a
          role="tab"
          aria-selected={current === f ? 'true' : 'false'}
          href={f ? `/admin/moderation/cases?status=${f}` : '/admin/moderation/cases'}
          class="tab {current === f ? 'is-active' : ''}"
        >{f ? statusLabels[f] ?? f : '全部'}</a>
      {/each}
    </div>

    <div class="card">
      <div class="card-body" style="padding:0;">
        {#if data.items.length === 0}
          <EmptyState icon="inbox" title="暂无案件" desc="当前筛选下没有待处理的案件" />
        {:else}
          <div style="display:flex;flex-direction:column;">
            {#each data.items as item}
              <a
                href={`/admin/moderation/cases/${item.id}`}
                class="post-row"
                style="padding:var(--space-4);border-bottom:var(--border-default);display:flex;gap:var(--space-3);align-items:center;justify-content:space-between;text-decoration:none;"
              >
                <div style="min-width:0;">
                  <div style="font-weight:var(--weight-medium);">{item.title || '未命名案件'}</div>
                  <div class="text-secondary" style="font-size:var(--text-sm);margin-top:2px;">
                    {statusLabels[item.status] ?? item.status} · {item.priority} · 指派：{item.assigned_to ?? '未指派'} · {formatRelative(item.created_at)}
                  </div>
                </div>
                <span class="badge">{item.priority}</span>
              </a>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  {/if}
</div>
