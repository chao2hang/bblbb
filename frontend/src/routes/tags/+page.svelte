<script lang="ts">
  // M03-UI-06：标签页 SSR——按分组展示标签（颜色/使用数），点击进入
  // /search?tag={slug} 标签筛选；空状态与权限无关（标签为公开元数据）。
  // 原型对齐：prototype/pages/tags.html
  // - 统一 .app-route-head（DISCOVER / TAGS）
  // - .app-card > .app-card__body 容器
  // - .app-tag-cloud > .app-tag 标签胶囊
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Seo from '$lib/components/Seo.svelte';
  import type { TagsPageData } from './+page.server';

  let { data }: { data: TagsPageData } = $props();

  const tags = $derived(data.tags);
  const groups = $derived(data.groups);
  const error = $derived(data.error);

  /** 按分组组织：有 group_id 的进对应组，无组的进「其他」。 */
  const grouped = $derived.by(() => {
    const result: Array<{ groupId: string; groupName: string; items: typeof tags }> = [];
    for (const group of groups) {
      const items = tags.filter((t) => t.group_id === group.id);
      if (items.length) result.push({ groupId: group.id, groupName: group.name, items });
    }
    const ungrouped = tags.filter((t) => !t.group_id);
    if (ungrouped.length) result.push({ groupId: 'ungrouped', groupName: '其他', items: ungrouped });
    return result;
  });
</script>

<Seo
  title="标签 · BBLBB 社区"
  description="从一个关键词进入相关内容"
  og={{ type: 'website', siteName: 'BBLBB' }}
  jsonLd={{
    '@context': 'https://schema.org',
    '@type': 'CollectionPage',
    name: 'BBLBB 标签'
  }}
/>

<div class="container app-page">
  <section class="page app-page app-route-tags" id="page-tags">
    <div class="app-route-head">
      <div class="app-route-head__copy">
        <span class="app-kicker">DISCOVER / TAGS</span>
        <h1 tabindex="-1">标签</h1>
        <p>从一个关键词进入相关内容</p>
      </div>
    </div>

    {#if error && tags.length === 0}
      <p class="input-hint is-error" role="alert">{error}</p>
    {/if}

    {#if tags.length === 0 && !error}
      <EmptyState icon="tag" title="暂无标签" desc="还没有标签" />
    {:else if grouped.length}
      <section class="app-card">
        <div class="app-card__body" style="display:flex;flex-direction:column;gap:20px;">
          {#each grouped as group}
            <div>
              <h2 style="margin:0 0 10px;font-size:15px;font-weight:600;color:var(--color-text-primary);font-family:var(--font-family-base);">
                {group.groupName}
              </h2>
              <div class="app-tag-cloud">
                {#each group.items as tag}
                  <a class="app-tag" href="/search?tag={tag.slug}">
                    <b>{tag.usage_count}</b> # {tag.name}
                  </a>
                {/each}
              </div>
            </div>
          {/each}
        </div>
      </section>
    {/if}
  </section>
</div>
