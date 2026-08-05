<script lang="ts">
  // 离线状态组件（M00-FRONTEND-05）：网络不可达时展示，可选重试按钮。
  import Icon from './Icon.svelte';
  import Button from './Button.svelte';

  let {
    title = '网络连接已断开',
    desc = '请检查网络连接后重试',
    onRetry
  }: { title?: string; desc?: string; onRetry?: () => void } = $props();
</script>

<div class="empty-state" role="alert">
  <div class="empty-state-icon"><Icon name="alert-triangle" size={40} /></div>
  <div class="empty-state-title">{title}</div>
  {#if desc}<div class="empty-state-desc">{desc}</div>{/if}
  {#if onRetry}
    <div style="margin-top:var(--space-3);">
      <Button text="重试" variant="secondary" size="sm" onclick={onRetry} />
    </div>
  {/if}
</div>