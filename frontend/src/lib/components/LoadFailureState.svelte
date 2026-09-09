<script lang="ts">
  // 瞬态服务端错误（5xx/429）的页面占位：中性「加载失败 + 重试」卡。
  // 产品约定——这类错误用全局 Toast 提示（announceTransientProblem），
  // 页面主体只保留本占位，不整页渲染错误态（ProblemState 留给
  // 401/403/404/409/422 等持续性错误）。无 JS 基线下本卡即可见（重试=整页刷新）。
  import Button from './ui/Button.svelte';
  import EmptyState from './ui/EmptyState.svelte';

  let {
    title = '加载失败',
    desc = '内容暂时没有加载出来，请稍后重试',
    onretry
  }: { title?: string; desc?: string; onretry?: () => void } = $props();
</script>

<div class="card load-failure-state">
  <div class="card-body">
    <EmptyState icon="alert-triangle" {title} {desc} />
    {#if onretry}
      <div class="load-failure-actions">
        <Button text="重试" variant="secondary" size="sm" onclick={() => onretry()} />
      </div>
    {/if}
  </div>
</div>

<style>
  .load-failure-actions {
    display: flex;
    justify-content: center;
    margin-top: var(--space-3);
  }
</style>
