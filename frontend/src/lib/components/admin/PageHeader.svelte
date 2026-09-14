<script lang="ts">
  // AUI 升级后渲染正式页头；升级前使用相邻 light-DOM fallback，避免慢 hydration 首屏丢标题。
  let {
    title,
    description = ''
  }: {
    title: string;
    description?: string;
  } = $props();
</script>

<aui-page-header class="app-admin-page-header" {title} {description}></aui-page-header>
<div class="app-admin-page-header-fallback">
  <h1>{title}</h1>
  {#if description}<p>{description}</p>{/if}
</div>
