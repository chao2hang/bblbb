<script lang="ts">
  // GAP-FIX（社交域·收藏）：/favorites——SSR 收藏列表 + 每行「取消收藏」
  // （form action + use:enhance → toast + invalidateAll）。
  //
  // - 无 JS 基线：列表 SSR 直出；取消收藏为原生 form POST（整页刷新）；
  // - 分页「加载更多」：JS 下客户端追加下一页（listMyFavorites 游标），
  //   无 JS 回退为 ?after= 链接（load 读取，整页翻页）；
  // - 空态：引导去首页逛逛。
  import { enhance } from '$app/forms';
  import { invalidateAll } from '$app/navigation';
  import Button from '$lib/components/ui/Button.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import LoadFailureState from '$lib/components/LoadFailureState.svelte';
  import ProblemState from '$lib/components/ProblemState.svelte';
  import { listMyFavorites } from '$lib/api/client';
  import type { PostSummary } from '$lib/api/types';
  import { isTransientProblem } from '$lib/errors';
  import { announceTransientProblem } from '$lib/ui/problem-toast';
  import { show } from '$lib/ui/toast';
  import { formatCount, formatRelative } from '$lib/utils';
  import { infiniteScroll } from '$lib/utils/infinite-scroll';
  import { boardVisuals } from '$lib/board-visuals';
  import type { FavoritesActionData, FavoritesPageData } from './+page.server';
  import PageTitle from '$lib/components/PageTitle.svelte';
  import CosmeticAvatar from '$lib/components/wardrobe/CosmeticAvatar.svelte';
  import CosmeticName from '$lib/components/wardrobe/CosmeticName.svelte';

  let { data, form }: { data: FavoritesPageData; form?: FavoritesActionData | null } = $props();

  /** 客户端追加的后续页（首页来自 SSR data.items）。 */
  let extraPages = $state<PostSummary[]>([]);
  /** loadMore 已推进到的游标；undefined = 尚未推进（回退 data.nextCursor）。 */
  let loadedCursor = $state<string | null | undefined>(undefined);
  let loadingMore = $state(false);
  let loadFailed = $state(false);

  const items = $derived([...data.items, ...extraPages]);
  /** 下一页游标：未推进时用服务端视图的游标（SSR 也可渲染加载更多）。 */
  const cursor = $derived(loadedCursor === undefined ? data.nextCursor : loadedCursor);
  /** action 结果文案（无 JS 整页刷新时可见；JS 下另有 toast）。 */
  const actionMessage = $derived(form?.message ?? null);

  // load 数据变化（导航 / invalidateAll）后重置客户端累积页，回到服务端视图。
  $effect(() => {
    void data.items;
    void data.after;
    extraPages = [];
    loadedCursor = undefined;
    loadFailed = false;
  });

  // 瞬态服务端错误（5xx/429）→ 全局 Toast 提示 + 页面只留「加载失败·重试」
  // 占位（产品约定：不整页展示错误态）；持续性错误仍走 ProblemState。
  $effect(() => {
    void data.problem;
    announceTransientProblem(data.problem);
  });

  /** JS 下追加下一页；无 JS 走 ?after= 链接（整页翻页）。 */
  async function loadMore(event?: MouseEvent): Promise<void> {
    event?.preventDefault();
    if (!cursor || loadingMore) return;
    loadingMore = true;
    loadFailed = false;
    try {
      const page = await listMyFavorites(fetch, cursor);
      extraPages = [...extraPages, ...(page.items ?? [])];
      loadedCursor = page.next_cursor ?? null;
      loadFailed = false;
    } catch {
      loadFailed = true;
      show('加载更多失败，请重试', 'danger');
    }
    loadingMore = false;
  }

  /** 行内展示作者标签：优先昵称，缺省回退用户名（嵌套投影优先，回退平面投影）。 */
  function authorLabel(post: PostSummary): string {
    return (
      post.author?.display_name ??
      post.author_display_name ??
      post.author?.username ??
      post.author_name ??
      '匿名'
    );
  }

  /** 后端时间戳为毫秒（M01-DB-08），formatRelative 口径为秒。 */
  function toSeconds(ts: number | null | undefined): number | null {
    if (typeof ts !== 'number' || !Number.isFinite(ts) || ts <= 0) return null;
    return ts > 1e11 ? Math.floor(ts / 1000) : ts;
  }
