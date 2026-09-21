<script lang="ts">
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';
  import Icon from './ui/Icon.svelte';
  import Avatar from './ui/Avatar.svelte';
  import CosmeticAvatar from './wardrobe/CosmeticAvatar.svelte';
  import CosmeticName from './wardrobe/CosmeticName.svelte';
  import Button from './ui/Button.svelte';
  import type { PublicPresentationTokens } from '$lib/api/types';
  import { formatRelative } from '$lib/utils';
  import type { Notification, SearchPageView, SearchResultView } from '$lib/api/client';
  import { searchPublic } from '$lib/api/client';
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
    user: { id?: string; username: string; display_name?: string | null; level?: number; roles?: string[]; presentation_tokens?: PublicPresentationTokens | null; avatar_attachment_id?: string | null } | null;
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

  // ── 移动端半屏用户菜单（Bottom Sheet）交互状态 ────────────────────────
  let sheetTranslateY = $state(0);
  let touchStartY = 0;
  let isDragging = false;

  function handleTouchStart(e: TouchEvent) {
    if (e.touches.length > 0) {
      touchStartY = e.touches[0].clientY;
      isDragging = true;
    }
  }

  function handleTouchMove(e: TouchEvent) {
    if (!isDragging || e.touches.length === 0) return;
    const delta = e.touches[0].clientY - touchStartY;
    if (delta > 0) {
      sheetTranslateY = delta;
    } else {
      sheetTranslateY = 0;
    }
  }

  function handleTouchEnd() {
    if (!isDragging) return;
    isDragging = false;
    if (sheetTranslateY > 70) {
      closeMenus();
    }
    sheetTranslateY = 0;
  }

  // ── Header 即时搜索状态（内嵌浮层，不再跳转独立搜索页）───────────
  let searchOpen = $state(false);
  let searchQuery = $state('');
  let searchLoading = $state(false);
  let searchResults = $state<SearchResultView[]>([]);
  let searchActiveIndex = $state(0);
  let searchDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  let searchInputEl: HTMLInputElement | null = $state(null);

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

  // 「更多」下拉（全局壳）：包含板块、标签与各功能模块入口。
  // 商城/市场/成就/下载账单/API 密钥后端均为 x-permission: authenticated，
  // 未登录投影时由 visibleMoreItems 过滤掉。
  const moreItems: NavItem[] = [
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
    if (searchResults.length > 0 && searchActiveIndex >= 0 && searchActiveIndex < searchResults.length) {
      const selected = searchResults[searchActiveIndex];
      selectSearchResult(selected);
    } else if (searchResults.length > 0) {
      selectSearchResult(searchResults[0]);
    }
  }

  async function performSearch(q: string) {
    const trimmed = q.trim();
    if (!trimmed) {
      searchResults = [];
      searchLoading = false;
      return;
    }
    searchLoading = true;
    try {
      const res = await searchPublic(fetch, trimmed, { limit: 8 });
      searchResults = res.items || [];
      searchActiveIndex = 0;
    } catch {
      searchResults = [];
    } finally {
      searchLoading = false;
    }
  }

  function handleSearchInput(e: Event) {
    const target = e.target as HTMLInputElement;
    searchQuery = target.value;
    searchOpen = true;
    if (searchDebounceTimer) clearTimeout(searchDebounceTimer);
    searchDebounceTimer = setTimeout(() => {
      performSearch(searchQuery);
    }, 200);
  }

  function handleSearchKeyDown(e: KeyboardEvent) {
    if (!searchOpen) {
      if (e.key === 'ArrowDown') {
        searchOpen = true;
        e.preventDefault();
      }
      return;
    }

    if (e.key === 'ArrowDown') {
      e.preventDefault();
      if (searchResults.length > 0) {
        searchActiveIndex = (searchActiveIndex + 1) % searchResults.length;
      }
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      if (searchResults.length > 0) {
        searchActiveIndex = (searchActiveIndex - 1 + searchResults.length) % searchResults.length;
      }
    } else if (e.key === 'Escape') {
      searchOpen = false;
      searchInputEl?.blur();
    } else if (e.key === 'Enter') {
      if (searchResults.length > 0 && searchActiveIndex >= 0 && searchActiveIndex < searchResults.length) {
        e.preventDefault();
        selectSearchResult(searchResults[searchActiveIndex]);
      }
    }
  }

  function selectSearchResult(item: SearchResultView) {
    searchOpen = false;
    searchQuery = '';
    closeMenus();
    if (item.url) {
      goto(item.url);
    }
  }

  function toggleMobileSearch() {
    searchOpen = !searchOpen;
    if (searchOpen) {
      setTimeout(() => {
        searchInputEl?.focus();
      }, 50);
    }
  }

  function closeMenus() {
    userMenuOpen = false;
    mobileOpen = false;
    notifOpen = false;
    moreOpen = false;
    searchOpen = false;
    sheetTranslateY = 0;
  }

  // 原型有而前端缺：菜单外点关闭 + Escape 关闭（统一作用于用户菜单 /
  // 通知下拉 / 更多下拉 / 搜索弹层 / 移动端抽屉）。data-nav-menu 标记菜单锚定区域，
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

