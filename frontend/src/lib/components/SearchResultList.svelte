<script lang="ts">
  // M08-UI-02：搜索结果列表——只渲染后端安全投影字段。
  //
  // 渲染范围 = SearchResultView 白名单：title/url/excerpt/highlight + 平面
  // post 行展示字段（board_slug/board_name/author_name/reply_count/view_count）。
  // 隐藏正文等任何受限字段由后端投影剔除；本组件按文本插值渲染，绝不使用
  // {@html}、绝不拼接/推导正文（对抗性输入测试见 search-nojs.test）。
  import EmptyState from './ui/EmptyState.svelte';
  import Icon from './ui/Icon.svelte';
  import type { SearchResultView } from '$lib/api/types';

  let {
    results,
    emptyTitle = '没有找到相关内容',
    emptyDesc = '换个关键词试试'
  }: {
    results: SearchResultView[];
    emptyTitle?: string;
    emptyDesc?: string;
  } = $props();

  /** 结果类型 → 中文标签（type 为后端白名单枚举，缺省按 post 展示）。 */
  function typeLabel(type: SearchResultView['type']): string {
    const map: Record<SearchResultView['type'], string> = {
      post: '帖子',
      user: '用户',
      board: '板块',
      tag: '标签'
    };
    return map[type] ?? '帖子';
  }

  function typeIcon(type: SearchResultView['type']): string {
    const map: Record<SearchResultView['type'], string> = {
      post: 'message-square',
      user: 'user',
      board: 'layout-dashboard',
      tag: 'tag'
    };
    return map[type] ?? 'search';
  }
</script>

{#if !results || results.length === 0}
  <EmptyState icon="search" title={emptyTitle} desc={emptyDesc} />
{:else}
  <ul class="search-result-list" aria-label="搜索结果">
    {#each results as item (item.id + item.type)}
      <li class="search-result-item">
        <a class="search-result-title" href={item.url}>
          {item.title || '（无标题）'}
        </a>
        <div class="search-result-meta">
          <span class="badge badge-neutral">
            <Icon name={typeIcon(item.type)} size={11} />
            {typeLabel(item.type)}
          </span>
          {#if item.type === 'post' && item.board_name}
            <span class="text-secondary">{item.board_name}</span>
          {/if}
          {#if item.author_name}
            <span class="text-secondary">作者：{item.author_name}</span>
          {/if}
        </div>
        {#if item.highlight}
          <!-- 后端安全高亮片段：纯文本插值（非 HTML），字符受限、已清洗。 -->
          <p class="search-result-highlight" data-field="highlight">{item.highlight}</p>
        {/if}
        {#if item.excerpt}
          <p class="search-result-excerpt">{item.excerpt}</p>
        {/if}
      </li>
    {/each}
  </ul>
{/if}
