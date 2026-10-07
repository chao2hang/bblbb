<script lang="ts">
  import { validateJumpTarget } from '$lib/link/interceptor';
  import { FALLBACK_SITE_NAME } from '$lib/site/copy';
  import type { PageData } from './$types';

  // data 在 SvelteKit 运行时恒有（PageData）；测试隔离渲染时允许部分或缺省。
  let { data }: { data?: Partial<PageData> } = $props();

  const targetParam = $derived(data?.target ?? '');
  const validation = $derived(validateJumpTarget(targetParam));
  const targetUrl = $derived(validation.url);

  const siteBrandTitle = $derived(data?.site?.siteName?.trim() || FALLBACK_SITE_NAME);
  const warningSiteName = $derived(siteBrandTitle);
</script>

<svelte:head>
  <title>跳转确认 — {siteBrandTitle}</title>
  <meta name="robots" content="noindex, nofollow" />
</svelte:head>

<div class="link-page-container">
  <!-- 顶栏 Logo -->
  <header class="link-page-header">
    <a href="/" class="link-brand-link" title="返回首页">
      <span class="link-brand-icon" aria-hidden="true">↗</span>
      <span class="link-brand-text">{siteBrandTitle}</span>
    </a>
  </header>

  <!-- 中间主体区域 -->
  <section class="link-page-main" aria-labelledby="link-heading">
    <div class="link-visual-wrapper">
      <div class="link-visual-icon" aria-hidden="true">↗</div>
    </div>

    <!-- 文本与操作按纽 -->
    {#if validation.valid}
      <h1 id="link-heading" class="link-heading">即将离开 {warningSiteName}，请注意账号和财产安全</h1>
      <div class="link-url-text" title={targetUrl}>{targetUrl}</div>
      <div class="link-action-group">
        <a
          href={targetUrl}
          class="link-proceed-btn link-confirm-proceed"
          data-direct-jump="true"
          rel="noopener noreferrer nofollow"
          target="_blank"
        >
          继续访问
        </a>
      </div>
    {:else}
      <h1 id="link-heading" class="link-heading">
        {#if validation.reason === 'unsafe_scheme'}
          不安全的链接协议
        {:else if validation.reason === 'malformed'}
          无效的跳转链接格式
        {:else}
          缺少跳转目标链接
        {/if}
      </h1>
      <div class="link-url-text link-url-error">
        {#if validation.reason === 'unsafe_scheme'}
          出于安全考量，系统仅允许访问以 http:// 或 https:// 开头的网络地址。
        {:else if validation.reason === 'malformed'}
          提供的目标网址格式无法解析，已终止重定向。
        {:else}
          未检测到需要跳转的外部地址，请确认链接是否完整。
        {/if}
      </div>
      <div class="link-action-group">
        <a href="/" class="link-proceed-btn link-home-btn">
          返回首页
        </a>
      </div>
    {/if}
  </section>

  <footer class="link-page-footer">
    <a href="/" class="link-footer-item">返回 {siteBrandTitle}</a>
  </footer>
</div>

<style>
  .link-page-container {
    box-sizing: border-box;
    width: 100%;
    min-height: 100vh;
    min-height: 100dvh;
    background-color: var(--color-bg-page);
    color: var(--color-text-primary);
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    font-family: var(--font-family-base);
    padding: 24px 32px;
  }

  /* 顶栏 */
  .link-page-header {
    display: flex;
    align-items: center;
    width: 100%;
  }

  .link-brand-link {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    text-decoration: none;
    color: var(--color-text-primary);
    transition: opacity 0.2s;
  }

  .link-brand-link:hover {
    opacity: 0.9;
  }

  .link-brand-icon {
    display: inline-grid;
    place-items: center;
    flex-shrink: 0;
    width: 26px;
    height: 26px;
    border: 1px solid var(--color-border);
    border-radius: 50%;
    background: var(--color-bg-card);
    color: var(--color-text-secondary);
    font-size: 18px;
  }

  .link-brand-text {
    font-size: 16px;
    font-weight: 700;
    letter-spacing: 0.2px;
    color: var(--color-text-primary);
  }

  /* 主体区域 */
  .link-page-main {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 40px 16px;
    text-align: center;
  }

  .link-visual-wrapper {
    width: 240px;
    height: 170px;
    display: flex;
    align-items: center;
    justify-content: center;
    margin-bottom: 24px;
    user-select: none;
  }

  .link-visual-icon {
    display: grid;
    place-items: center;
    width: 112px;
    height: 112px;
    border: 1px solid var(--color-border);
    border-radius: 50%;
    background: var(--color-bg-card);
    color: var(--color-brand);
    font-size: 64px;
    line-height: 1;
  }

  .link-heading {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
    color: var(--color-text-primary);
    letter-spacing: 0.3px;
    line-height: 1.5;
  }

  .link-url-text {
    margin-top: 10px;
    font-size: 13px;
    color: var(--color-text-secondary);
    word-break: break-all;
    max-width: min(640px, 90vw);
    line-height: 1.5;
  }

  .link-url-error {
    color: var(--color-danger);
  }

  .link-action-group {
    margin-top: 24px;
  }

  .link-proceed-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background-color: var(--color-brand);
    color: var(--color-text-on-brand);
    font-size: 14px;
    font-weight: 500;
    padding: 8px 30px;
    border-radius: var(--radius-sm);
    text-decoration: none;
    cursor: pointer;
    transition: background-color 0.2s, box-shadow 0.2s, transform 0.1s;
    outline: none;
  }

  .link-proceed-btn:hover {
    background-color: var(--color-brand-hover);
    box-shadow: var(--shadow-control);
  }

  .link-proceed-btn:active {
    transform: scale(0.98);
  }

  .link-proceed-btn:focus-visible {
    box-shadow: var(--color-focus-ring);
  }

  .link-home-btn {
    background-color: var(--color-bg-subtle);
    color: var(--color-text-primary);
    border: 1px solid var(--color-border);
  }

  .link-home-btn:hover {
    background-color: var(--color-surface-hover);
    box-shadow: var(--shadow-control);
  }

  /* 底栏 */
  .link-page-footer {
    display: flex;
    align-items: center;
    justify-content: center;
    padding-top: 24px;
    font-size: 14px;
    width: 100%;
    box-sizing: border-box;
  }

  .link-footer-item {
    color: var(--color-text-secondary);
    text-decoration: underline;
    text-underline-offset: 0.2em;
    transition: color 0.2s;
  }

  .link-footer-item:hover {
    color: var(--color-text-primary);
  }

  /* 移动端适配 */
  @media (max-width: 768px) {
    .link-page-container {
      padding: 16px 20px;
    }

    .link-page-footer {
      flex-direction: column;
      align-items: center;
      text-align: center;
      gap: 14px;
    }

  }
</style>
