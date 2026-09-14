<script lang="ts">
  // M03-UI-06：标签页 SSR——按分组展示标签，点击进入 /tags/{slug} 聚合页。
  //
  // 优化后的信息设计（对齐冷墨 tokens，无新增颜色）：
  // - 首屏保留「索引即内容」原则，不加装饰性路由标题块；以一行 kicker
  //   （DISCOVER / TAGS，同原型 app-kicker）+ 一行统计（标签数/累计使用）
  //   提供页面定位。
  // - 标签按 usage_count 降序排列；胶囊字号随相对热度（count/max）连续
  //   缩放——层级来自真实数据，不做装饰性强调。
  // - usage_count 为 0 的标签以虚线「幽灵」样式呈现（视觉上让位于在用
  //   标签），卡片脚注解释该样式。
  // - 分组标题仅在实际存在分组时渲染：全部未分组时不再出现孤立的「其他」。
  // - 空态/错误态文案给出行动指引；错误态附重新加载入口。
  // 无 JS 可读：分组名、标签名与 /tags/{slug} 链接均在 SSR HTML 中。
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Seo from '$lib/components/Seo.svelte';
  import { tagSearchUrl } from '$lib/search';
  import { formatCount } from '$lib/utils';
  import type { TagsPageData } from './+page.server';
  import { resolveSiteCopy, type SiteCopyView } from '$lib/site/copy';

  // data.site：根 layout 注入的全站文案（0065）；隔离渲染时兜底解析。
  let { data }: { data: TagsPageData & { site?: SiteCopyView | null } } = $props();

  const site = $derived<SiteCopyView>(data.site ?? resolveSiteCopy(null));

  const tags = $derived(data.tags);
  const groups = $derived(data.groups);
  const error = $derived(data.error);

  /** 按热度排序（usage 降序，同名按中文字序），0 使用自然沉底。 */
  const sortedTags = $derived(
    [...tags].sort(
      (a, b) =>
        b.usage_count - a.usage_count || a.name.localeCompare(b.name, 'zh-Hans-CN')
    )
  );

  /** 相对热度基准：最大使用数；全 0 时所有标签按幽灵样式渲染。 */
  const maxUsage = $derived(tags.reduce((m, t) => Math.max(m, t.usage_count), 0));
  const totalUsage = $derived(tags.reduce((sum, t) => sum + t.usage_count, 0));
  const unusedCount = $derived(tags.filter((t) => t.usage_count === 0).length);

  /** 标签的相对热度（0–1），驱动胶囊字号。 */
  function weightOf(usage: number): number {
    if (maxUsage <= 0 || usage <= 0) return 0;
    return Math.round((usage / maxUsage) * 1000) / 1000;
  }

  /** 按分组组织：有 group_id 的进对应组，无组的进「其他」（仅当存在分组）。 */
  const grouped = $derived.by(() => {
    const result: Array<{ groupId: string; groupName: string; items: typeof sortedTags }> = [];
    for (const group of groups) {
      const items = sortedTags.filter((t) => t.group_id === group.id);
      if (items.length) result.push({ groupId: group.id, groupName: group.name, items });
    }
    const ungrouped = sortedTags.filter((t) => !t.group_id);
    if (ungrouped.length && groups.length) {
      result.push({ groupId: 'ungrouped', groupName: '其他', items: ungrouped });
    }
    return result;
  });

  /** 无分组时直接渲染单一云（不显示「其他」标题）。 */
  const flatCloud = $derived(grouped.length === 0 ? sortedTags : []);
</script>

<Seo
  title="标签"
  description="从一个关键词进入相关内容"
  og={{ type: 'website' }}
  jsonLd={{
    '@context': 'https://schema.org',
    '@type': 'CollectionPage',
    name: `标签 · ${site.siteName}`
  }}
/>

