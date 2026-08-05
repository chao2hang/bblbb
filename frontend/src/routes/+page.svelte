<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/state';
  import { listBoards, listTags, search, getMe, type Board, type Tag, type PostSummary } from '$lib/api/client';
  import SectionHeader from '$lib/components/SectionHeader.svelte';
  import PostList from '$lib/components/PostList.svelte';
  import BoardCard from '$lib/components/BoardCard.svelte';
  import { boardVisuals } from '$lib/board-visuals';
  import Card from '$lib/components/ui/Card.svelte';
  import Avatar from '$lib/components/ui/Avatar.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import TagChip from '$lib/components/ui/Tag.svelte';

  let boards = $state<Board[]>([]);
  let tags = $state<Tag[]>([]);
  let posts = $state<PostSummary[]>([]);
  let user = $state<{ username: string; display_name?: string | null } | null>(null);
  let loading = $state(true);

  onMount(async () => {
    user = await getMe(fetch);
    const [boardResult, tagResult] = await Promise.allSettled([listBoards(fetch), listTags(fetch)]);
    if (boardResult.status === 'fulfilled') boards = boardResult.value.items;
    if (tagResult.status === 'fulfilled') tags = tagResult.value.items;

    // 最新讨论：使用公开搜索拉取全部公开帖子（后端 LIKE '%'）
    try {
      const result = await search(fetch, '', 8);
      posts = result.items;
    } catch {
      posts = [];
    }
    loading = false;
  });
</script>

<svelte:head>
  <title>BBLBB — 社区论坛</title>
  <meta name="description" content="BBLBB 社区论坛首页" />
</svelte:head>

<div class="container">
  <div class="page-content content-grid home-content">
    <div class="main-col home-main">
      <section class="content-section home-discussions">
        <SectionHeader title="最新讨论" desc="社区正在发生的讨论" moreHref="/boards" />
        <div class="section-surface">
          {#if loading}
            <div class="empty-state"><div class="empty-state-title">加载中…</div></div>
          {:else}
            <PostList posts={posts} emptyTitle="暂无帖子" emptyDesc="成为第一个发帖的人吧！" />
          {/if}
        </div>
      </section>

      <section class="content-section">
        <SectionHeader title="板块" desc="选择你感兴趣的板块" moreHref="/boards" />
        {#if boards.length > 0}
          <div class="boards-grid home-boards">
            {#each boards as board}
              {@const visuals = boardVisuals(board.slug)}
              <BoardCard
                slug={board.slug}
                name={board.name}
                description={board.description ?? ''}
                post_count={board.post_count}
                icon={visuals.icon}
                color={visuals.color}
              />
            {/each}
          </div>
        {:else if !loading}
          <div class="empty-state"><div class="empty-state-title">暂无板块</div></div>
        {/if}
      </section>
    </div>

    <div class="side-col">
      {#if user}
        <div class="card">
          <div class="card-body" style="display:flex;align-items:center;gap:var(--space-3);">
            <Avatar name={user.display_name || user.username} size="lg" />
            <div style="min-width:0;">
              <div style="font-weight:var(--weight-semibold);">{user.display_name || user.username}</div>
              <div class="text-secondary" style="font-size:var(--text-sm);">欢迎回来</div>
            </div>
          </div>
        </div>
      {:else}
        <div class="card">
          <div class="card-header"><span class="card-title">加入 BBLBB</span></div>
          <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-3);">
            <p class="text-secondary" style="font-size:var(--text-sm);">登录后参与讨论，点亮你的社区身份。</p>
            <Button text="登录" variant="secondary" size="sm" href="/login" />
            <Button text="注册新账号" variant="primary" size="sm" href="/register" />
          </div>
        </div>
      {/if}

      {#if tags.length > 0}
        <div class="card">
          <div class="card-header"><span class="card-title">热门标签</span></div>
          <div class="card-body">
            <div class="tag-cloud">
              {#each tags as tag}
                <TagChip name={tag.name} count={tag.usage_count} href="/search?q={tag.name}" />
              {/each}
            </div>
          </div>
        </div>
      {/if}
    </div>
  </div>
</div>
