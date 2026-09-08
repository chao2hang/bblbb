<script lang="ts">
  // M03-UI-06：板块详情 SSR——板块信息 + 帖子列表 + 权限提示 + 空状态。
  // 原型对齐：prototype/pages/board.html
  // - 统一 .app-route-head（COMMUNITY / SLUG）
  // - .app-grid 两栏布局（主栏 + 300px 侧栏）
  // - 主栏：app-card + app-filter-tabs + 关注/发帖操作 + app-post-list
  // - 侧栏：板块信息卡 + 热门标签（app-tag-cloud）
  import { enhance } from '$app/forms';
  import { invalidateAll } from '$app/navigation';
  import PostList from '$lib/components/PostList.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { formatCount } from '$lib/utils';
  import { show } from '$lib/ui/toast';
  import { tagSearchUrl } from '$lib/search';
  import Seo from '$lib/components/Seo.svelte';
  import type { BoardDetailData, BoardFollowActionData } from './+page.server';
  import { resolveSiteCopy, type SiteCopyView } from '$lib/site/copy';

  let { data, form }: { data: BoardDetailData & { site?: SiteCopyView | null }; form?: BoardFollowActionData | null } = $props();

  const board = $derived(data.board);
  const posts = $derived(data.posts);
  const error = $derived(data.error);
  const slug = $derived(board?.slug ?? '');
  const tags = $derived(data.tags ?? []);

  /** 排序 tab（M18-BOARD-02 对齐原型：最新/热门/精华/未回复）；
   *  链接式（无 JS 可用），后端 GET /boards/{slug}/posts?sort=…。 */
  const sortTabs = [
    { value: 'latest' as const, label: '最新' },
    { value: 'hot' as const, label: '热门' },
    { value: 'featured' as const, label: '精华' },
    { value: 'unanswered' as const, label: '未回复' }
  ];

  /** 筛选词保留态（tab 链接带 q，切换排序不丢筛选）。 */
  const qSuffix = $derived(data.q ? `&q=${encodeURIComponent(data.q)}` : '');

  /** 空态文案按排序区分。 */
  const emptyTitle = $derived.by(() => {
    if (data.q) return '没有匹配的帖子';
    if (data.sort === 'hot') return '暂无热门帖';
    if (data.sort === 'featured') return '暂无精华帖';
    if (data.sort === 'unanswered') return '暂无未回复帖';
    return '暂无帖子';
  });

  /** 权限提示：非公开板块对匿名/非成员不可见（members 需登录、
   *  restricted 需角色）；readonly/closed 提示只读。 */
  const permissionHint = $derived.by(() => {
    if (!board) return null;
    const hints: string[] = [];
    if (board.visibility === 'members') hints.push('该板块仅对登录成员可见');
    if (board.visibility === 'restricted') hints.push('该板块需加入后可见');
    if (board.visibility === 'hidden') hints.push('该板块仅对具备权限的管理员/版主可见');
    if (board.posting_mode === 'readonly') hints.push('该板块当前为只读，不能发布新帖');
    if (board.posting_mode === 'closed') hints.push('该板块已关闭发帖');
    if (board.posting_mode === 'approval') hints.push('发帖需审核后展示');
    return hints.length ? hints : null;
  });

  const indexable = $derived(
    Boolean(
      board &&
        (board.visibility ?? 'public') === 'public' &&
        board.is_active !== 0
    )
  );

  function followEnhance() {
    return async ({ result, update }: { result: { type: string; data?: unknown }; update: () => Promise<void> }) => {
      if (result.type === 'success') {
        const actionData = result.data as BoardFollowActionData | undefined;
        show(actionData?.message ?? '操作成功', 'success');
        await update();
        await invalidateAll();
      } else if (result.type === 'failure') {
        const actionData = result.data as BoardFollowActionData | undefined;
        show(actionData?.message ?? '操作失败，请稍后重试', 'danger');
        await update();
      } else {
        await update();
      }
    };
  }
</script>

<Seo
  title={board?.name ?? slug}
  description={board?.description ?? '板块：' + (board?.name ?? slug)}
  noindex={!indexable}
  og={{ type: 'website' }}
  jsonLd={
    indexable
      ? {
          '@context': 'https://schema.org',
          '@type': 'CollectionPage',
          name: board!.name,
          description: board!.description
        }
      : null
  }
/>

