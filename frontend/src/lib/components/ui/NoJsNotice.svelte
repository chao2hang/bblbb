<script lang="ts">
  // M00-FRONTEND-08：无 JS 降级提示（<noscript> 内容由服务端渲染进 SSR HTML）。
  let { message = '本站部分功能需要启用 JavaScript 才能使用。' }: { message?: string } = $props();
</script>

<noscript>
  <div class="empty-state" role="note">
    <div class="empty-state-title">需要启用 JavaScript</div>
    <div class="empty-state-desc">{message}</div>
  </div>
</noscript>