<script lang="ts">
  // 标签聚合页（公开 SSR）：标签标题 + 该标签下的帖子列表 + 空态引导 +
  // 分页。标签帖子端点未就绪时（unavailable）渲染空态，不报错。
  import PostList from '$lib/components/PostList.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import Seo from '$lib/components/Seo.svelte';
  import { formatCount } from '$lib/utils';
  import type { TagDetailPageData } from './+page.server';
  import { resolveSiteCopy, type SiteCopyView } from '$lib/site/copy';

  // data.site：根 layout 注入的全站文案（0065）；隔离渲染时兜底解析。
  let { data }: { data: TagDetailPageData & { site?: SiteCopyView | null } } = $props();

  const site = $derived<SiteCopyView>(data.site ?? resolveSiteCopy(null));

  const tagName = $derived(data.tag?.name ?? data.slug);
  const tagDescription = $derived(data.tag?.description ?? null);
  const posts = $derived(data.posts);

  /** 下一页链接（?after= keyset）。 */
  const nextHref = $derived.by(() => {
    if (!data.hasMore || !data.nextCursor) return '';
    return `/tags/${encodeURIComponent(data.slug)}?after=${encodeURIComponent(data.nextCursor)}`;
  });
</script>

<Seo
  title="标签：{tagName}"
  description={`社区标签「${tagName}」下的帖子聚合`}
  og={{ type: 'website' }}
  jsonLd={{
    '@context': 'https://schema.org',
    '@type': 'CollectionPage',
    name: `标签：${tagName} · ${site.siteName}`
  }}
/>

<div class="container page-content">

  <header class="card" style="margin-bottom:var(--space-4);">
    <div class="card-body">
      <h1
        style="display:flex;align-items:center;gap:var(--space-2);margin:0 0 var(--space-2);"
      >
        <Icon name="tag" size={22} />
        {tagName}
      </h1>
      {#if tagDescription}
        <p class="text-secondary" style="margin:0;">{tagDescription}</p>
      {:else}
        <p class="text-secondary" style="margin:0;">聚合所有包含该标签的帖子</p>
      {/if}
      {#if data.tag}
        <p class="text-secondary" style="margin:var(--space-2) 0 0;font-size:var(--text-sm);">
          {formatCount(data.tag.usage_count)} 次使用
        </p>
      {/if}
    </div>
  </header>

  {#if posts.length === 0}
    <div class="card">
      <div class="card-body">
        <EmptyState
          icon="tag"
          title="这个标签下还没有内容"
          desc="发布第一篇带 #{tagName} 标签的帖子吧"
        />
        {#if data.unavailable}
          <p class="text-secondary" style="text-align:center;font-size:var(--text-sm);">
            标签内容聚合接口尚未开放，稍后再来看看。
          </p>
        {/if}
        <div style="text-align:center;margin-top:var(--space-3);">
          <Button text="去发布" variant="secondary" icon="pen-line" href="/editor" />
        </div>
      </div>
    </div>
  {:else}
    <div class="card">
      <div class="card-body" style="padding:0;">
        <PostList {posts} emptyTitle="这个标签下还没有内容" emptyDesc="发布第一篇相关帖子吧" />
      </div>
    </div>

    {#if nextHref}
      <div style="display:flex;justify-content:center;margin-top:var(--space-4);">
        <a class="btn btn-secondary" href={nextHref}>下一页 →</a>
      </div>
    {:else}
      <p class="text-secondary" style="text-align:center;margin-top:var(--space-4);">— 没有更多内容了 —</p>
    {/if}
  {/if}
</div>