<div class="container app-page">
  <section class="page app-page app-route-board" id="page-board">
    {#if error && !board}
      <p class="input-hint is-error" role="alert">{error}</p>
    {/if}

    {#if board}
      <div class="app-route-head">
        <div class="app-route-head__copy">
          <span class="app-kicker">COMMUNITY / {board.slug.toUpperCase()}</span>
          <h1 tabindex="-1">{board.name}</h1>
          <p>{board.description || '按兴趣进入社区的不同讨论空间。'}</p>
        </div>
      </div>

      <div class="app-grid">
        <div class="app-stack">
          {#if permissionHint}
            <div class="app-notice" role="note" style="margin-bottom:14px;">
              <div>
                {#each permissionHint as hint}
                  <p style="margin:0;">{hint}</p>
                {/each}
              </div>
            </div>
          {/if}

          <section class="app-card">
            <div style="display:flex;align-items:center;justify-content:space-between;gap:10px;flex-wrap:wrap;padding:0 18px;border-bottom:1px solid var(--color-border);">
              <div class="app-filter-tabs" role="tablist">
                {#each sortTabs as tab (tab.value)}
                  <a
                    class="tab {data.sort === tab.value ? 'is-active' : ''}"
                    style="height:42px;display:inline-flex;align-items:center;padding:0 13px;text-decoration:none;font-size:13px;color:var(--color-text-secondary);border-bottom:2px solid {data.sort === tab.value ? 'var(--color-brand)' : 'transparent'};font-weight:{data.sort === tab.value ? '600' : '400'};"
                    href="/boards/{encodeURIComponent(board.slug)}?sort={tab.value}{qSuffix}"
                    aria-current={data.sort === tab.value ? 'page' : undefined}
                  >
                    {tab.label}
                  </a>
                {/each}
              </div>

              <div style="display:flex;gap:8px;align-items:center;padding:8px 0;">
                {#if data.authed}
                  <form
                    method="POST"
                    action={data.following ? '?/unfollow' : '?/follow'}
                    use:enhance={followEnhance}
                    style="display:inline-flex;"
                  >
                    <button
                      type="submit"
                      class="btn {data.following ? 'secondary' : 'ghost'} sm"
                    >
                      <Icon name={data.following ? 'check' : 'plus'} size={14} />
                      {data.following ? '已关注' : '关注板块'}
                    </button>
                  </form>
                {:else}
                  <!-- 匿名：关注是登录操作，渲染登录引导（?next= 回跳本板块）。 -->
                  <a href="/login?next={encodeURIComponent(`/boards/${data.board?.slug ?? ''}`)}" class="btn ghost sm">
                    <Icon name="log-in" size={14} />
                    登录后关注
                  </a>
                {/if}
                <a href="/editor" class="btn primary sm">
                  发布讨论
                </a>
              </div>
            </div>

            <!-- M18-BOARD-02：作者/标题筛选（原型「按作者或标题筛选…」+ 清除）。
                 GET 表单无 JS 可用；筛选词在切换排序 tab 时通过 qSuffix 保留。 -->
            <form
              method="GET"
              action="/boards/{encodeURIComponent(board.slug)}"
              aria-label="板块帖子筛选"
              style="display:flex;align-items:center;gap:8px;flex-wrap:wrap;padding:10px 18px;border-bottom:1px solid var(--color-border);"
            >
              <input type="hidden" name="sort" value={data.sort} />
              <input
                type="search"
                name="q"
                value={data.q}
                class="input-field"
                placeholder="按作者或标题筛选…"
                aria-label="按作者或标题筛选板块帖子"
                maxlength="100"
                style="flex:1;min-width:150px;height:34px;"
              />
              <button type="submit" class="btn secondary sm">筛选</button>
              {#if data.q}
                <a
                  href="/boards/{encodeURIComponent(board.slug)}?sort={data.sort}"
                  class="btn ghost sm"
                >
                  清除
                </a>
              {/if}
            </form>

            <div class="app-post-list">
              <PostList
                posts={posts}
                emptyTitle={emptyTitle}
                emptyDesc="成为第一个发帖的人吧！"
              />
            </div>
          </section>
        </div>

        <aside class="app-stack">
          <section class="app-card">
            <header class="app-card__head">
              <h2>板块信息</h2>
            </header>
            <div class="app-card__body">
              <p class="app-muted" style="margin:0 0 6px;">
                主题总数 <strong>{formatCount(board.post_count)}</strong>
              </p>
              <p class="app-muted" style="margin:0;">
                今日新增 <strong>{formatCount(board.today_post_count ?? 0)}</strong>
              </p>
            </div>
          </section>

          {#if tags.length > 0}
            <section class="app-card">
              <header class="app-card__head">
                <h2>热门标签</h2>
              </header>
              <div class="app-card__body">
                <div class="app-tag-cloud">
                  {#each tags as tag (tag.id)}
                    <a class="app-tag" href={tagSearchUrl(tag)}>
                      # {tag.name}
                    </a>
                  {/each}
                </div>
              </div>
            </section>
          {/if}
        </aside>
      </div>
    {/if}
  </section>
</div>
