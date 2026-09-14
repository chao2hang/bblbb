<script lang="ts">
  // M03-UI-03 / 社交域·关注：/users/{username}/following——正在关注列表页。
  //
  // 与 followers/+page.svelte 同构：SSR 首页直出 + JS「加载更多」游标追加
  // （无 JS 回退 ?after= 链接）；正在关注 / 粉丝 segmented 切换；返回用户主页。
  //   入口：用户悬浮卡「关注」统计、用户主页 meta 行。
  import Seo from '$lib/components/Seo.svelte';
  import PageTitle from '$lib/components/PageTitle.svelte';
  import FollowUserList from '$lib/components/users/FollowUserList.svelte';
  import type { UserFollowingPageData } from './+page.server';

  let { data }: { data: UserFollowingPageData } = $props();

  // username 由 SSR load 注入（params.username），独立 SSR 渲染（vitest ssr）
  // 无 SvelteKit page 上下文 → 不读取 $app/state 兜底。
  const username = $derived(data.username);
  const profileUrl = $derived(`/users/${encodeURIComponent(username)}`);
</script>

<PageTitle title={`${username} 正在关注`} />
<Seo
  title={`${username} 正在关注`}
  description={`查看 ${username} 正在关注的人`}
  noindex={true}
/>

<div class="container page-content" id="page-user-following">
  <nav aria-label="返回用户主页" style="margin-bottom:var(--space-4);">
    <a
      class="text-secondary"
      href={profileUrl}
      style="display:inline-flex;align-items:center;gap:var(--space-1);font-size:var(--text-sm);text-decoration:none;"
    >
      ← {username} 的主页
    </a>
  </nav>

  <!-- 正在关注 / 粉丝 segmented 切换（复用用户主页 tabs 样式口径）。 -->
  <nav
    class="tabs-nav"
    aria-label="社交关系"
    style="display:flex;align-items:center;gap:var(--space-1);border-bottom:var(--border-default);overflow-x:auto;scrollbar-width:none;flex-wrap:nowrap;"
  >
    <a
      href={`${profileUrl}/followers`}
      class="tab-link"
      style="padding:var(--space-2) var(--space-3);font-size:var(--text-sm);border-bottom:2px solid transparent;color:var(--color-text-secondary);text-decoration:none;white-space:nowrap;flex-shrink:0;"
    >
      粉丝
    </a>
    <a
      href={`${profileUrl}/following`}
      class="tab-link"
      aria-current="page"
      style="padding:var(--space-2) var(--space-3);font-size:var(--text-sm);border-bottom:2px solid var(--color-brand);color:var(--color-text-primary);text-decoration:none;white-space:nowrap;flex-shrink:0;"
    >
      正在关注
    </a>
  </nav>

  <div class="card" style="margin-top:var(--space-4);">
    <div class="card-body" style="padding:0;">
      <FollowUserList
        username={username}
        direction="following"
        items={data.items}
        nextCursor={data.nextCursor}
        emptyTitle="还没有关注任何人"
        emptyDesc="在帖子或用户卡片点「+ 关注」，关注的人会出现在这里"
        listLabel={`${username} 正在关注的用户列表`}
      />
    </div>
  </div>
</div>
