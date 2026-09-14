<script lang="ts">
  // 真实渲染 blbui StatusTag；custom elements 由根布局一次性注册。
  import type { AdminStatus } from '@chaos_team/blbui-core';

  let { text, type = 'neutral' }: { text: string; type?: string } = $props();

  const auiStatus = $derived((): AdminStatus => {
    switch (type) {
      case 'success':
        return 'success';
      case 'danger':
        return 'danger';
      case 'warning':
        return 'warning';
      case 'info':
      case 'pinned':
        return 'info';
      default:
        return 'default';
    }
  });
</script>

<span class="badge badge-{type}">
  <aui-status-tag status={auiStatus()}>{text}</aui-status-tag>
</span>
