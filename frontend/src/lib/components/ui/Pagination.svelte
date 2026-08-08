<script lang="ts">
  // M14-COMPONENTS-01/04：可访问 Pagination 基础组件。
  //
  // - <nav aria-label="分页"> + 页码链接，当前页用 aria-current="page"；
  // - prev/next 链接带 aria-label（图标/箭头可见文案缺省时仍可读）；
  // - 只接收白名单 prop + onchange 回调（安全投影，M14-COMPONENTS-06）；
  // - 无 JS 时退化为普通链接（href 由调用方提供）。
  export interface PageLink {
    href: string;
    label: string;
    current?: boolean;
  }

  let {
    pages = [] as PageLink[],
    prevHref = '',
    nextHref = '',
    prevLabel = '上一页',
    nextLabel = '下一页',
    label = '分页',
    onchange
  }: {
    pages?: PageLink[];
    prevHref?: string;
    nextHref?: string;
    prevLabel?: string;
    nextLabel?: string;
    label?: string;
    onchange?: (page: PageLink) => void;
  } = $props();

  function handleClick(event: MouseEvent, page: PageLink): void {
    if (!onchange) return;
    event.preventDefault();
    onchange(page);
  }
</script>

<nav class="pagination" aria-label={label}>
  {#if prevHref}
    <a class="page-btn" href={prevHref} aria-label={prevLabel} onclick={(e) => handleClick(e, { href: prevHref, label: prevLabel })}>
      ‹
    </a>
  {/if}
  {#each pages as page (page.href + page.label)}
    {#if page.current}
      <a
        class="page-btn is-active"
        href={page.href}
        aria-current="page"
        onclick={(e) => handleClick(e, page)}
      >
        {page.label}
      </a>
    {:else}
      <a class="page-btn" href={page.href} onclick={(e) => handleClick(e, page)}>
        {page.label}
      </a>
    {/if}
  {/each}
  {#if nextHref}
    <a class="page-btn" href={nextHref} aria-label={nextLabel} onclick={(e) => handleClick(e, { href: nextHref, label: nextLabel })}>
      ›
    </a>
  {/if}
</nav>
