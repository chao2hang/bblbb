<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    href = '',
    class: klass = '',
    children
  }: {
    href?: string;
    class?: string;
    children?: Snippet;
  } = $props();
</script>

{#if href}
  <a class="list-row {klass}" href={href}>{@render children?.()}</a>
{:else}
  <div class="list-row {klass}">{@render children?.()}</div>
{/if}

<style>
  .list-row {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-4) 0;
    border-bottom: 1px solid var(--color-border-muted);
    color: inherit;
    text-decoration: none;
    transition: background-color var(--duration-fast), border-color var(--duration-fast);
  }

  .list-row:last-child {
    border-bottom: 0;
  }

  .list-row:hover {
    background: var(--color-surface-hover);
    text-decoration: none;
  }
</style>
