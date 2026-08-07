<script lang="ts">
  // M09-UI-03：AI 任务状态页——排队/处理中（可取消）/成功/取消/失败状态。
  // - SSR 渲染服务端任务投影；客户端对处理中任务自动轮询刷新（增强，
  //   无 JS 时显示静态状态并可用原生取消表单）；
  // - 错误只展示稳定码与脱敏信息；成功时链接建议详情。
  import { onDestroy, onMount } from 'svelte';
  import { page } from '$app/state';
  import { getAiTask, aiTaskStatusLabel, aiPurposeLabel } from '$lib/api/client';
  import Button from '$lib/components/ui/Button.svelte';
  import type { AiTask } from '$lib/api/types';
  import type { AiTaskActionData, AiTaskPageData } from './+page.server';

  let { data, form }: { data: AiTaskPageData; form?: AiTaskActionData | null } = $props();

  const task = $derived(data.task);
  const forbidden = $derived(data.forbidden);
  const notFound = $derived(data.notFound);
  const error = $derived(data.error);
  const message = $derived(form?.message ?? null);
  const cancelOk = $derived(form?.ok === true);

  // 客户端轮询（增强）：处理中状态自动刷新。
  let liveStatus = $state<AiTask['status'] | null>(null);
  let pollTimer: ReturnType<typeof setInterval> | null = null;
  let disposed = false;

  onMount(() => {
    if (!task || !isProcessing(task.status)) return;
    pollTimer = setInterval(async () => {
      if (disposed) return;
      try {
        const t = await getAiTask(fetch, task.id);
        liveStatus = t.status;
        if (!isProcessing(t.status) && pollTimer) {
          clearInterval(pollTimer);
          pollTimer = null;
        }
      } catch {
        // 轮询失败保持现有状态。
      }
    }, 2000);
    return () => {
      disposed = true;
      if (pollTimer) clearInterval(pollTimer);
    };
  });

  onDestroy(() => {
    disposed = true;
    if (pollTimer) clearInterval(pollTimer);
  });

  const displayStatus = $derived(liveStatus ?? task?.status ?? null);

  function isProcessing(s: AiTask['status'] | null | undefined): boolean {
    return s === 'queued' || s === 'running' || s === 'retry_wait';
  }

  const statusBadge = $derived.by(() => {
    switch (displayStatus) {
      case 'queued':
        return 'badge-neutral';
      case 'running':
        return 'badge-neutral';
      case 'retry_wait':
        return 'badge-warning';
      case 'succeeded':
        return 'badge-success';
      case 'cancelled':
        return 'badge-neutral';
      case 'dead':
        return 'badge-warning';
      default:
        return 'badge-neutral';
    }
  });
</script>

<svelte:head>
  <title>AI 任务 — BBLBB</title>
  <meta name="robots" content="noindex,follow" />
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <a href="/ai" class="breadcrumb-link">AI 能力</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">任务</span>
  </nav>

  {#if message}
    <p class="input-hint {cancelOk ? '' : 'is-error'}" role="alert">{message}</p>
  {/if}

  {#if notFound}
    <div class="card">
      <div class="card-body">
        <p class="input-hint" style="margin:0;">任务不存在或已被移除。</p>
      </div>
    </div>
  {:else if forbidden}
    <div class="card">
      <div class="card-body">
        <p class="input-hint is-error" role="alert" style="margin:0;">你没有权限查看该任务。</p>
      </div>
    </div>
  {:else if error}
    <div class="card">
      <div class="card-body">
        <p class="input-hint is-error" role="alert" style="margin:0;">{error}</p>
        <a class="btn btn-secondary btn-sm" style="margin-top:var(--space-2);" href={page.url.pathname}>重试</a>
      </div>
    </div>
  {:else if task}
    <div class="card">
      <div class="card-header"><span class="card-title">AI 任务 {task.id.slice(0, 8)}</span></div>
      <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-3);">
        <div style="display:flex;flex-wrap:wrap;gap:var(--space-2);align-items:center;">
          <span class="badge {statusBadge}" role="status">{aiTaskStatusLabel(displayStatus ?? undefined)}</span>
          <span class="text-secondary" style="font-size:var(--text-sm);">{aiPurposeLabel(task.task_type)}</span>
          {#if task.source_revision}
            <span class="text-secondary" style="font-size:var(--text-sm);">内容版本 v{task.source_revision}</span>
          {/if}
          {#if task.policy_version}
            <span class="text-secondary" style="font-size:var(--text-sm);">策略版本 v{task.policy_version}</span>
          {/if}
        </div>

        <dl class="profile-about-list" style="margin:0;">
          <div class="profile-about-item"><dt>创建时间</dt><dd>{new Date(task.created_at).toLocaleString()}</dd></div>
          {#if task.started_at}
            <div class="profile-about-item"><dt>开始时间</dt><dd>{new Date(task.started_at).toLocaleString()}</dd></div>
          {/if}
          {#if task.finished_at}
            <div class="profile-about-item"><dt>结束时间</dt><dd>{new Date(task.finished_at).toLocaleString()}</dd></div>
          {/if}
        </dl>

        {#if isProcessing(displayStatus)}
          <!-- 处理中：原生取消表单（无 JS 可提交）。 -->
          <div role="status" style="display:flex;flex-wrap:wrap;gap:var(--space-2);align-items:center;">
            <span class="text-secondary" style="font-size:var(--text-sm);">任务处理中，结果只生成版本化建议。</span>
            <form method="POST" action="?/cancel">
              <input type="hidden" name="client_request_id" value={data.clientRequestId} />
              <Button text="取消任务" variant="ghost" size="sm" type="submit" />
            </form>
          </div>
        {:else if displayStatus === 'succeeded' && task.suggestion_id}
          <div>
            <a class="btn btn-primary btn-sm" href="/ai/suggestions/{task.suggestion_id}">查看建议</a>
          </div>
        {:else if displayStatus === 'cancelled'}
          <p class="input-hint" role="status" style="margin:0;">任务已取消，不再生成建议。</p>
        {:else if displayStatus === 'dead'}
          <p class="input-hint is-error" role="alert" style="margin:0;">
            任务失败：{task.error_code ? `（${task.error_code}）` : ''}{task.error_message ?? '未知原因'}
          </p>
        {/if}

        <p class="input-hint" style="margin:0;">任务错误只显示稳定码与脱敏信息；Provider 响应原文与内部 Prompt 不会展示。</p>
      </div>
    </div>
  {/if}
</div>
