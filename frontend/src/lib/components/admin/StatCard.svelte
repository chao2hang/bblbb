<script lang="ts">
  // GAP-FIX（M17-GAPFIX-07·组件封装）：统计卡（原型 .app-stat 同构）。
  // 替代各页重复的 `.app-stat` markup；note 为底部补充说明（增量/状态）。
  import Icon from '$lib/components/ui/Icon.svelte';

  let {
    value,
    label,
    icon = '',
    note = ''
  }: { value: string | number; label: string; icon?: string; note?: string } = $props();
</script>

<div class="app-stat">
  <b>{value}</b>
  <span>{#if icon}<Icon name={icon} size={13} />{/if}{label}</span>
  {#if note}<em>{note}</em>{/if}
</div>
