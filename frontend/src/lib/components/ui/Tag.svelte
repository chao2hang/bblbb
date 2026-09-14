<script lang="ts">
  // 真实渲染 blbui StatusTag；custom elements 由根布局一次性注册。

  let {
    name,
    count = null,
    href = null,
    onremove = null
  }: {
    name: string;
    count?: number | null;
    href?: string | null;
    onremove?: (() => void) | null;
  } = $props();
</script>

{#if href}
  <a href={href} class="tag">
    <aui-status-tag status="default">{name}</aui-status-tag>
    {#if count !== null}<span class="tag-count">{count}</span>{/if}
  </a>
{:else if onremove}
  <span class="tag tag--removable tag-chip">
    <aui-status-tag status="default">{name}</aui-status-tag>
    {#if count !== null}<span class="tag-count">{count}</span>{/if}
    <button
      type="button"
      class="tag-chip-remove"
      aria-label="移除标签 {name}"
      onclick={(e) => {
        e.stopPropagation();
        onremove();
      }}
    >
      <span aria-hidden="true">&times;</span>
    </button>
  </span>
{:else}
  <span class="tag">
    <aui-status-tag status="default">{name}</aui-status-tag>
    {#if count !== null}<span class="tag-count">{count}</span>{/if}
  </span>
{/if}
