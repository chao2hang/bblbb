<script lang="ts">
  // 状态组件（M00-FRONTEND-05）：按 Problem/status 显示 401/403/404/409/422/429/503
  // 等错误状态；统一走 $lib/errors 的文案映射与 request_id 透传。
  import type { Snippet } from 'svelte';
  import Icon from './ui/Icon.svelte';
  import { problemMessage, requestIdOf, retryAfterOf, type Problem } from '$lib/errors';

  let {
    problem = null,
    status,
    title,
    desc = '',
    showRequestId = true,
    children
  }: {
    problem?: Problem | null;
    status?: number;
    title?: string;
    desc?: string;
    showRequestId?: boolean;
    children?: Snippet;
  } = $props();

  const effectiveStatus = $derived(problem?.status ?? status);

  const TITLES: Record<number, string> = {
    401: '需要登录',
    403: '没有权限',
    404: '内容未找到',
    409: '操作冲突',
    422: '校验未通过',
    429: '操作太频繁',
    500: '服务器错误',
    503: '服务暂不可用'
  };

  const ICONS: Record<number, string> = {
    401: 'lock',
    403: 'shield',
    404: 'search',
    409: 'alert-triangle',
    422: 'alert-triangle',
    429: 'clock',
    500: 'alert-triangle',
    503: 'alert-triangle'
  };

  const iconName = $derived(ICONS[effectiveStatus ?? 0] ?? 'alert-triangle');
  const heading = $derived(title ?? TITLES[effectiveStatus ?? 0] ?? '出错了');
  const message = $derived(desc || problemMessage(problem));
  const requestId = $derived(requestIdOf(problem));
  const retryAfter = $derived(retryAfterOf(problem));
</script>

<div class="problem-state empty-state">
  <div class="empty-state-icon"><Icon name={iconName} size={40} /></div>
  <div class="empty-state-title">{heading}</div>
  <div class="empty-state-desc">{message}</div>
  {#if retryAfter != null}
    <div class="empty-state-desc">请在 {retryAfter} 秒后重试</div>
  {/if}
  {#if showRequestId && requestId}
    <code class="problem-request-id" title="服务端请求号">请求号：{requestId}</code>
  {/if}
  {#if children}{@render children()}{/if}
</div>

<style>
  .problem-request-id {
    display: inline-block;
    margin-top: var(--space-3);
    padding: var(--space-1) var(--space-2);
    border: var(--border-default);
    border-radius: var(--radius-sm);
    background: var(--color-surface-muted, #f5f5f4);
    font-size: var(--text-xs);
    color: var(--color-text-tertiary);
    user-select: all;
  }
</style>
