<script lang="ts">
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';
  import Icon from './ui/Icon.svelte';
  import Avatar from './ui/Avatar.svelte';
  import Button from './ui/Button.svelte';
  import { formatRelative } from '$lib/utils';
  import type { Notification } from '$lib/api/client';
  import {
    readPreference,
    applyTheme,
    switchToNextTheme,
    preferenceLabel,
    type ThemePreference
  } from '$lib/theme';
  import { show as showToast } from '$lib/ui/toast';

  let {
    user,
    siteName = 'BBLBB',
    unread = 0,
    notifications = [],
    onlogout,
    onreadall
  }: {
    user: { username: string; display_name?: string | null; level?: number; roles?: string[] } | null;
    /** 品牌名（后台系统设置 site_name；由 +layout.svelte 提供）。 */
    siteName?: string;
    unread?: number;
    /** 铃铛下拉速览（最近几条通知；由 +layout.svelte 提供）。 */
    notifications?: Notification[];
    onlogout?: () => void;
    /** 「全部已读」回调（调 API + 更新徽标态；由 +layout.svelte 提供）。 */
    onreadall?: () => void | Promise<void>;
  } = $props();

  let userMenuOpen = $state(false);
  let mobileOpen = $state(false);
  let notifOpen = $state(false);
  let moreOpen = $state(false);

  // ── 主题切换（全局壳）──────────────────────────────────────────────
  // SSR 初始为 null（不触碰 document）；挂载后从 localStorage 读偏好，
  // 切换时写 localStorage + html.light/dark class + theme-color meta 并
  // toast 反馈（见 $lib/theme.ts，与 app.html 防闪烁脚本同一约定）。
  let themePref = $state<ThemePreference | null>(null);
  let resolvedDark = $state(false);

  const themeIcon = $derived(
    themePref === 'dark' ? 'moon' : themePref === 'light' ? 'sun' : 'monitor'
  );

  onMount(() => {
    themePref = readPreference();
    resolvedDark = document.documentElement.classList.contains('dark');
    // 「跟随系统」偏好时，OS 主题变化实时跟随（显式偏好不受影响）。
    const mq =
      typeof window.matchMedia === 'function'
        ? window.matchMedia('(prefers-color-scheme: dark)')
        : null;
    const onChange = () => {
      themePref = readPreference();
      if (themePref === 'system') {
        resolvedDark = applyTheme('system') === 'dark';
      } else {
        resolvedDark = document.documentElement.classList.contains('dark');
      }
    };
    mq?.addEventListener?.('change', onChange);
    return () => mq?.removeEventListener?.('change', onChange);
  });

  function toggleTheme() {
    const { preference, resolved } = switchToNextTheme();
    themePref = preference;
    resolvedDark = resolved === 'dark';
    const message =
      preference === 'system'
        ? '主题已跟随系统设置'
        : resolved === 'dark'
          ? '已切换为暗色主题'
          : '已切换为亮色主题';
    showToast(message, preference === 'system' ? 'info' : 'success');
  }

  interface NavItem {
    label: string;
    href: string;
    icon: string;
    /** 仅登录用户可见（未登录不展示入口）。 */
    authOnly?: boolean;
  }

  // 主导航项：1:1 对齐原型 .desktop-nav（首页 / 发现 / 消息）。
  // authOnly 项未登录不展示（菜单隐藏不是安全边界，/messages 等私有路由
  // 仍由服务端 401 → /login 强制）。
  const navItems: NavItem[] = [
    { label: '首页', href: '/', icon: 'house' },
    { label: '发现', href: '/discover', icon: 'compass' },
    { label: '消息', href: '/messages', icon: 'mail', authOnly: true }
  ];

  // 「更多」下拉（全局壳）：包含搜索、板块、标签与各功能模块入口。
  // 商城/市场/成就/下载账单/API 密钥后端均为 x-permission: authenticated，
  // 未登录投影时由 visibleMoreItems 过滤掉。
  const moreItems: NavItem[] = [
    { label: '搜索', href: '/search', icon: 'search' },
    { label: '板块', href: '/boards', icon: 'layout-dashboard' },
    { label: '标签', href: '/tags', icon: 'tag' },
    { label: '商城', href: '/shop', icon: 'shopping-bag', authOnly: true },
    { label: '市场', href: '/marketplace', icon: 'package', authOnly: true },
    { label: '成就', href: '/achievements', icon: 'trophy', authOnly: true },
    { label: '下载账单', href: '/me/billing', icon: 'download', authOnly: true },
    { label: 'API 密钥', href: '/apikeys', icon: 'key', authOnly: true }
  ];

  // 未登录投影：过滤 authOnly 项（访客只留公开浏览入口；会话态由
  // +layout.svelte 按路径刷新，登录/退出后菜单即时增减）。
  const visibleNavItems = $derived(user ? navItems : navItems.filter((i) => !i.authOnly));
  const visibleMoreItems = $derived(user ? moreItems : moreItems.filter((i) => !i.authOnly));

  const path = $derived(page.url.pathname);

  // M13-ADMIN-01：按真实 Session 投影生成管理入口（菜单隐藏不是安全边界，
  // /admin 页面仍由服务端 user.manage 等权限门强制）。
  const canAdmin = $derived(Array.isArray(user?.roles) && user.roles.includes('administrator'));

  function isActive(href: string): boolean {
    if (href === '/') return path === '/';
    return path === href || path.startsWith(href + '/');
  }

  function submitSearch(event: SubmitEvent) {
    event.preventDefault();
    const form = event.currentTarget as HTMLFormElement;
    const input = form.querySelector('input') as HTMLInputElement | null;
    const q = (input?.value || '').trim();
    goto(q ? `/search?q=${encodeURIComponent(q)}` : '/search');
  }

  function closeMenus() {
    userMenuOpen = false;
    mobileOpen = false;
    notifOpen = false;
    moreOpen = false;
  }

  // 原型有而前端缺：菜单外点关闭 + Escape 关闭（统一作用于用户菜单 /
  // 通知下拉 / 更多下拉 / 移动端抽屉）。data-nav-menu 标记菜单锚定区域，
  // 点击区域外即关闭。
  function onWindowClick(event: MouseEvent) {
    const target = event.target as Element | null;
    if (target && typeof target.closest === 'function' && !target.closest('[data-nav-menu]')) {
      closeMenus();
    }
  }

  function onWindowKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') closeMenus();
  }

  async function onReadAll() {
    // 徽标/速览态与 toast 由 +layout.svelte 的回调更新；下拉保持打开，
    // 即时反映「全部已读」后的状态。
    await onreadall?.();
  }
