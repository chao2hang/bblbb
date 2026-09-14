<script lang="ts">
  // 统计项保留原生 SSR 标记，并消费 blbui --aui-* token；aui-stat 的
  // shadow DOM 不提供图标/无 JS 退化，因此不再渲染隐藏宿主。
  import Icon from '$lib/components/ui/Icon.svelte';

  let {
    value,
    label,
    icon = '',
    note = '',
    unit = '',
    trend = '',
    tone = ''
  }: {
    value: string | number;
    label: string;
    icon?: string;
    note?: string;
    unit?: string;
    trend?: string;
    tone?: string;
  } = $props();

  const detail = $derived(trend || note);
</script>

<div class="app-stat" data-tone={tone || undefined}>
  <b>{value}{#if unit}<small>{unit}</small>{/if}</b>
  <span>{#if icon}<Icon name={icon} size={13} />{/if}{label}</span>
  {#if detail}<em>{detail}</em>{/if}
</div>
