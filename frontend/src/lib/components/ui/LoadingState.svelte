<script lang="ts">
  // 加载状态组件（M00-FRONTEND-05）：统一「加载中」占位，带 aria-live 播报。
  let { title = '加载中…', desc = '' }: { title?: string; desc?: string } = $props();
</script>

<div class="empty-state" role="status" aria-live="polite">
  <div class="loading-spinner" aria-hidden="true"></div>
  <div class="empty-state-title">{title}</div>
  {#if desc}<div class="empty-state-desc">{desc}</div>{/if}
</div>

<style>
  .loading-spinner {
    width: 28px;
    height: 28px;
    margin-bottom: var(--space-2);
    border: 3px solid var(--color-border-strong);
    border-top-color: var(--color-brand, var(--color-accent));
    border-radius: 50%;
    animation: bblbb-spin 0.8s linear infinite;
  }

  @keyframes bblbb-spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>