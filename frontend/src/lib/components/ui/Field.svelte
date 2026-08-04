<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    label = '',
    hint = '',
    error = '',
    id,
    class: klass = '',
    children
  }: {
    label?: string;
    hint?: string;
    error?: string;
    id?: string;
    class?: string;
    children?: Snippet;
  } = $props();
</script>

<div class="input-wrapper {klass}">
  {#if label}
    <label class="input-label" for={id}>{label}</label>
  {/if}
  {#if children}{@render children()}{/if}
  {#if error}
    <p class="input-hint is-error" role="alert">{error}</p>
  {:else if hint}
    <p class="input-hint">{hint}</p>
  {/if}
</div>
