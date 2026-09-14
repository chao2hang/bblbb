<script lang="ts">
  // 板块分类导航卡（首页左栏 category-card 同款视觉：55px 行 + 身份色图标 +
  // 右对齐计数，当前项品牌色高亮）——供板块详情页左栏与帖子详情页侧栏共用，
  // 读帖/切板块时随时可跳转其他板块。
  //
  // 首页（routes/+page.svelte）的同款卡片暂未收敛到本组件（首页带移动端
  // sheet/筛选联动，重构另行处理）；本组件的行结构/样式与其逐字对齐，
  // 改动任一侧时须同步另一侧。
  //
  // - 「全部」→ 首页全部信息流 /（与首页左栏分类卡的「全部」同语义；
  //   activeSlug 为空时高亮）；板块索引 /boards 由导航栏/板块页入口承接；
  // - 计数诚实性（M03）：匿名投影契约隐藏 post_count → 行不渲染计数；
  //   「全部」只在有任一计数投影时汇总（不得假报 0，与 /boards 索引同规则）。
  // - 列表为空（接口降级）时整卡不渲染，不占侧栏空间。
  import Icon from '$lib/components/ui/Icon.svelte';
  import { boardVisuals } from '$lib/board-visuals';
  import { formatCount } from '$lib/utils';

  export interface BoardNavItem {
    id: string;
    slug: string;
    name: string;
    /** 匿名投影缺省（契约隐藏计数）→ undefined，行不渲染计数。 */
    post_count?: number | null;
    /** 持久化板块图标（boards.icon；null/未知名由 boardVisuals 回退）。 */
    icon?: string | null;
  }

  let {
    boards,
    activeSlug = null
  }: {
    boards: BoardNavItem[];
    /** 当前所在板块 slug（空 = 「全部」高亮，如首页/无板块投影时）。 */
    activeSlug?: string | null;
  } = $props();

  const total = $derived.by(() => {
    if (!boards.some((b) => typeof b.post_count === 'number')) return null;
    return boards.reduce((acc, b) => acc + (b.post_count ?? 0), 0);
  });
</script>

{#if boards.length > 0}
  <section class="category-card" aria-label="板块分类">
    <a class:selected={!activeSlug} href="/" aria-current={!activeSlug ? 'page' : undefined}>
      <b>全部</b>
      {#if total !== null}<em>{formatCount(total)}</em>{/if}
    </a>
    {#each boards as board (board.id)}
      {@const visuals = boardVisuals(board.slug, board.icon)}
      <a
        class:selected={board.slug === activeSlug}
        href="/boards/{board.slug}"
        aria-current={board.slug === activeSlug ? 'page' : undefined}
      >
        <span class="cat-name">
          <span class="cat-name__icon" style="color:{visuals.color};" aria-hidden="true">
            <Icon name={visuals.icon || 'workflow'} size={15} />
          </span>
          {board.name}
        </span>
        {#if typeof board.post_count === 'number'}<em>{formatCount(board.post_count)}</em>{/if}
      </a>
    {/each}
  </section>
{/if}

<style>
  /* ===== 首页左栏 category-card 同款（逐字对齐 routes/+page.svelte）=====
     表面（bg/border/shadow）由全局 surface/chinese-elegance 的 .category-card
     分组提供，这里只负责行布局。 */
  .category-card {
    height: max-content;
    padding: 6px 16px;
  }
  .category-card a {
    height: 55px;
    border-bottom: var(--border-default);
    display: flex;
    align-items: center;
    justify-content: space-between;
    color: var(--color-text-secondary);
    text-decoration: none;
    font-size: 15px;
    padding: 0 14px;
    transition: color var(--duration-fast);
  }
  .category-card a:last-child {
    border: 0;
  }
  /* 行悬停：首页未做，轻补一个文字提亮（选中态保持品牌色） */
  .category-card a:hover {
    color: var(--color-text-primary);
  }
  .category-card a.selected,
  .category-card a.selected:hover {
    color: var(--color-brand);
    font-weight: var(--weight-semibold);
  }
  .category-card em {
    font-style: normal;
    font-size: var(--text-xs);
    color: var(--color-text-tertiary);
  }
  /* 板块行：持久化图标（boards.icon，回退 slug 映射）+ 名称 */
  .category-card .cat-name {
    display: inline-flex;
    align-items: center;
    gap: 9px;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* 图标容器自身为 flex：svg 脱离文本基线排版，与 CJK 文字几何居中
     （inline span 直包 svg 会因基线降部空隙整体偏高）。 */
  .category-card .cat-name__icon {
    display: inline-flex;
    align-items: center;
    line-height: 0;
  }
  .category-card .cat-name :global(svg) {
    flex: 0 0 auto;
    display: block;
  }
</style>
