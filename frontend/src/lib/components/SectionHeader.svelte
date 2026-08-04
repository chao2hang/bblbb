<script lang="ts">
  let { title, desc = '', moreHref = '', moreLabel = '查看全部' }: { title: string; desc?: string; moreHref?: string; moreLabel?: string } = $props();
</script>

<div class="section-header">
  <div>
    <h2 class="section-title">{title}</h2>
    {#if desc}<p class="section-desc">{desc}</p>{/if}
  </div>
  {#if moreHref}<a href={moreHref} class="section-more">{moreLabel}</a>{/if}
</div>
