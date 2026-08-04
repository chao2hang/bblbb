<script lang="ts">
  import Icon from './ui/Icon.svelte';
  import Badge from './ui/Badge.svelte';
  import Tag from './ui/Tag.svelte';
  import Avatar from './ui/Avatar.svelte';
  import EmptyState from './ui/EmptyState.svelte';
  import { formatCount, formatRelative, escapeHtml } from '$lib/utils';

  export interface PostRowData {
    id: string;
    title: string;
    board_slug?: string | null;
    board_name?: string | null;
    author_name?: string | null;
    author_id?: string | null;
    reply_count?: number;
    view_count?: number;
    created_at?: number;
    last_reply_at?: number | null;
    pinned?: boolean;
    visibility?: string;
    tags?: string[];
  }

  let {
    posts,
    emptyTitle = '暂无帖子',
    emptyDesc = '成为第一个发帖的人吧！'
  }: { posts: PostRowData[]; emptyTitle?: string; emptyDesc?: string } = $props();
</script>

{#if !posts || posts.length === 0}
  <EmptyState icon="message-square" title={emptyTitle} desc={emptyDesc} />
{:else}
  <div class="post-list" role="table" aria-label="帖子列表">
    <div class="post-list-head" role="row">
      <div class="post-list-head-cell is-main" role="columnheader">主题</div>
      <div class="post-list-head-cell is-posters" role="columnheader">参与者</div>
      <div class="post-list-head-cell" role="columnheader">回复</div>
      <div class="post-list-head-cell" role="columnheader">浏览</div>
      <div class="post-list-head-cell" role="columnheader">活动</div>
    </div>
    {#each posts as post (post.id)}
      <div class="post-row" role="row">
        <div class="post-row-main" role="cell">
          <div class="post-row-title">
            {#if post.pinned}<Badge text="置顶" type="pinned" />{/if}
            <a href="/posts/{post.id}">{escapeHtml(post.title)}</a>
          </div>
          <div class="post-row-meta">
            {#if post.board_slug && post.board_name}
              <a href="/boards/{post.board_slug}" class="category-badge" style="--cat-color:var(--color-accent);">
                <span class="category-badge-square"></span><span>{post.board_name}</span>
              </a>
            {/if}
            {#if post.tags?.length}
              {#each post.tags as tag}<Tag name={tag} />{/each}
            {/if}
          </div>
        </div>
        <div class="post-row-posters" role="cell">
          {#if post.author_name}
            <span class="author-hover-trigger" aria-label="查看 {post.author_name} 的个人资料">
              <Avatar name={post.author_name} size="xs" />
            </span>
          {/if}
        </div>
        <div class="post-row-num {(post.reply_count ?? 0) >= 20 ? 'is-hot' : ''}" role="cell">{formatCount(post.reply_count)}</div>
        <div class="post-row-num" role="cell">{formatCount(post.view_count)}</div>
        <div class="post-row-activity" role="cell">{formatRelative(post.last_reply_at ?? post.created_at)}</div>
      </div>
    {/each}
  </div>
{/if}
