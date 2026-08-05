<script lang="ts">
  // M02-UX-07：全局错误页（SvelteKit +error.svelte）——load/action 抛错、
  // 未匹配路由等未处理错误的可访问、可恢复兜底。复用 ProblemState
  // （role=alert + 按状态恢复动作 + request ID），与页面内错误一致。
  import { page } from '$app/state';
  import ProblemState from '$lib/components/ProblemState.svelte';

  interface PageError {
    status?: number;
    message?: string;
  }

  const error = $derived((page.error ?? {}) as PageError);
  const status = $derived(error.status ?? 500);
  const desc = $derived(
    error.message && error.message !== 'Not Found' && error.message !== 'Internal Error'
      ? error.message
      : ''
  );
</script>

<svelte:head>
  <title>出错了 — BBLBB</title>
</svelte:head>

<main class="container page-content">
  <div style="padding:var(--space-8) 0;">
    <ProblemState {status} {desc} />
  </div>
</main>