<div class="container app-page">
  <section class="page app-route-tags" id="page-tags">
    <h1 class="u-visually-hidden">标签</h1>

    {#if error && tags.length === 0}
      <div class="app-card" role="alert">
        <div class="app-card__body tags-error">
          <p class="input-hint is-error" style="margin:0;">{error}</p>
          <a class="btn secondary sm" href="/tags">重新加载</a>
        </div>
      </div>
    {:else if tags.length === 0}
      <div class="app-card">
        <div class="app-card__body">
          <EmptyState
            icon="tag"
            title="暂无标签"
            desc="发布带标签的帖子后，标签会出现在这里"
          />
        </div>
      </div>
    {:else}
      <!-- 页面定位：kicker + 统计，一行说明这里有什么、有多少 -->
      <header class="tags-meta">
        <p class="tags-meta__kicker">DISCOVER / TAGS</p>
        <p class="tags-meta__stats">
          {tags.length} 个标签 · 累计 {formatCount(totalUsage)} 次使用
        </p>
      </header>

      <div class="app-card">
        <div class="app-card__body tags-body">
          {#if grouped.length}
            {#each grouped as group (group.groupId)}
              <section class="tags-group">
                <h2 class="tags-group__label">
                  {group.groupName}
                  <span class="tags-group__count">{group.items.length}</span>
                </h2>
                <ul class="tags-cloud">
                  {#each group.items as tag (tag.id)}
                    <li>
                      <a
                        class="app-tag"
                        class:is-ghost={tag.usage_count === 0}
                        class:is-lead={maxUsage > 0 && tag.usage_count === maxUsage}
                        style={`--w:${weightOf(tag.usage_count)}`}
                        href={tagSearchUrl(tag)}
                      >
                        <span class="app-tag__hash" aria-hidden="true">#</span>
                        <span class="app-tag__name">{tag.name}</span>
                        <span class="app-tag__count">{formatCount(tag.usage_count)}</span>
                      </a>
                    </li>
                  {/each}
                </ul>
              </section>
            {/each}
          {:else if flatCloud.length}
            <ul class="tags-cloud">
              {#each flatCloud as tag (tag.id)}
                <li>
                  <a
                    class="app-tag"
                    class:is-ghost={tag.usage_count === 0}
                    class:is-lead={maxUsage > 0 && tag.usage_count === maxUsage}
                    style={`--w:${weightOf(tag.usage_count)}`}
                    href={tagSearchUrl(tag)}
                  >
                    <span class="app-tag__hash" aria-hidden="true">#</span>
                    <span class="app-tag__name">{tag.name}</span>
                    <span class="app-tag__count">{formatCount(tag.usage_count)}</span>
                  </a>
                </li>
              {/each}
            </ul>
          {/if}
        </div>
        {#if unusedCount > 0}
          <footer class="app-card__foot tags-foot">
            <span>虚线标签暂无内容——发帖时打上标签即可点亮</span>
          </footer>
        {/if}
      </div>
    {/if}
  </section>
</div>

<style>
  /* ── 页面定位行：kicker（品牌信号，沿用原型 app-kicker 语言）+ 统计 ── */
  .tags-meta {
    display: flex;
    flex-direction: column;
    gap: 3px;
    margin: 2px 2px 12px;
  }
  .tags-meta__kicker {
    margin: 0;
    color: var(--color-brand);
    font-family: var(--font-family-mono);
    font-size: 10px;
    font-weight: 600;
    line-height: 1.4;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }
  .tags-meta__stats {
    margin: 0;
    color: var(--color-text-tertiary);
    font-family: var(--font-family-mono);
    font-size: 11px;
    font-weight: 500;
    line-height: 1.5;
    letter-spacing: 0.02em;
  }

  .tags-error {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 12px;
  }
  .tags-body {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  /* ── 分组：仅在存在分组时出现；组名 + 发丝线延伸 ── */
  .tags-group + .tags-group {
    margin-top: 20px;
    padding-top: 18px;
    border-top: 1px solid var(--color-border-muted);
  }
  .tags-group__label {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0 0 14px;
    color: var(--color-text-tertiary);
    font-family: var(--font-family-base);
    font-size: 12px;
    font-weight: 600;
    line-height: 1.4;
    letter-spacing: 0.05em;
  }
  .tags-group__label::after {
    content: '';
    flex: 1;
    height: 1px;
    background: var(--color-border-muted);
  }
  .tags-group__count {
    color: var(--color-text-tertiary);
    opacity: 0.7;
    font-family: var(--font-family-mono);
    font-size: 10px;
    font-weight: 500;
  }

  /* ── 标签云：层级即数据。胶囊字号随相对热度连续缩放；全站唯一的
     尺度冒险留给这个索引页，其余保持细边描线语言。 ── */
  .tags-cloud {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 10px 14px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .tags-cloud li {
    display: flex;
  }
  .tags-cloud .app-tag {
    display: inline-flex !important;
    align-items: baseline !important;
    gap: 6px !important;
    padding: 6px 11px 7px !important;
    border: 1px solid var(--color-border) !important;
    border-radius: var(--radius-sm) !important;
    background: transparent !important;
    color: var(--color-text-primary) !important;
    font-family: var(--font-family-mono) !important;
    /* 相对热度 → 12–19px；0 使用标签固定 11px（见 .is-ghost） */
    font-size: calc(12px + 7px * var(--w, 0)) !important;
    font-weight: 600 !important;
    letter-spacing: var(--label-letter-spacing, 0.06em) !important;
    text-transform: uppercase !important;
    text-decoration: none !important;
    transition:
      border-color var(--duration-fast, 0.15s) ease,
      color var(--duration-fast, 0.15s) ease;
  }
  .tags-cloud .app-tag__hash,
  .tags-cloud .app-tag__count {
    color: inherit;
    opacity: 0.45;
    font-weight: 500;
  }
  .tags-cloud .app-tag__count {
    font-size: 10px;
    font-variant-numeric: tabular-nums;
  }
  /* 最高热度：边框加重一档，墨色不变——强调仍来自尺度而非涂装 */
  .tags-cloud .app-tag.is-lead {
    border-color: var(--color-border-strong) !important;
  }
  /* 0 使用：虚线幽灵，让位于在用标签 */
  .tags-cloud .app-tag.is-ghost {
    border: 1px dashed var(--color-border) !important;
    color: var(--color-text-tertiary) !important;
    font-size: 11px !important;
    font-weight: 500 !important;
  }
  .tags-cloud .app-tag:hover,
  .tags-cloud .app-tag:focus-visible {
    border-color: var(--color-brand) !important;
    color: var(--color-brand) !important;
  }
  .tags-foot {
    font-size: 11px;
  }

  @media (max-width: 767px) {
    .tags-meta {
      margin-bottom: 10px;
    }
    .tags-cloud {
      gap: 8px 10px;
    }
    .tags-cloud .app-tag {
      padding: 8px 12px 9px !important;
    }
  }
</style>
