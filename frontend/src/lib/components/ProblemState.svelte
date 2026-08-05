<script lang="ts">
  // 状态组件（M00-FRONTEND-05 / M02-UX-07）：按 Problem/status 显示
  // 401/403/404/409/422/429/503 等错误状态。
  // - 可访问：容器 role=alert（屏幕阅读器播报），request ID 可复制；
  // - 可恢复：按状态给出默认恢复动作（去登录/返回首页/刷新/返回上一页），
  //   429 显示 Retry-After 秒数；页面可用 children 槽自定义动作（覆盖默认）。
  import type { Snippet } from 'svelte';
  import Button from './ui/Button.svelte';
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

  /** 按状态码给出默认恢复动作（有 children 时由页面自定义，覆盖默认）。 */
  function refresh(): void {
    window.location.reload();
  }
  function goBack(): void {
    window.history.back();
  }
</script>

<div class="problem-state empty-state" role="alert">
  <div class="empty-state-icon"><Icon name={iconName} size={40} /></div>
  <div class="empty-state-title">{heading}</div>
  <div class="empty-state-desc">{message}</div>
  {#if retryAfter != null}
    <div class="empty-state-desc">请在 {retryAfter} 秒后重试</div>
  {/if}
  {#if showRequestId && requestId}
    <code class="problem-request-id" title="服务端请求号" aria-label="服务端请求号：{requestId}">请求号：{requestId}</code>
  {/if}

  {#if children}
    {@render children()}
  {:else if effectiveStatus === 401}
    <div class="problem-actions">
      <Button text="去登录" variant="primary" size="sm" href="/login" />
      <Button text="返回首页" variant="ghost" size="sm" href="/" />
    </div>
  {:else if effectiveStatus === 403}
    <div class="problem-actions">
      <Button text="返回首页" variant="primary" size="sm" href="/" />
      <Button text="返回上一页" variant="ghost" size="sm" onclick={goBack} />
    </div>
  {:else if effectiveStatus === 404}
    <div class="problem-actions">
      <Button text="返回首页" variant="primary" size="sm" href="/" />
    </div>
  {:else if effectiveStatus === 409}
    <div class="problem-actions">
      <Button text="刷新页面" variant="primary" size="sm" onclick={refresh} />
    </div>
  {:else if effectiveStatus === 422}
    <div class="problem-actions">
      <Button text="返回上一页" variant="primary" size="sm" onclick={goBack} />
    </div>
  {:else if effectiveStatus === 429}
    <div class="problem-actions">
      <Button text="刷新页面" variant="primary" size="sm" onclick={refresh} />
    </div>
  {:else if effectiveStatus === 503}
    <div class="problem-actions">
      <Button text="稍后重试" variant="primary" size="sm" onclick={refresh} />
      <Button text="返回首页" variant="ghost" size="sm" href="/" />
    </div>
  {:else if effectiveStatus === 500}
    <div class="problem-actions">
      <Button text="刷新页面" variant="primary" size="sm" onclick={refresh} />
      <Button text="返回首页" variant="ghost" size="sm" href="/" />
    </div>
  {/if}
</div>

<style>
  .problem-actions {
    display: flex;
    gap: var(--space-2);
    justify-content: center;
    margin-top: var(--space-3);
  }
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
