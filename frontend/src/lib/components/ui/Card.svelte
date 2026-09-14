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

<!--
  Card 是兼容旧调用点的薄适配层，实际表面由 blbui aui-card 渲染。
  页面级布局不要继续新增 Card；列表/模块优先使用 Panel。
-->
<aui-card class="bblbb-aui-card {klass}">
  {#if title}
    <span slot="header" class="card-title">{title}</span>
  {:else if header}
    <span slot="header" class="card-header-content">{@render header()}</span>
  {/if}
  {#if children}{@render children()}{/if}
  {#if footer}<span slot="footer" class="card-footer-content">{@render footer()}</span>{/if}
</aui-card>
