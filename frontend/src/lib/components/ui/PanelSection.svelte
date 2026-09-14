<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    label = '',
    description = '',
    children
  }: {
    label?: string;
    description?: string;
    children?: Snippet;
  } = $props();
</script>

<section class="panel-section">
  {#if label || description}
    <header class="panel-section__header">
      {#if label}<h2>{label}</h2>{/if}
      {#if description}<p>{description}</p>{/if}
    </header>
  {/if}
  {@render children?.()}
</section>

<style>
  .panel-section {
    min-width: 0;
  }

  .panel-section__header {
    display: grid;
    gap: var(--space-1);
    padding: var(--space-4) 0 var(--space-2);
    border-bottom: 1px solid var(--aui-border);
  }

  h2 {
    margin: 0;
    color: var(--aui-text-secondary);
    font: 600 11px/1.4 var(--aui-font-mono);
    letter-spacing: var(--label-letter-spacing);
    text-transform: uppercase;
  }

  p {
    margin: 0;
    color: var(--aui-text-muted);
    font-size: var(--text-sm);
  }
</style>
