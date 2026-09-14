<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    items = [],
    children
  }: {
    items?: string[];
    children?: Snippet;
  } = $props();
</script>

<span class="meta">
  {#if items.length > 0}
    {#each items as item, index (index)}
      {#if index > 0}<span class="meta__separator" aria-hidden="true">·</span>{/if}
      <span>{item}</span>
    {/each}
  {/if}
  {@render children?.()}
</span>

<style>
  .meta {
    display: inline-flex;
    min-width: 0;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-1);
    color: var(--aui-text-secondary);
    font: var(--text-xs)/1.4 var(--aui-font-mono);
    font-variant-numeric: tabular-nums;
    letter-spacing: var(--meta-letter-spacing);
  }

  .meta__separator {
    color: var(--aui-text-muted);
  }
</style>
