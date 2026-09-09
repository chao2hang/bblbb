<script lang="ts">
  import Icon from './Icon.svelte';

  let {
    message,
    detail,
    type = 'info',
    onclose
  }: {
    message: string;
    detail?: string;
    type?: 'info' | 'success' | 'warning' | 'danger';
    onclose?: () => void;
  } = $props();

  const icons: Record<string, string> = { info: 'info', success: 'check-circle', warning: 'alert-triangle', danger: 'alert-triangle' };
</script>

<div class="toast toast-{type}" role="status">
  <span class="toast-icon"><Icon name={icons[type]} size={18} /></span>
  <span class="toast-body">
    <span class="toast-message">{message}</span>
    {#if detail}<span class="toast-detail">{detail}</span>{/if}
  </span>
  {#if onclose}
    <button type="button" class="toast-close" aria-label="关闭" onclick={onclose}><Icon name="x" size={14} /></button>
  {/if}
</div>
