<script lang="ts">
  // M03-UI-03 / 社交域·关注：/users/{username}/followers——粉丝列表页。
  //
  // - SSR 首页直出（+page.server.ts load，公开端点，无 JS 也可读）；
  // - 「加载更多」：JS 下客户端游标追加（FollowUserList），无 JS 回退
  //   ?after= 链接整页翻页；
  // - 粉丝 / 正在关注互为 segmented 切换；页头返回用户主页。
  //   入口：用户悬浮卡「粉丝」统计、用户主页 meta 行。
  import Seo from '$lib/components/Seo.svelte';
  import PageTitle from '$lib/components/PageTitle.svelte';
  import FollowUserList from '$lib/components/users/FollowUserList.svelte';
  import type { UserFollowersPageData } from './+page.server';

  let { data }: { data: UserFollowersPageData } = $props();

  // username 由 SSR load 注入（params.username），独立 SSR 渲染（vitest ssr）
  // 无 SvelteKit page 上下文 → 不读取 $app/state 兜底。
  const username = $derived(data.username);
  const profileUrl = $derived(`/users/${encodeURIComponent(username)}`);
</script>

<PageTitle title={`${username} 的粉丝`} />
<Seo
  title={`${username} 的粉丝`}
  description={`查看 ${username} 的粉丝列表`}
  noindex={true}
/>

<div class="container page-content" id="page-user-followers">
  <nav aria-label="返回用户主页" style="margin-bottom:var(--space-4);">
    <a
      class="text-secondary"
      href={profileUrl}
      style="display:inline-flex;align-items:center;gap:var(--space-1);font-size:var(--text-sm);text-decoration:none;"
    >
      ← {username} 的主页
    </a>
  </nav>

  <!-- 粉丝 / 正在关注 segmented 切换（复用用户主页 tabs 样式口径）。 -->
  <nav
    class="tabs-nav"
    aria-label="社交关系"
    style="display:flex;align-items:center;gap:var(--space-1);border-bottom:var(--border-default);overflow-x:auto;scrollbar-width:none;flex-wrap:nowrap;"
  >
    <a
      href={`${profileUrl}/followers`}
      class="tab-link"
      aria-current="page"
      style="padding:var(--space-2) var(--space-3);font-size:var(--text-sm);border-bottom:2px solid var(--color-brand);color:var(--color-text-primary);text-decoration:none;white-space:nowrap;flex-shrink:0;"
    >
      粉丝
    </a>
    <a
      href={`${profileUrl}/following`}
      class="tab-link"
      style="padding:var(--space-2) var(--space-3);font-size:var(--text-sm);border-bottom:2px solid transparent;color:var(--color-text-secondary);text-decoration:none;white-space:nowrap;flex-shrink:0;"
    >
      正在关注
    </a>
  </nav>

  <div class="card" style="margin-top:var(--space-4);">
    <div class="card-body" style="padding:0;">
      <FollowUserList
        username={username}
        direction="followers"
        items={data.items}
        nextCursor={data.nextCursor}
        emptyTitle="还没有粉丝"
        emptyDesc="发布优质内容，被关注后粉丝会出现在这里"
        listLabel={`${username} 的粉丝列表`}
      />
    </div>
  </div>
</div>
