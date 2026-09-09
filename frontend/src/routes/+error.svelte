<script lang="ts">
  // M02-UX-07：全局错误页（SvelteKit +error.svelte）——load/action 抛错、
  // 未匹配路由等未处理错误的可访问、可恢复兜底。复用 ProblemState
  // （role=alert + 按状态恢复动作 + request ID），与页面内错误一致。
  import { page } from '$app/state';
  import ProblemState from '$lib/components/ProblemState.svelte';
  import { resolveSiteCopy } from '$lib/site/copy';

  interface PageError {
    status?: number;
    message?: string;
  }

  const error = $derived((page.error ?? {}) as PageError);
  // SvelteKit 未匹配路由的默认错误对象是 { message: 'Not Found' }，不带
  // status 字段；直接 ?? 500 会把 404 渲染成「服务器错误」。按 message 兜底。
  const status = $derived(error.status ?? (error.message === 'Not Found' ? 404 : 500));
  const desc = $derived(
    error.message && error.message !== 'Not Found' && error.message !== 'Internal Error'
      ? error.message
      : status === 404
        ? '页面不存在或已被删除。'
        : ''
  );
  // 全站文案（0065）：站点名来自 layout 数据（page.data 合并了 layout load；
  // 根 layout 自身失败时 page.data 为空对象 → resolveSiteCopy 兜底）。
  const siteName = $derived(resolveSiteCopy(page.data?.site ?? null).siteName);
</script>

<svelte:head>
  <title>出错了 — {siteName}</title>
  <!-- M14-SEO-03：错误页（404/403/409/422/429/503）一律 noindex ——
       删除/隐藏内容以 404 呈现，不允许被索引。 -->
  <meta name="robots" content="noindex, noarchive, nofollow" />
</svelte:head>

<main class="container page-content">
  <div style="padding:var(--space-8) 0;">
    <ProblemState {status} {desc} />
  </div>
</main>
