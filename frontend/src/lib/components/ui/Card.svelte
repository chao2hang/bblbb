<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    title = '',
    class: klass = '',
    children,
    header,
    footer
  }: {
    title?: string;
    class?: string;
    children?: Snippet;
    header?: Snippet;
    footer?: Snippet;
  } = $props();
</script>

<div class="card {klass}">
  {#if title}
    <div class="card-header">
      <div class="card-title">{title}</div>
    </div>
  {:else if header}
    <div class="card-header">{@render header()}</div>
  {/if}
  {#if children}<div class="card-body">{@render children()}</div>{/if}
  {#if footer}
    <div class="card-footer">{@render footer()}</div>
  {/if}
</div>
