<script lang="ts">
  import Icon from './ui/Icon.svelte';
  import { formatCount, escapeHtml } from '$lib/utils';

  let {
    slug,
    name,
    description = '',
    post_count = 0,
    today_count = 0,
    icon = 'message-square',
    color = 'var(--color-accent)'
  }: {
    slug: string;
    name: string;
    description?: string;
    post_count?: number;
    today_count?: number;
    icon?: string;
    color?: string;
  } = $props();
</script>

<a href="/boards/{slug}" class="board-card" style="--cat-color:{color};">
  <div class="board-card-icon"><Icon name={icon} size={20} /></div>
  <div class="board-card-name">{escapeHtml(name)}</div>
  {#if description}<div class="board-card-desc">{escapeHtml(description)}</div>{/if}
  <div class="board-card-stats">
    <span><strong>{formatCount(post_count)}</strong> 帖子</span>
    <span><strong>{formatCount(today_count)}</strong> 今日</span>
  </div>
</a>