<header class="navbar" class:mobile-search-open={searchOpen}>
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

    <div class="nav-center" data-nav-menu>
      <form class="search-form" role="search" onsubmit={submitSearch}>
        <span class="search-icon" aria-hidden="true">
          <Icon name="search" size={16} />
        </span>
        <input
          bind:this={searchInputEl}
          type="search"
          name="q"
          id="header-search-input"
          placeholder="搜索帖子、用户、标签…"
          class="search-input"
          aria-label="搜索帖子、用户和标签"
          aria-controls={searchOpen && searchQuery.trim() ? 'header-search-popover' : undefined}
          autocomplete="off"
          value={searchQuery}
          oninput={handleSearchInput}
          onfocus={() => {
            if (searchQuery.trim()) searchOpen = true;
          }}
          onkeydown={handleSearchKeyDown}
        />
        {#if searchLoading}
          <span class="search-spinner" aria-label="正在搜索">
            <aui-spinner></aui-spinner>
          </span>
        {/if}
      </form>

      {#if searchOpen && searchQuery.trim()}
        <div class="search-popover dropdown" id="header-search-popover" role="listbox" aria-label="搜索建议">
          {#if searchLoading && searchResults.length === 0}
            <div class="search-feedback">
              <aui-spinner></aui-spinner>
              <span>正在搜索…</span>
            </div>
          {:else if searchResults.length === 0}
            <div class="search-feedback">
              <span>未找到关于 “{searchQuery}” 的内容</span>
            </div>
          {:else}
            <div class="search-results-list">
              {#each searchResults as item, index (item.id + item.type)}
                <a
                  href={item.url}
                  class="search-result-row {index === searchActiveIndex ? 'is-active' : ''}"
                  role="option"
                  aria-selected={index === searchActiveIndex}
                  onclick={(e) => {
                    e.preventDefault();
                    selectSearchResult(item);
                  }}
                  onmouseenter={() => (searchActiveIndex = index)}
                >
                  <span class="search-item-badge">
                    <aui-status-tag status={item.type === 'post' ? 'info' : item.type === 'user' ? 'success' : item.type === 'board' ? 'warning' : 'default'}>
                      {item.type === 'post' ? '帖子' : item.type === 'user' ? '用户' : item.type === 'board' ? '板块' : '标签'}
                    </aui-status-tag>
                  </span>
                  <div class="search-item-info">
                    <div class="search-item-title">{item.title || '（无标题）'}</div>
                    {#if item.highlight || item.excerpt}
                      <div class="search-item-snippet">{item.highlight || item.excerpt}</div>
                    {/if}
                  </div>
                  {#if item.author_name}
                    <span class="search-item-author">{item.author_name}</span>
                  {/if}
                </a>
              {/each}
            </div>
          {/if}
        </div>
      {/if}
    </div>

    <div class="nav-right" data-nav-menu>
      <!-- 全站唯一搜索入口：有 JS 打开页内浮层（聚焦输入框）；
           无 JS 回退为普通链接跳 /search（渐进增强，无 JS 基线可达）。 -->
      <a
        href="/search"
        class="nav-icon-btn mobile-search-btn"
        aria-label="搜索"
        title="搜索"
        onclick={(event) => {
          event.preventDefault();
          toggleMobileSearch();
        }}
      >
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
            <CosmeticAvatar name={user.display_name || user.username} size="md" presentation={user.presentation_tokens} avatarAttachmentId={user.avatar_attachment_id} seed={user.username ?? user.id} />
          </button>
          {#if userMenuOpen}
            <div
              class="user-sheet-backdrop"
              role="button"
              tabindex="-1"
              aria-label="关闭用户菜单"
              onclick={closeMenus}
              onkeydown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault();
                  closeMenus();
                }
              }}
            ></div>
            <div
              class="dropdown user-menu"
              role="menu"
              aria-label="用户菜单"
              style={sheetTranslateY > 0 ? `transform: translateY(${sheetTranslateY}px); transition: none;` : undefined}
            >
              <div
                class="user-menu-handle-bar"
                aria-hidden="true"
                ontouchstart={handleTouchStart}
                ontouchmove={handleTouchMove}
                ontouchend={handleTouchEnd}
              >
                <div class="user-menu-handle"></div>
              </div>
              <div class="user-menu-header">
                <CosmeticAvatar name={user.display_name || user.username} size="lg" presentation={user.presentation_tokens} avatarAttachmentId={user.avatar_attachment_id} seed={user.username ?? user.id} />
                <div class="user-menu-info">
                  <div class="user-menu-name">
                    <CosmeticName name={user.display_name || user.username} presentation={user.presentation_tokens} />
                  </div>
                  <div class="user-menu-level">LV.{user.level ?? 1}</div>
                </div>
                <button type="button" class="user-menu-close" aria-label="关闭" onclick={closeMenus}>
                  <Icon name="x" size={20} />
                </button>
              </div>
              <div class="dropdown-sep"></div>
              <div class="user-menu-items">
                <a href="/me" class="dropdown-item" role="menuitem" onclick={closeMenus}><Icon name="user" size={16} /><span>我的主页</span></a>
                <a href="/me/level" class="dropdown-item" role="menuitem" onclick={closeMenus}><Icon name="award" size={16} /><span>我的等级</span></a>
                <a href="/me/balance" class="dropdown-item" role="menuitem" onclick={closeMenus}><Icon name="coins" size={16} /><span>积分明细</span></a>
                <a href="/me/wardrobe" class="dropdown-item" role="menuitem" onclick={closeMenus}><Icon name="sparkles" size={16} /><span>我的装扮</span></a>
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
          <a
            href={item.href}
            class="drawer-nav-item {isActive(item.href) ? 'is-active' : ''}"
            onclick={closeMenus}
          >
            <Icon name={item.icon} size={17} />
            <span>{item.label}</span>
          </a>
        {/each}
        <div class="drawer-divider"></div>
        {#each visibleMoreItems as item (item.href)}
          <a
            href={item.href}
            class="drawer-nav-item {isActive(item.href) ? 'is-active' : ''}"
            onclick={closeMenus}
          >
            <Icon name={item.icon} size={17} />
            <span>{item.label}</span>
          </a>
        {/each}
        {#if user}
          <!-- 个人链接区已收敛到右侧头像用户菜单（Bottom Sheet），抽屉仅保留退出登录 -->
          <button type="button" class="drawer-nav-item is-danger" onclick={() => { closeMenus(); onlogout?.(); }}>退出登录</button>
        {:else}
          <a href="/login" class="drawer-nav-item" onclick={closeMenus}>登录</a>
          <a href="/register" class="drawer-nav-item" onclick={closeMenus}>注册</a>
        {/if}
      </nav>
    </div>
  </div>
{/if}

<style>
  .nav-center {
    position: relative;
  }

  .search-spinner {
    position: absolute;
    right: 12px;
    top: 50%;
    transform: translateY(-50%);
    display: inline-flex;
    align-items: center;
    color: var(--aui-text-muted);
  }

  .search-popover {
    position: absolute;
    top: calc(100% + 8px);
    left: 0;
    width: 100%;
    min-width: 360px;
    max-width: 520px;
    background: var(--aui-surface);
    border: 1px solid var(--aui-border);
    box-shadow: var(--aui-shadow-pop);
    border-radius: var(--aui-radius-md, 0px);
    overflow: hidden;
    z-index: 100;
  }

  .search-feedback {
    padding: 16px;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    color: var(--aui-text-secondary);
    font: 12px/1.4 var(--font-family-mono);
  }

  .search-results-list {
    max-height: 380px;
    overflow-y: auto;
    padding: 4px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .search-result-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    color: var(--aui-text-secondary);
    text-decoration: none;
    border-radius: var(--aui-radius-sm, 0px);
    transition: background 0.1s ease, color 0.1s ease;
  }

  .search-result-row:hover,
  .search-result-row.is-active {
    background: var(--aui-header);
    color: var(--aui-text-primary);
  }

  .search-item-badge {
    flex-shrink: 0;
  }

  .search-item-info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .search-item-title {
    font-size: 13px;
    font-weight: 500;
    color: var(--aui-text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .search-item-snippet {
    font-size: 11px;
    color: var(--aui-text-muted);
    font-family: var(--font-family-mono);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .search-item-author {
    flex-shrink: 0;
    font-size: 11px;
    color: var(--aui-text-muted);
    font-family: var(--font-family-mono);
  }

  .user-sheet-backdrop {
    display: none;
  }

  .user-menu-handle-bar {
    display: none;
  }

  .user-menu-close {
    display: none;
  }

  .user-menu-info {
    min-width: 0;
  }

  .user-menu-items {
    display: flex;
    flex-direction: column;
  }

  @keyframes user-sheet-fade-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  @keyframes user-sheet-slide-up {
    from {
      transform: translateY(100%);
      opacity: 0.6;
    }
    to {
      transform: translateY(0);
      opacity: 1;
    }
  }

  @media (max-width: 768px) {
    .search-popover {
      position: fixed;
      top: 56px;
      left: 8px;
      right: 8px;
      width: calc(100vw - 16px);
      min-width: unset;
      max-width: unset;
    }
  }
</style>
