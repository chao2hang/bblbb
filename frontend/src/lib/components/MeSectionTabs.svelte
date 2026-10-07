<script lang="ts">
  // M02-UX-05：个人中心分区 tag 栏（每个 tag = 独立路由页面）。
  //
  // - 账户资产 → /me（概览枢纽：资产 + 快捷入口）
  // - 账号安全 → /me/security（登录密码 / 两步验证 / OAuth / 登录设备）
  // - 信任等级 → /me/level（社区信任等级中心 TL0–TL4）
  //
  // 当前项由 URL 推导（aria-current='page'），SSR 直出、无 JS 可用；
  // 点击即整页路由切换，不做页内锚点滚动。
  // 桌面端隐藏（桌面用侧栏 / 设置导航）；≤767px 为吸顶横滑轨，
  // 与首页分类轨（mobile.css §2a）、用户页 tabs-nav 同一视觉语言。
  import { page } from '$app/state';

  interface MeSectionTab {
    href: string;
    label: string;
    /** 命中即高亮（含子路径）。 */
    isActive: (pathname: string) => boolean;
  }

  const tabs: MeSectionTab[] = [
    { href: '/me', label: '账户资产', isActive: (p) => p === '/me' },
    { href: '/me/security', label: '账号安全', isActive: (p) => p.startsWith('/me/security') },
    { href: '/me/level', label: '信任等级', isActive: (p) => p.startsWith('/me/level') }
  ];

  const pathname = $derived(page.url.pathname);
</script>

<nav class="me-tabs" aria-label="个人中心分区">
  {#each tabs as tab (tab.href)}
    <a
      href={tab.href}
      class="me-tab"
      class:is-active={tab.isActive(pathname)}
      aria-current={tab.isActive(pathname) ? 'page' : undefined}
    >
      {tab.label}
    </a>
  {/each}
</nav>

<style>
  /* 桌面默认隐藏：桌面端个人中心用侧栏/行内链接导航，不需要分区 tag 轨 */
  .me-tabs {
    display: none;
  }
  .me-tab {
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    height: 42px;
    padding: 0 13px;
    font-size: 13px;
    color: var(--color-text-secondary);
    text-decoration: none;
    white-space: nowrap;
    border-bottom: 2px solid transparent;
    transition: color 0.15s ease, border-color 0.15s ease;
  }
  .me-tab:hover {
    color: var(--color-text-primary);
  }
  .me-tab.is-active {
    color: var(--color-text-primary);
    font-weight: var(--weight-semibold);
    border-bottom-color: var(--color-brand);
  }

  @media (max-width: 767px) {
    .me-tabs {
      display: flex;
      position: sticky;
      top: var(--mobile-header-h, 52px);
      z-index: 12;
      gap: 0;
      width: 100%;
      height: 42px;
      margin: 0 0 10px;
      padding: 0 4px;
      overflow-x: auto;
      scrollbar-width: none;
      background: var(--color-bg-page);
      border-bottom: 1px solid var(--color-border);
      -webkit-overflow-scrolling: touch;
      /* /me 紧凑资料条滑入/滑出时，top 随 :has() 联动平滑过渡 */
      transition: top 0.22s ease;
    }
    .me-tabs::-webkit-scrollbar {
      display: none;
    }
    .me-tab {
      height: 42px;
      min-height: 42px;
      padding: 0 13px;
      font-size: 13px;
    }
  }
</style>
