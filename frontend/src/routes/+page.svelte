<script lang="ts">
  import { onMount } from 'svelte';
  import { fetchHealth } from '$lib/api/client';

  let apiStatus = '检查中…';

  onMount(async () => {
    try {
      const health = await fetchHealth();
      apiStatus = health.status ?? '服务正常';
    } catch {
      apiStatus = 'API 暂不可用';
    }
  });
</script>

<svelte:head>
  <title>BBLBB</title>
  <meta name="description" content="BBLBB SvelteKit 前端基础骨架" />
</svelte:head>

<section class="intro" aria-labelledby="page-title">
  <p class="eyebrow">基础工程</p>
  <h1 id="page-title">BBLBB 前端已就绪</h1>
  <p class="summary">这是面向同源 SSR/API 的最小 SvelteKit 应用。具体业务页面将在后续按契约逐步接入。</p>

  <div class="status" aria-live="polite">
    <span class="dot" aria-hidden="true"></span>
    <span>API 状态：{apiStatus}</span>
  </div>
</section>

<style>
  .intro { background: #fff; border: 1px solid #e5e7eb; border-radius: 0.75rem; padding: clamp(1.5rem, 5vw, 3rem); }
  .eyebrow { color: #4f46e5; font-size: 0.8rem; font-weight: 700; letter-spacing: 0.12em; margin: 0 0 0.75rem; text-transform: uppercase; }
  h1 { font-size: clamp(2rem, 6vw, 3.5rem); line-height: 1.1; margin: 0; }
  .summary { color: #4b5563; line-height: 1.7; margin: 1.25rem 0 2rem; max-width: 42rem; }
  .status { align-items: center; background: #f9fafb; border-radius: 0.5rem; display: inline-flex; gap: 0.6rem; padding: 0.75rem 1rem; }
  .dot { background: #f59e0b; border-radius: 50%; height: 0.65rem; width: 0.65rem; }
</style>
