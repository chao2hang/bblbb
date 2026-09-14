<script lang="ts">
  import Icon from './ui/Icon.svelte';
  import Badge from './ui/Badge.svelte';
  import Tag from './ui/Tag.svelte';
  import Avatar from './ui/Avatar.svelte';
  import EmptyState from './ui/EmptyState.svelte';
  import UserCard from './UserCard.svelte';
  import { formatCount, formatRelative, escapeHtml } from '$lib/utils';
  import { boardVisuals } from '$lib/board-visuals';

  export interface PostRowData {
    id: string;
    title: string;
    summary?: string | null;
    board_slug?: string | null;
    board_name?: string | null;
    author_name?: string | null;
    author_id?: string | null;
    /** 嵌套作者投影（GET /posts、GET /boards/{slug}/posts）。
     *  display_name 为作者昵称；chip 优先昵称、缺省回退用户名。 */
    author?: { id?: string; username?: string | null; display_name?: string | null } | null;
    /** 平面作者用户名投影（搜索等）。 */
    author_username?: string | null;
    /** 作者昵称平面投影（与 author.display_name 同源）。 */
    author_display_name?: string | null;
    reply_count?: number;
    view_count?: number;
    like_count?: number;
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

  /** 行内可用的作者用户名（嵌套投影优先；无则退回平面投影）。 */
  function authorUsername(post: PostRowData): string | null {
    return post.author?.username ?? post.author_username ?? post.author_name ?? null;
  }

  /** 行内展示的作者标签：优先昵称，缺省回退用户名（账号）。 */
  function authorLabel(post: PostRowData): string {
    return (
      post.author?.display_name ?? post.author_display_name ?? authorUsername(post) ?? '匿名'
    );
  }
</script>

{#if !posts || posts.length === 0}
  <EmptyState icon="message-square" title={emptyTitle} desc={emptyDesc} />
{:else}
  <div class="app-post-list" role="feed" aria-label="帖子列表">
    {#each posts as post (post.id)}
      {@const uname = authorUsername(post) ?? '匿名'}
      {@const account = authorUsername(post)}
      {@const label = authorLabel(post)}
      <div class="app-post-row" data-post-id={post.id}>
        {#if account}
          <!-- 头像即触发链接：hover/focus 出公开资料悬浮卡（UserCard portal），
               窄屏点击出底部卡；无账号投影（匿名）保持普通头像。 -->
          <UserCard user={{ username: account, display_name: label }} label="查看 {label} 的个人资料">
            <Avatar name={uname} size="md" seed={account ?? uname} />
          </UserCard>
        {:else}
          <Avatar name={uname} size="md" seed={account ?? uname} />
        {/if}
        <a class="app-post-row__main" href="/posts/{post.id}">
          <h3>
            {#if post.pinned}<Badge text="置顶" type="pinned" />{/if}
            {escapeHtml(post.title)}
          </h3>
          <span class="app-post-row__meta">
            {#if post.board_slug && post.board_name}
              <span class="category-badge" style="--cat-color:{boardVisuals(post.board_slug).color};">
                <span class="category-badge-square"></span>
                <span>{post.board_name}</span>
              </span>
            {:else if post.board_name}
              <span class="sbadge sb-gray">{post.board_name}</span>
            {/if}
            {#if post.tags?.length}
              {#each post.tags as tag}
                <span class="app-tag-subtle">#{tag}</span>
              {/each}
            {/if}
            <span>· {authorLabel(post)}</span>
          </span>
        </a>
        <span class="app-post-row__right">
          <b>{formatCount(post.reply_count ?? 0)} 回复</b>
          <span>{formatRelative(post.last_reply_at ?? post.created_at ?? Date.now())}</span>
        </span>
      </div>
    {/each}
  </div>
{/if}