</script>

  <PageTitle title="我的收藏" />

<div class="container page-content" id="page-favorites">
  <h1 class="u-visually-hidden">我的收藏</h1>

  {#if data.problem && isTransientProblem(data.problem)}
    <LoadFailureState onretry={() => void invalidateAll()} />
  {:else if data.problem}
    <ProblemState problem={data.problem} />
  {:else}
    <div class="card">
      <div class="card-header">
        <span class="card-title">我的收藏</span>
      </div>
      <div class="card-body" style="padding:0;">
        {#if actionMessage}
          <p class="input-hint" role="status" style="padding:var(--space-3) var(--space-4);margin:0;">{actionMessage}</p>
        {/if}
        {#if items.length === 0}
          <div class="favorites-empty-wrap" style="padding:var(--space-6);">
            <EmptyState
              icon="star"
              title="还没有收藏"
              desc="逛逛首页，遇到喜欢的帖子点收藏就能在这里找到"
            />
            <div style="text-align:center;margin-top:var(--space-3);">
              <Button text="去首页逛逛" variant="secondary" size="sm" href="/" />
            </div>
          </div>
        {:else}
          <div style="display:flex;flex-direction:column;">
            {#each items as post (post.id)}
              <div class="post-row" style="padding:var(--space-4);border-bottom:var(--border-default);display:flex;gap:var(--space-3);align-items:center;">
                <CosmeticAvatar
                  name={authorLabel(post)}
                  size="md"
                  presentation={post.author?.presentation_tokens}
                  avatarAttachmentId={post.author?.avatar_attachment_id ?? null}
                  seed={post.author?.username ?? post.author?.id ?? authorLabel(post)}
                />
                <div style="min-width:0;flex:1;">
                  <div style="font-weight:var(--weight-medium);overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">
                    <a href="/posts/{encodeURIComponent(post.id)}">{post.title}</a>
                  </div>
                  <div class="text-secondary" style="font-size:var(--text-sm);margin-top:2px;display:flex;gap:var(--space-2);align-items:center;flex-wrap:wrap;">
                    {#if post.board_slug && post.board_name}
                      <span
                        class="category-badge"
                        style="--cat-color:{boardVisuals(post.board_slug).color};"
                      >
                        <span class="category-badge-square"></span><span>{post.board_name}</span>
                      </span>
                    {/if}
                    <span><CosmeticName name={authorLabel(post)} presentation={post.author?.presentation_tokens} /></span>
                    <span>·</span>
                    <span>{formatCount(post.reply_count)} 回复</span>
                    <span>·</span>
                    <span>{formatCount(post.view_count)} 浏览</span>
                    <span>·</span>
                    <span>收藏于 {formatRelative(toSeconds(post.created_at))}</span>
                  </div>
                </div>
                <form
                  method="POST"
                  action="?/unfavorite"
                  style="flex-shrink:0;"
                  use:enhance={() => {
                    return async ({ result, update }) => {
                      if (result.type === 'success') {
                        show('已取消收藏', 'success');
                        await update();
                        await invalidateAll();
                      } else {
                        if (result.type === 'failure') {
                          show(
                            String(
                              (result.data as FavoritesActionData | undefined)?.message ??
                                '取消收藏失败'
                            ),
                            'danger'
                          );
                        }
                        await update();
                      }
                    };
                  }}
                >
                  <input type="hidden" name="post_id" value={post.id} />
                  <Button type="submit" text="取消收藏" variant="ghost" size="sm" />
                </form>
              </div>
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
                href="/favorites?after={encodeURIComponent(cursor)}"
                text={loadingMore ? '加载中…' : loadFailed ? '加载失败，点击重试' : '加载更多'}
                variant="secondary"
                size="sm"
                disabled={loadingMore}
                onclick={(event) => void loadMore(event)}
              />
            </div>
          {/if}
        {/if}
      </div>
    </div>
  {/if}
</div>
