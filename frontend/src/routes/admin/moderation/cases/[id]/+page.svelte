<script lang="ts">
  // M18-ADMIN-REPORTS：案件详情页（对齐原型 #admin-report:id 5 大卡片结构）。
  import { enhance } from '$app/forms';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import { formatRelative } from '$lib/utils';
  import type { PageData } from './$types';

  let { data, form }: { data: PageData; form: any } = $props();

  const caseItem = $derived(data.caseItem);
  const okMessage = $derived(form?.ok as string | undefined);

  let penaltyAction = $state<'hide' | 'mute' | 'dismiss'>('mute');
  let reasonText = $state('');

  const statusLabels: Record<string, string> = {
    open: '待处理',
    triaged: '处理中',
    investigating: '调查中',
    resolved: '已解决',
    rejected: '已驳回',
    reopened: '已重开'
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
      {#if form?.message}<p class="form-error" role="alert">{form.message}</p>{/if}
      <EmptyState icon="inbox" title="未找到案件" desc="该案件不存在或当前角色无权查看" />
    </div>
  </div>
{:else}
  {#if form?.message}<div class="alert alert-danger" role="alert" style="margin-bottom:12px;">{form.message}</div>{/if}
  {#if okMessage}<div class="alert alert-success" role="status" style="margin-bottom:12px;">{okMessage}</div>{/if}

  <!-- 卡片 1：原内容预览 -->
  <section class="app-card" style="margin-bottom:14px;">
    <header class="app-card__head">
      <h2>原内容预览</h2>
    </header>
    <div class="app-card__body">
      <div style="background:var(--color-bg-subtle, rgba(0,0,0,0.03));padding:14px 16px;border-left:3px solid var(--color-text-secondary);border-radius:0 var(--radius-sm) var(--radius-sm) 0;margin-bottom:12px;">
        <p style="margin:0;font-size:14px;line-height:1.6;color:var(--color-text-primary);">
          {caseItem.title || '【违规内容】某商业产品测试垃圾广告内容！现在购买 8 折优惠…'}
        </p>
      </div>
      <a href="/posts" target="_blank" class="text-link" style="font-size:12px;">查看原帖</a>
    </div>
  </section>

  <!-- 卡片 2：举报原因与处罚 -->
  <section class="app-card" style="margin-bottom:14px;">
    <header class="app-card__head">
      <h2>举报原因与处罚</h2>
    </header>
    <div class="app-card__body">
      <div style="font-size:13px;color:var(--color-text-secondary);margin-bottom:12px;">
        举报人 Yuwen · 原因：<b>广告 / 垃圾信息</b>
      </div>

      <form method="POST" action="?/transition" use:enhance style="display:flex;flex-direction:column;gap:12px;">
        <!-- 隐式映射到后端的 status 与 resolution -->
        <input type="hidden" name="status" value={penaltyAction === 'dismiss' ? 'rejected' : 'resolved'} />
        
        <div>
          <label class="input-label" for="case-reason" style="font-size:13px;margin-bottom:6px;display:block;">
            处理原因 <span style="color:var(--color-danger);">*</span>
          </label>
          <textarea
            id="case-reason"
            name="resolution"
            class="input-field"
            rows="3"
            placeholder="必填，写入审计日志"
            required
            bind:value={reasonText}
            style="width:100%;font-size:13px;"
          ></textarea>
        </div>

        <div style="display:grid;grid-template-columns:repeat(3, 1fr);gap:8px;">
          <button
            type="button"
            class="btn sm {penaltyAction === 'hide' ? 'secondary' : 'ghost'}"
            style={penaltyAction === 'hide' ? 'border:1px solid var(--color-brand);font-weight:600;' : ''}
            onclick={() => (penaltyAction = 'hide')}
          >
            隐藏内容
          </button>
          <button
            type="button"
            class="btn sm {penaltyAction === 'mute' ? 'secondary' : 'ghost'}"
            style={penaltyAction === 'mute' ? 'border:1px solid var(--color-brand);font-weight:600;' : ''}
            onclick={() => (penaltyAction = 'mute')}
          >
            禁言 7 天
          </button>
          <button
            type="button"
            class="btn sm {penaltyAction === 'dismiss' ? 'secondary' : 'ghost'}"
            style={penaltyAction === 'dismiss' ? 'border:1px solid var(--color-brand);font-weight:600;' : ''}
            onclick={() => (penaltyAction = 'dismiss')}
          >
            驳回举报
          </button>
        </div>

        <div style="margin-top:4px;">
          <Button text="提交处理" variant="primary" type="submit" block />
        </div>
      </form>
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
          <span>自动分配给 {caseItem.assigned_to ?? 'Chaos'} · {formatRelative(caseItem.updated_at)}</span>
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
        <span class="text-secondary">板块</span>
        <span>rust</span>
      </div>
      <div style="display:flex;gap:8px;">
        <span class="text-secondary">负责人</span>
        <span>{caseItem.assigned_to ?? 'Chaos'}</span>
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
{/if}
