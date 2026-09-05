<script lang="ts">
  // 全局壳·移动端底部导航（≤768px）——照 prototype BOTTOM_NAV 的结构：
  // nav.bottom-nav + 5 项（首页 / 板块 / 发布（凸起 FAB）/ 消息 / 我的）。
  // 当前路由高亮用 aria-current="page"；发布项是凸起 FAB 按钮。
  // >768px 隐藏（桌面主导航在 Navbar）。
  import { page } from '$app/state';
  import Icon from './ui/Icon.svelte';

  let {
    user
  }: {
    /** 会话投影（仅用存在性判断登录态；由 +layout.svelte 提供）。 */
    user: { username: string; display_name?: string | null; level?: number; roles?: string[] } | null;
  } = $props();

  const path = $derived(page.url.pathname);

  function isActive(href: string): boolean {
    if (href === '/') return path === '/';
    return path === href || path.startsWith(href + '/');
  }

  /** 底部 tab 项（发布 FAB 在模板中单独渲染，保持中间位）。
   *  authOnly 项仅登录展示：未登录不显示「消息」（私有路由仍由服务端
   *  401 → /login 强制）；「我的」保留为登录入口（同原型 guest 行为）。 */
  const tabs: { label: string; href: string; icon: string; authOnly?: boolean }[] = [
    { label: '首页', href: '/', icon: 'house' },
    // 对齐原型 BOTTOM_NAV（prototype/assets/page-chrome.js）：第二项为
    // 「发现」（罗盘图标）。板块入口在首页分类栏与桌面导航，不在底部导航。
    { label: '发现', href: '/discover', icon: 'compass' },
    { label: '消息', href: '/messages', icon: 'mail', authOnly: true },
    { label: '我的', href: '/me', icon: 'user' }
  ];

  const visibleTabs = $derived(tabs.filter((t) => user || !t.authOnly));
</script>

<nav class="bottom-nav" aria-label="移动端底部导航">
  {#each visibleTabs.slice(0, 2) as tab (tab.href)}
    <a href={tab.href} class="bottom-nav-item" aria-current={isActive(tab.href) ? 'page' : undefined}>
      <Icon name={tab.icon} size={22} />
      <span>{tab.label}</span>
    </a>
  {/each}

  {#if user}
    <a href="/editor" class="bottom-nav-fab" aria-label="发布" aria-current={isActive('/editor') ? 'page' : undefined}>
      <Icon name="plus" size={26} />
    </a>
  {/if}

  {#each visibleTabs.slice(2) as tab (tab.href)}
    <a href={tab.href} class="bottom-nav-item" aria-current={isActive(tab.href) ? 'page' : undefined}>
      <Icon name={tab.icon} size={22} />
      <span>{tab.label}</span>
    </a>
  {/each}
</nav>

<style>
  /* 原型 chinese-elegance：.bottom-nav 为不透明卡片底 + 1px 实上边 +
     --shadow-subtle，无毛玻璃（与桌面顶栏 .desktop-header 同规范）。 */
  .bottom-nav {
    position: fixed;
    bottom: 0;
    left: 0;
    right: 0;
    z-index: var(--z-sticky);
    display: none;
    align-items: center;
    justify-content: space-around;
    height: calc(64px + env(safe-area-inset-bottom, 0px));
    padding: 0 0 env(safe-area-inset-bottom, 0px);
    background: var(--color-bg-card);
    border-top: var(--border-default);
    box-shadow: var(--shadow-subtle);
  }

  .bottom-nav-item {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 3px;
    min-width: 64px;
    height: 52px;
    color: var(--color-text-secondary);
    font-size: var(--text-xs);
    text-decoration: none;
    border-radius: var(--radius-md);
    transition: color var(--duration-fast) var(--ease-out);
  }

  .bottom-nav-item:hover {
    color: var(--color-text-primary);
    text-decoration: none;
  }

  .bottom-nav-item[aria-current='page'] {
    color: var(--color-brand);
    font-weight: var(--weight-medium);
  }

  .bottom-nav-item :global(.icon) {
    color: currentColor;
  }

  /* 凸起发布 FAB（照 prototype BOTTOM_NAV 中间按钮） */
  .bottom-nav-fab {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 50px;
    height: 50px;
    margin-top: -18px;
    background: var(--color-brand);
    color: var(--color-text-on-brand);
    border-radius: var(--radius-full);
    box-shadow: 0 6px 16px color-mix(in srgb, var(--color-brand) 40%, transparent);
    transition: background var(--duration-fast) var(--ease-out),
      transform var(--duration-fast) var(--ease-out);
  }

  .bottom-nav-fab:hover {
    background: var(--color-brand-hover);
    transform: translateY(-1px);
  }

  .bottom-nav-fab:active {
    transform: translateY(0);
  }

  .bottom-nav-fab[aria-current='page'] {
    background: var(--color-brand-pressed);
  }

  @media (max-width: 767px) {
    .bottom-nav {
      display: flex;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .bottom-nav-fab:hover,
    .bottom-nav-fab:active {
      transform: none;
    }
  }
</style>
