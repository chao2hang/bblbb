<script lang="ts">
  // 用户社交关系列表（M03-UI-03 / 社交域·关注）：粉丝 / 正在关注 共用列表。
  //
  // 数据：GET /api/v1/users/{username}/followers|following（follows.rs 公开端点，
  // listFollowers / listFollowing）。SSR 首页由各 +page.server.ts load 直出
  // （无 JS 也可读）；本组件负责行渲染与「加载更多」：
  //   - JS 下客户端 keyset 游标追加（?after=）；
  //   - 无 JS 回退为 ?after= 链接（整页翻页，load 读取同名参数）。
  // 行内只渲染服务端公开投影（username/display_name/level/follow created_at），
  // 每行链接到该用户主页 /users/{username}。
  import Button from '$lib/components/ui/Button.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import CosmeticAvatar from '$lib/components/wardrobe/CosmeticAvatar.svelte';
  import { listFollowers, listFollowing } from '$lib/api/client';
  import { formatRelative } from '$lib/utils';
  import { infiniteScroll } from '$lib/utils/infinite-scroll';
  import { show } from '$lib/ui/toast';

  /** 服务端公开投影行（follows.rs follow_list_response）。 */
  export type FollowUserItem = {
    username: string;
    display_name: string | null;
    level: number;
    /** 关注关系创建时间（Unix 毫秒）。 */
    created_at: number;
  };

  let {
    username,
    direction,
    items: initialItems,
    nextCursor: initialCursor,
    emptyTitle,
    emptyDesc,
    listLabel
  }: {
    username: string;
    /** followers = 关注 {username} 的人；following = {username} 关注的人。 */
    direction: 'followers' | 'following';
    items: FollowUserItem[];
    nextCursor: string | null;
    emptyTitle: string;
    emptyDesc: string;
    /** 列表 aria 标签（如「粉丝列表」）。 */
    listLabel: string;
  } = $props();

  /** 客户端追加的后续页（首页来自 SSR data）。 */
  let extraPages = $state<FollowUserItem[]>([]);
  /** loadMore 已推进到的游标；undefined = 尚未推进（回退服务端游标）。 */
  let loadedCursor = $state<string | null | undefined>(undefined);
  let loadingMore = $state(false);
  let loadFailed = $state(false);

  const items = $derived([...initialItems, ...extraPages]);
  /** 下一页游标：未推进时用服务端视图（SSR 也可渲染加载更多入口）。 */
  const cursor = $derived(loadedCursor === undefined ? initialCursor : loadedCursor);

  // load 数据变化（导航 / ?after= 翻页）后重置客户端累积页，回到服务端视图。
  $effect(() => {
    void initialItems;
    void initialCursor;
    extraPages = [];
    loadedCursor = undefined;
    loadFailed = false;
  });

  /** JS 下客户端追加下一页；无 JS 走 ?after= 链接（同一游标口径）。 */
  async function loadMore(event?: MouseEvent): Promise<void> {
    event?.preventDefault();
    if (!cursor || loadingMore) return;
    loadingMore = true;
    loadFailed = false;
    try {
      const page =
        direction === 'followers'
          ? await listFollowers(fetch, username, cursor)
          : await listFollowing(fetch, username, cursor);
      extraPages = [...extraPages, ...((page.items ?? []) as FollowUserItem[])];
      loadedCursor = page.next_cursor ? String(page.next_cursor) : null;
      loadFailed = false;
    } catch {
      loadFailed = true;
      show('加载更多失败，请重试', 'danger');
    }
    loadingMore = false;
  }

  /** 后端时间戳为毫秒（M01-DB-08），formatRelative 口径为秒。 */
  function toSeconds(ts: number | null | undefined): number | null {
    if (typeof ts !== 'number' || !Number.isFinite(ts) || ts <= 0) return null;
    return ts > 1e11 ? Math.floor(ts / 1000) : ts;
  }
</script>

{#if items.length === 0}
  <div style="padding:var(--space-6);">
    <EmptyState icon="users" title={emptyTitle} desc={emptyDesc} />
  </div>
{:else}
  <div role="feed" aria-label={listLabel} style="display:flex;flex-direction:column;">
    {#each items as item (item.username)}
      {@const followedTs = toSeconds(item.created_at)}
      <!-- 行 = 用户主页链接；公开投影行，不渲染任何私有字段。 -->
      <a
        class="follow-user-row"
        href="/users/{encodeURIComponent(item.username)}"
        style="display:flex;align-items:center;gap:var(--space-3);padding:var(--space-3) var(--space-4);border-bottom:var(--border-default);text-decoration:none;"
      >
        <CosmeticAvatar
          name={item.display_name || item.username}
          size="md"
          username={item.username}
          title={`@ ${item.username}`}
        />
        <div style="min-width:0;flex:1;">
          <div style="font-weight:var(--weight-medium);color:var(--color-text-primary);overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">
            {item.display_name || item.username}
          </div>
          <div class="text-secondary" style="font-size:var(--text-sm);margin-top:2px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">
            @ {item.username}
          </div>
        </div>
        <span class="badge badge-level" style="flex-shrink:0;">TL{item.level}</span>
        {#if followedTs !== null}
          <span class="text-secondary" style="flex-shrink:0;font-size:var(--text-sm);">
            {formatRelative(followedTs)}
          </span>
        {/if}
      </a>
    {/each}
  </div>
  {#if cursor}
    <div
      style="padding:var(--space-4);display:flex;justify-content:center;"
      use:infiniteScroll={{
        hasMore: !!cursor,
        loading: loadingMore,
        disabled: loadFailed,
        onLoadMore: () => void loadMore()
      }}
    >
      <!-- JS：客户端追加下一页；无 JS：?after= 整页翻页（均走同一游标）。 -->
      <Button
        href="?after={encodeURIComponent(cursor)}"
        text={loadingMore ? '加载中…' : loadFailed ? '加载失败，点击重试' : '加载更多'}
        variant="secondary"
        size="sm"
        disabled={loadingMore}
        onclick={(event) => void loadMore(event)}
      />
    </div>
  {/if}
{/if}

<style>
  /* 行 hover 反馈：与站点列表行一致（浅底），不做下划线（行内已有次级信息）。 */
  .follow-user-row:hover,
  .follow-user-row:focus-visible {
    background: var(--color-surface-hover);
  }
  .follow-user-row:focus-visible {
    outline: 2px solid var(--color-brand);
    outline-offset: -2px;
  }
</style>
