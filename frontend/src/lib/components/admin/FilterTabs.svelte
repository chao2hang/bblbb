<script lang="ts">
  // GAP-FIX（M17-GAPFIX-07·组件封装）：链接式筛选 Tab（原型 app-filter-tabs
  // 同构）——每项为链接（无 JS 可切换），支持计数后缀。
  let {
    ariaLabel = '',
    tabs,
    style = 'margin-bottom:14px;'
  }: {
    ariaLabel?: string;
    /** value 仅作 key；active 控制高亮；count 有值时显示数字后缀。 */
    tabs: Array<{ value: string; label: string; href: string; active?: boolean; count?: number }>;
    style?: string;
  } = $props();
</script>

<div class="app-filter-tabs" aria-label={ariaLabel} style={style}>
  {#each tabs as tab (tab.value)}
    <a
      href={tab.href}
      class={tab.active ? 'is-active' : ''}
      aria-current={tab.active ? 'true' : undefined}
      style="display:inline-flex;align-items:center;min-height:42px;padding:0 14px;color:inherit;text-decoration:none;border-bottom:2px solid transparent;{tab.active
        ? 'border-bottom-color:var(--color-brand);color:var(--color-brand);'
        : ''}"
    >
      {tab.label}{#if tab.count !== undefined}&nbsp;{tab.count}{/if}
    </a>
  {/each}
</div>