</script>

<svelte:window onclick={onWindowClick} onkeydown={onWindowKeydown} />

<header class="navbar">
  <div class="container nav-container">
    <div class="nav-left" data-nav-menu>
      <button
        type="button"
        class="mobile-menu-btn"
        aria-label="菜单"
        aria-expanded={mobileOpen}
        onclick={() => (mobileOpen = !mobileOpen)}
      >
        <Icon name="menu" size={20} />
      </button>
      <a href="/" class="nav-logo" onclick={closeMenus}>{siteName}</a>
      <nav class="nav-items" aria-label="主导航">
        {#each visibleNavItems as item}
          <a
            href={item.href}
            class="nav-link {isActive(item.href) ? 'is-active' : ''}"
            aria-current={isActive(item.href) ? 'page' : undefined}
          ><Icon name={item.icon} size={18} />{item.label}</a>
        {/each}
        <div class="nav-more-wrapper">
          <button
            type="button"
            class="nav-link nav-more-btn"
            aria-haspopup="menu"
            aria-expanded={moreOpen}
            aria-label="更多导航"
            onclick={() => {
              moreOpen = !moreOpen;
              userMenuOpen = false;
              notifOpen = false;
            }}
          >
            更多
            <Icon name="chevron-down" size={14} />
          </button>
          {#if moreOpen}
            <div class="dropdown more-menu" role="menu" aria-label="更多导航">
              {#each visibleMoreItems as item (item.href)}
                <a
                  class="dropdown-item"
                  role="menuitem"
                  href={item.href}
                  aria-current={isActive(item.href) ? 'page' : undefined}
                  onclick={closeMenus}
                >
                  <Icon name={item.icon} size={16} />
                  <span>{item.label}</span>
                </a>
              {/each}
            </div>
          {/if}
        </div>
      </nav>
    </div>

    <div class="nav-center">
      <form class="search-form" role="search" onsubmit={submitSearch}>
        <span class="search-icon" aria-hidden="true">
          <Icon name="search" size={16} />
        </span>
        <input
          type="search"
          name="q"
          placeholder="搜索帖子、用户、标签…"
          class="search-input"
          aria-label="搜索帖子、用户和标签"
          autocomplete="off"
        />
      </form>
    </div>

    <div class="nav-right">
      <a href="/search" class="nav-icon-btn mobile-search-btn" aria-label="搜索">
        <Icon name="search" size={18} />
      </a>
      <button
        type="button"
        class="nav-icon-btn"
        aria-label="切换主题"
        aria-pressed={resolvedDark}
        title={themePref ? `切换主题（当前：${preferenceLabel(themePref)}）` : '切换主题'}
        onclick={toggleTheme}
      >
        <Icon name={themeIcon} size={18} />
      </button>
      <div class="user-menu-wrapper" data-nav-menu>
        {#if user}
          <button
            type="button"
            class="nav-icon-btn"
            aria-label="通知"
            aria-haspopup="menu"
            aria-expanded={notifOpen}
            onclick={() => {
              notifOpen = !notifOpen;
              userMenuOpen = false;
              moreOpen = false;
            }}
          >
            <Icon name="bell" size={18} />
            {#if unread > 0}
              <span class="nav-notif-dot">{unread > 99 ? '99+' : unread}</span>
            {/if}
          </button>
          {#if notifOpen}
            <div class="dropdown notif-menu" role="menu" aria-label="通知速览">
              <div class="notif-menu-head">
                <span class="notif-menu-title">
                  通知{#if unread > 0}<b class="notif-menu-count">{unread} 条未读</b>{/if}
                </span>
                {#if unread > 0}
                  <button type="button" class="notif-markall-btn" onclick={onReadAll}>全部已读</button>
                {/if}
              </div>
              <div class="dropdown-sep"></div>
              {#if notifications.length === 0}
                <p class="notif-empty">暂无通知</p>
              {:else}
                {#each notifications as item (item.id)}
                  <a
                    class="dropdown-item notif-item"
                    role="menuitem"
                    href={item.unavailable || !item.link ? '/notifications' : item.link}
                    onclick={closeMenus}
                  >
                    <span class="notif-item-title">{item.title}</span>
                    <span class="notif-item-time">{formatRelative(item.created_at)}</span>
                  </a>
                {/each}
              {/if}
              <div class="dropdown-sep"></div>
              <a class="dropdown-item" role="menuitem" href="/notifications" onclick={closeMenus}>
                <Icon name="inbox" size={16} />
                <span>查看全部</span>
              </a>
            </div>
          {/if}
        {/if}
      </div>
      <div class="user-menu-wrapper" data-nav-menu>
        {#if user}
          <button
            type="button"
            class="user-avatar-btn"
            aria-label="用户菜单"
            aria-expanded={userMenuOpen}
            onclick={(e) => {
              e.stopPropagation();
              userMenuOpen = !userMenuOpen;
              notifOpen = false;
              moreOpen = false;
            }}
          >
            <Avatar name={user.display_name || user.username} size="md" />
          </button>
          {#if userMenuOpen}
            <div class="dropdown user-menu" role="menu" aria-label="用户菜单">
              <div class="user-menu-header">
                <Avatar name={user.display_name || user.username} size="lg" />
                <div>
                  <div class="user-menu-name">{user.display_name || user.username}</div>
                  <div class="user-menu-level">LV.{user.level ?? 1}</div>
                </div>
              </div>
              <div class="dropdown-sep"></div>
              <a href="/me" class="dropdown-item" role="menuitem" onclick={closeMenus}><Icon name="user" size={16} /><span>我的主页</span></a>
              <a href="/favorites" class="dropdown-item" role="menuitem" onclick={closeMenus}><Icon name="bookmark" size={16} /><span>我的收藏</span></a>
              <a href="/editor" class="dropdown-item" role="menuitem" onclick={closeMenus}><Icon name="pen-line" size={16} /><span>发布内容</span></a>
              <a href="/settings" class="dropdown-item" role="menuitem" onclick={closeMenus}><Icon name="settings" size={16} /><span>账号设置</span></a>
              {#if canAdmin}
                <a href="/admin" class="dropdown-item" role="menuitem" onclick={closeMenus}><Icon name="shield-check" size={16} /><span>管理后台</span></a>
              {/if}
              <div class="dropdown-sep"></div>
              <button type="button" class="dropdown-item is-danger" role="menuitem" onclick={() => { closeMenus(); onlogout?.(); }}>
                <Icon name="log-out" size={16} /><span>退出登录</span>
              </button>
            </div>
          {/if}
        {:else}
          <a href="/login" class="nav-link">登录</a>
          <a href="/register" class="nav-link">注册</a>
        {/if}
      </div>
    </div>
  </div>
</header>

{#if mobileOpen}
  <div
    class="drawer-overlay"
    role="button"
    tabindex="-1"
    aria-label="关闭菜单"
    onclick={closeMenus}
    onkeydown={(e) => {
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        closeMenus();
      }
    }}
  >
    <div
      class="drawer"
      role="dialog"
      aria-modal="true"
      aria-label="菜单"
      tabindex="-1"
      data-nav-menu
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => {
        if (e.key === 'Escape') closeMenus();
      }}
    >
      <div class="drawer-header">
        <span class="drawer-title">菜单</span>
        <button type="button" class="drawer-close" aria-label="关闭菜单" onclick={closeMenus}><Icon name="x" size={20} /></button>
      </div>
      <nav class="drawer-nav" aria-label="移动端导航">
        {#each visibleNavItems as item}
          <a href={item.href} class="drawer-nav-item" onclick={closeMenus}>{item.label}</a>
        {/each}
        <div class="drawer-divider"></div>
        {#if user}
          <a href="/me" class="drawer-nav-item" onclick={closeMenus}>我的主页</a>
          <a href="/favorites" class="drawer-nav-item" onclick={closeMenus}>我的收藏</a>
          <a href="/editor" class="drawer-nav-item" onclick={closeMenus}>发布内容</a>
          <a href="/settings" class="drawer-nav-item" onclick={closeMenus}>账号设置</a>
          <a href="/notifications" class="drawer-nav-item" onclick={closeMenus}>通知</a>
          <div class="drawer-divider"></div>
          <button type="button" class="drawer-nav-item is-danger" onclick={() => { closeMenus(); onlogout?.(); }}>退出登录</button>
        {:else}
          <a href="/login" class="drawer-nav-item" onclick={closeMenus}>登录</a>
          <a href="/register" class="drawer-nav-item" onclick={closeMenus}>注册</a>
        {/if}
      </nav>
    </div>
  </div>
{/if}
