<script lang="ts">
  import Icon from './Icon.svelte';

  let { icon = 'inbox', title = '暂无内容', desc = '' }: { icon?: string; title?: string; desc?: string } = $props();
</script>

<div class="empty-state">
  <div class="empty-state-icon"><Icon name={icon} size={40} /></div>
  <div class="empty-state-title">{title}</div>
  {#if desc}<div class="empty-state-desc">{desc}</div>{/if}
</div>
