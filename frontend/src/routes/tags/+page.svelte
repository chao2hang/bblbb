<script lang="ts">
  // M03-UI-06：标签页 SSR——按分组展示标签（颜色/使用数），点击进入
  // /search?tag={slug} 标签筛选；空状态与权限无关（标签为公开元数据）。
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import type { TagsPageData } from './+page.server';

  let { data }: { data: TagsPageData } = $props();

  const tags = $derived(data.tags);
  const groups = $derived(data.groups);
  const error = $derived(data.error);

  /** 按分组组织：有 group_id 的进对应组，无组的进「未分组」。 */
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

<svelte:head>
  <title>标签 — BBLBB</title>
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">标签</span>
  </nav>

  {#if error && tags.length === 0}
    <p class="input-hint is-error" role="alert">{error}</p>
  {/if}

  {#if tags.length === 0 && !error}
    <EmptyState icon="tag" title="暂无标签" desc="还没有标签" />
  {:else if grouped.length}
    <div class="card">
      <div class="card-header">
        <span class="card-title">全部标签</span>
        <span class="text-secondary" style="font-size:var(--text-sm);">共 {tags.length} 个标签</span>
      </div>
      <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-5);">
        {#each grouped as group}
          <section aria-label={group.groupName}>
            <h2 class="board-tree-title" style="margin:0 0 var(--space-3);font-size:var(--text-lg);">{group.groupName}</h2>
            <div style="display:flex;flex-wrap:wrap;gap:var(--space-2);">
              {#each group.items as tag}
                <a
                  class="tag-chip"
                  href="/search?tag={tag.slug}"
                  style="--tag-color:{tag.color || '#666'};"
                  title="{tag.description ?? tag.name}"
                >
                  <Icon name="tag" size={12} />
                  <span>{tag.name}</span>
                  <span class="tag-count">{tag.usage_count}</span>
                </a>
              {/each}
            </div>
          </section>
        {/each}
      </div>
    </div>
  {/if}
</div>
