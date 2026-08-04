<script lang="ts">
  import Icon from './ui/Icon.svelte';
  import Avatar from './ui/Avatar.svelte';
  import { formatCount, escapeHtml } from '$lib/utils';

  let {
    id,
    title,
    summary = '',
    author_name = '',
    view_count = 0,
    color = '#0969DA'
  }: {
    id: string;
    title: string;
    summary?: string;
    author_name?: string;
    view_count?: number;
    color?: string;
  } = $props();
</script>

<a href="/posts/{id}" class="article-card">
  <div
    class="article-card-cover"
    style="background:linear-gradient(135deg, {color} 0%, {color}cc 55%, var(--color-ink) 130%);"
  >
    <Icon name="file-text" size={36} />
  </div>
  <div class="article-card-body">
    <div class="article-card-title">{escapeHtml(title)}</div>
    {#if summary}<div class="article-card-summary">{escapeHtml(summary)}</div>{/if}
    <div class="article-card-footer">
      <div class="article-card-author">
        {#if author_name}
          <span class="author-hover-trigger" aria-label="查看 {author_name} 的个人资料">
            <Avatar name={author_name} size="xs" />
          </span>
          <span class="author-hover-name-trigger">{escapeHtml(author_name)}</span>
        {/if}
      </div>
      <span class="article-card-reads">{formatCount(view_count)} 阅读</span>
    </div>
  </div>
</a>
