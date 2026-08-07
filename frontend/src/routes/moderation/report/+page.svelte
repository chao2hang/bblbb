<script lang="ts">
  import { enhance } from '$app/forms';
  import type { PageData } from './$types';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import { formatRelative } from '$lib/utils';

  let { data, form }: { data: PageData; form: any } = $props();

  const submitted = $derived(form?.submitted as { id: string; status: string } | null | undefined);

  const reasons = [
    { value: 'spam', label: '垃圾广告' },
    { value: 'harassment', label: '骚扰谩骂' },
    { value: 'illegal', label: '违法违规' },
    { value: 'nsfw', label: '色情不当' },
    { value: 'misinformation', label: '不实信息' },
    { value: 'impersonation', label: '冒充他人' },
    { value: 'other', label: '其他' }
  ];

  const targetTypes = [
    { value: 'post', label: '帖子' },
    { value: 'comment', label: '评论' },
    { value: 'user', label: '用户' },
    { value: 'board', label: '板块' }
  ];
</script>

<svelte:head>
  <title>举报 — BBLBB</title>
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">举报</span>
  </nav>

  <h1 class="page-title">举报内容</h1>

  {#if submitted}
    <div class="card" role="status" data-testid="report-success">
      <div class="card-header"><span class="card-title">举报已提交</span></div>
      <div class="card-body">
        <p>举报 <code>{submitted.id}</code> 已受理，当前状态：{submitted.status}。感谢你的反馈。</p>
        <p class="text-secondary">若为误报，可在下方“我的举报”中撤回。</p>
      </div>
    </div>
  {:else}
    <div class="card">
      <div class="card-header"><span class="card-title">提交举报</span></div>
      <div class="card-body">
        <form method="POST" action="?/report" use:enhance class="stack">
          <div class="field-grid" style="display:grid;grid-template-columns:1fr 1fr;gap:var(--space-3);">
            <label>
              <span class="field-label">目标类型</span>
              <select name="target_type" required>
                {#each targetTypes as t}
                  <option value={t.value}>{t.label}</option>
                {/each}
              </select>
            </label>
            <label>
              <span class="field-label">目标 ID</span>
              <input name="target_id" type="text" required placeholder="帖子/评论/用户 ID" />
            </label>
          </div>
          <label>
            <span class="field-label">举报原因</span>
            <select name="reason" required>
              {#each reasons as r}
                <option value={r.value}>{r.label}</option>
              {/each}
            </select>
          </label>
          <label>
            <span class="field-label">补充说明（可选，最多 2000 字）</span>
            <textarea name="detail" rows="4" maxlength="2000"></textarea>
          </label>
          {#if form?.message}
            <p class="form-error" role="alert" data-testid="report-error">{form.message}</p>
          {/if}
          <button type="submit" class="btn btn-primary">提交举报</button>
        </form>
      </div>
    </div>
  {/if}

  <div class="card" style="margin-top:var(--space-5);">
    <div class="card-header"><span class="card-title">我的举报</span></div>
    <div class="card-body" style="padding:0;">
      {#if data.items.length === 0}
        <EmptyState icon="flag" title="暂无举报" desc="你提交的举报会显示在这里" />
      {:else}
        <div style="display:flex;flex-direction:column;">
          {#each data.items as item}
            <div class="post-row" style="padding:var(--space-4);border-bottom:var(--border-default);display:flex;gap:var(--space-3);align-items:center;justify-content:space-between;">
              <div style="min-width:0;">
                <div><span class="badge">{item.target_type}</span> <code>{item.target_id}</code></div>
                <div class="text-secondary" style="font-size:var(--text-sm);margin-top:2px;">
                  原因：{item.reason_code} · 状态：{item.status} · {formatRelative(item.created_at)}
                </div>
              </div>
              {#if item.status !== 'withdrawn'}
                <form method="POST" action="?/withdraw" use:enhance>
                  <input type="hidden" name="report_id" value={item.id} />
                  <button type="submit" class="btn btn-secondary btn-sm">撤回</button>
                </form>
              {/if}
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>
</div>
