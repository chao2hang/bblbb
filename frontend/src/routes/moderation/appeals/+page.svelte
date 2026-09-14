<script lang="ts">
  import { enhance } from '$app/forms';
  import type { AppealsActionData, AppealsPageData } from './+page.server';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import { formatRelative } from '$lib/utils';
  import PageTitle from '$lib/components/PageTitle.svelte';

  let { data, form }: { data: AppealsPageData; form?: AppealsActionData | null } = $props();

  const submitted = $derived(form?.submitted as import('$lib/api/types').OwnAppeal | null | undefined);

  const statusLabels: Record<string, string> = {
    submitted: '待复核',
    reviewing: '复核中',
    upheld: '已支持',
    partially_upheld: '部分支持',
    rejected: '已驳回',
    withdrawn: '已撤回'
  };
  let targetSanctionId = $state('');

  const sanctionKindLabels: Record<string, string> = {
    warning: '警告',
    rate_limit: '限流',
    mute: '禁言',
    board_mute: '板块禁言',
    ban: '封禁',
    suspend: '暂停'
  };

  const reportStatusLabels: Record<string, string> = {
    pending: '待处理',
    triaged: '已分级',
    investigating: '调查中',
    resolved: '已处理',
    dismissed: '已驳回'
  };
</script>

  <PageTitle title="申诉中心" />

<div class="container page-content">
  <h1 class="u-visually-hidden">申诉中心</h1>

  <!-- M18-APPEAL-02：第一区块「我相关的处罚案件」（对齐原型） -->
  <section class="card" style="margin-top:var(--space-4);">
    <div class="card-header"><span class="card-title">我相关的处罚案件</span></div>
    <div class="card-body" style="padding:0;">
      {#if data.sanctions.length === 0}
        <EmptyState icon="shield" title="没有处罚案件" desc="保持记录干净，继续参与社区" />
      {:else}
        <div style="display:flex;flex-direction:column;">
          {#each data.sanctions as s}
            <div
              class="post-row"
              style="padding:var(--space-4);border-bottom:var(--border-default);display:flex;gap:var(--space-3);align-items:center;justify-content:space-between;"
            >
              <div style="min-width:0;">
                <div>
                  <span class="badge badge-danger">{sanctionKindLabels[s.kind] ?? s.kind}</span>
                  <code>{s.id}</code>
                  {#if s.case_id}
                    <span class="text-tertiary" style="font-size:var(--text-xs);margin-left:var(--space-2);">案件 #{s.case_id.slice(0, 8)}</span>
                  {/if}
                </div>
                {#if s.reason}
                  <div class="text-secondary" style="font-size:var(--text-sm);margin-top:2px;">处罚原因：{s.reason}</div>
                {/if}
                <div class="text-tertiary" style="font-size:var(--text-xs);margin-top:2px;">生效于 {formatRelative(s.created_at)}</div>
              </div>
              <button
                type="button"
                class="btn btn-secondary btn-sm"
                onclick={() => (targetSanctionId = s.id)}
              >
                申诉此项
              </button>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </section>

  <!-- 第二区块：提交申诉表单 -->
  <div class="card" style="margin-top:var(--space-4);">
    <div class="card-header"><span class="card-title">提交申诉</span></div>
    <div class="card-body">
      <form method="POST" action="?/create" use:enhance class="stack">
        <label>
          <span class="field-label">处罚 ID（sanction_id）</span>
          <input name="sanction_id" type="text" required placeholder="处罚通知中的 ID" bind:value={targetSanctionId} />
        </label>
        <label>
          <span class="field-label">申诉内容（1–5000 字，禁止附件引用）</span>
          <textarea name="content" rows="4" maxlength="5000" required placeholder="请说明你希望复核的事实与理由…"></textarea>
        </label>
        {#if form?.message}
          <p class="form-error" role="alert" data-testid="appeal-error">{form.message}</p>
        {/if}
        <button type="submit" class="btn btn-primary">提交申诉</button>
      </form>
    </div>
  </div>

  <!-- M18-APPEAL-02：第三区块「我提交的举报」（对齐原型） -->
  <section class="card" style="margin-top:var(--space-4);">
    <div class="card-header"><span class="card-title">我提交的举报</span></div>
    <div class="card-body" style="padding:0;">
      {#if data.reports.length === 0}
        <EmptyState icon="inbox" title="还没有提交过举报" desc="在帖子或回复的「举报」按钮提交，处理进度会显示在这里" />
      {:else}
        <div style="display:flex;flex-direction:column;">
          {#each data.reports as r}
            <div
              class="post-row"
              style="padding:var(--space-4);border-bottom:var(--border-default);display:flex;gap:var(--space-3);align-items:center;justify-content:space-between;"
            >
              <div style="min-width:0;">
                <div>
                  <span class="badge badge-neutral">{reportStatusLabels[r.status] ?? r.status}</span>
                  <strong>举报 #{r.id.slice(0, 8)}</strong>
                </div>
                <div class="text-secondary" style="font-size:var(--text-sm);margin-top:2px;">对象：{r.target_type} · 理由：{r.reason_code}</div>
                <div class="text-tertiary" style="font-size:var(--text-xs);margin-top:2px;">提交于 {formatRelative(r.created_at)}</div>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </section>

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
