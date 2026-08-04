<script lang="ts">
  let { name, count = null, href = null }: { name: string; count?: number | null; href?: string | null } = $props();
</script>

{#if href}
  <a href={href} class="tag">{name}{#if count !== null}<span class="tag-count">{count}</span>{/if}</a>
{:else}
  <span class="tag">{name}{#if count !== null}<span class="tag-count">{count}</span>{/if}</span>
{/if}
