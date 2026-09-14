<script lang="ts">
  // 管理后台独立布局系统（独立于前台 Header / 底部导航）
  // 提供专业沉浸式控制台：左侧全高侧栏 + 顶部轻量面包屑/操作栏 + 自适应主内容区
  import { onDestroy, onMount, tick } from 'svelte';
  import type { Snippet } from 'svelte';
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { logout } from '$lib/api/client';
  import type { LayoutData } from './$types';
  import Icon from '$lib/components/ui/Icon.svelte';
  import Avatar from '$lib/components/ui/Avatar.svelte';
  import {
    readPreference,
    applyTheme,
    switchToNextTheme,
    type ThemePreference
  } from '$lib/theme';
  import { show as showToast } from '$lib/ui/toast';

  let { data, children }: { data?: LayoutData; children?: Snippet } = $props();

  const path = $derived(page.url.pathname);
  const currentUser = $derived(data?.user ?? null);
  const roles = $derived(currentUser?.roles);
  const unreadCount = $derived(data?.notifications?.unreadCount ?? 0);

  // 主题偏好响应式状态
  let themePref = $state<ThemePreference>('system');
  let resolvedDark = $state(false);

  onMount(() => {
    themePref = readPreference();
    resolvedDark = document.documentElement.classList.contains('dark') ||
      (themePref === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches);

    const mq = typeof window.matchMedia === 'function'
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

  function toggleTheme(): void {
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

  const userRoleLabel = $derived.by(() => {
    if (Array.isArray(roles)) {
      if (roles.includes('administrator') || roles.includes('admin')) return '超级管理员';
      if (roles.includes('global_moderator') || roles.includes('moderator')) return '版主/协管';
    }
    return '管理员';
  });

  type AdminNavItem = {
    label: string;
    href: string;
    icon: string;
    exact?: boolean;
    adminOnly?: boolean;
    badge?: number;
  };
  type AdminNavGroup = {
    label: string;
    items: AdminNavItem[];
    adminOnly?: boolean;
  };

  // 顶层常驻项：仪表盘
  const primaryItem: AdminNavItem = {
    label: '仪表盘',
    href: '/admin',
    icon: 'layout-dashboard',
    exact: true
  };

  const navGroups: AdminNavGroup[] = [
    {
      label: '内容',
      items: [
        { label: '帖子与文章', href: '/admin/posts', icon: 'file-text' },
        { label: '内容审核', href: '/admin/content', icon: 'clipboard-check' },
        { label: '板块管理', href: '/admin/boards', icon: 'workflow' },
        { label: '标签管理', href: '/admin/tags', icon: 'tag' },
        { label: '附件管理', href: '/admin/attachments', icon: 'paperclip' }
      ]
    },
    {
      label: '用户与权限',
      items: [
        { label: '用户管理', href: '/admin/users', icon: 'users', adminOnly: true },
        { label: '角色委派', href: '/admin/assignments', icon: 'user-check', adminOnly: true },
        { label: '角色与权限', href: '/admin/roles', icon: 'shield', adminOnly: true },
        { label: '举报与审核', href: '/admin/moderation/cases', icon: 'flag' },
        { label: '风控策略', href: '/admin/moderation/risk', icon: 'shield-alert' }
      ]
    },
    {
      label: '激励与交易',
      adminOnly: true,
      items: [
        { label: '成就管理', href: '/admin/achievements', icon: 'trophy' },
        { label: '积分与货币', href: '/admin/points', icon: 'coins', exact: true },
        { label: '积分规则', href: '/admin/points/rules', icon: 'list' },
        { label: '等级管理', href: '/admin/levels', icon: 'shield-check' },
        { label: '商城管理', href: '/admin/shop', icon: 'shopping-bag' },
        { label: '签到与活跃', href: '/admin/activity', icon: 'activity' },
        { label: '市场与交易', href: '/admin/marketplace', icon: 'shopping-cart' },
        { label: '下载计费', href: '/admin/download-billing', icon: 'download' }
      ]
    },
    {
      label: '扩展与集成',
      adminOnly: true,
      items: [
        { label: '主题管理', href: '/admin/themes', icon: 'palette' },
        { label: '插件管理', href: '/admin/plugins', icon: 'puzzle' },
        { label: 'OAuth 客户端', href: '/admin/oauth', icon: 'shield' },
        { label: '大模型设置', href: '/admin/ai', icon: 'sparkles' },
        { label: '视频插件', href: '/admin/video', icon: 'video' }
      ]
    },
    {
      label: '系统',
      adminOnly: true,
      items: [
        { label: '文件存储', href: '/admin/storage', icon: 'server' },
        { label: '通知与邮件', href: '/admin/notifications', icon: 'bell' },
        { label: '审计日志', href: '/admin/audit', icon: 'file-text' },
        { label: '系统设置', href: '/admin/settings', icon: 'settings' },
        { label: '功能开关', href: '/admin/feature-flags', icon: 'toggle-right' },
        { label: 'BI 看板', href: '/admin/bi', icon: 'bar-chart' }
      ]
    }
  ];

  const visibleNavGroups = $derived.by(() => {
    const isKnownNonAdmin = Array.isArray(roles) && !roles.some((role) => role === 'administrator' || role === 'admin');
    if (!isKnownNonAdmin) return navGroups;

    return navGroups
      .filter((group) => !group.adminOnly)
      .map((group) => ({
        ...group,
        items: group.items.filter((item) => !item.adminOnly)
      }))
      .filter((group) => group.items.length > 0);
  });

  function isItemActive(item: { href: string; exact?: boolean }): boolean {
    if (item.exact) return path === item.href;
    return path === item.href || path.startsWith(item.href + '/');
  }

  const activeItem = $derived(
    [primaryItem, ...visibleNavGroups.flatMap((group) => group.items)].find((item) => isItemActive(item))
  );
  const activeLabel = $derived(activeItem?.label ?? '管理后台');
  const activeGroupLabel = $derived(
    visibleNavGroups.find((group) => group.items.some((item) => isItemActive(item)))?.label ?? '概览'
  );

  const activeGroupIndex = $derived.by(() => {
    const idx = visibleNavGroups.findIndex((group) => group.items.some((item) => isItemActive(item)));
    return idx >= 0 ? idx : null;
  });

  let openGroupIndexes = $state<number[]>([]);

  $effect(() => {
    openGroupIndexes = activeGroupIndex === null ? visibleNavGroups.map((_, index) => index) : [activeGroupIndex];
  });

  function isGroupExpanded(index: number): boolean {
    return openGroupIndexes.includes(index);
  }

  function toggleGroup(index: number): void {
    openGroupIndexes = isGroupExpanded(index)
      ? openGroupIndexes.filter((item) => item !== index)
      : [...openGroupIndexes, index];
  }

  let mobileMenuOpen = $state(false);
  let mobileMenuButton: HTMLButtonElement | undefined = $state();
  let mobileFirstLink: HTMLAnchorElement | undefined = $state();

  function closeMobileMenu(restoreFocus = false): void {
    mobileMenuOpen = false;
    if (restoreFocus) mobileMenuButton?.focus();
  }

  function toggleMobileMenu(): void {
    if (mobileMenuOpen) closeMobileMenu(true);
    else mobileMenuOpen = true;
  }

  function handleAdminKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape' && mobileMenuOpen) {
      event.preventDefault();
      closeMobileMenu(true);
    }
  }

  $effect(() => {
    if (!mobileMenuOpen) return;
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    void tick().then(() => mobileFirstLink?.focus());
    return () => {
      document.body.style.overflow = previousOverflow;
    };
  });

  $effect(() => {
    void path;
    mobileMenuOpen = false;
  });

  onDestroy(() => {
    if (typeof document !== 'undefined') document.body.style.overflow = '';
  });

  async function handleLogout(): Promise<void> {
    try {
      await logout(fetch);
    } finally {
      goto('/login');
    }
  }
</script>

<svelte:head>
  <title>{activeLabel} — 管理后台 · BBLBB</title>
</svelte:head>

<div
  class="app-admin-shell app-admin-page-shell"
  class:admin-menu-open={mobileMenuOpen}
  id="page-admin"
>
  <!-- 移动端侧边抽屉遮罩 -->
  <button
    type="button"
    class="app-admin-menu-backdrop"
    aria-label="关闭管理菜单"
    tabindex="-1"
    onclick={() => closeMobileMenu()}
  ></button>

  <!-- 左侧控制台侧栏导航 -->
  <aside
    class="app-admin-side"
    id="admin-navigation"
    aria-label="管理后台导航"
  >
    <!-- 侧栏顶部品牌与状态 -->
    <div class="app-admin-side__brand app-admin-side__brand-header">
      <a href="/admin" class="app-admin-side__brand-link" onclick={() => closeMobileMenu()}>
        <div class="app-admin-brand-icon">
          <Icon name="layout-dashboard" size={16} />
        </div>
        <div class="app-admin-brand-text">
          <span class="app-admin-brand-title">BBLBB Admin</span>
          <span class="app-admin-brand-tag">CONTROL ROOM</span>
        </div>
      </a>
      <div class="app-admin-brand-actions">
        <span class="app-admin-status-pill desktop-only" title="管理控制台在线">
          <span class="app-admin-pulse-dot"></span>
          <span class="app-admin-pulse-text">LIVE</span>
        </span>
        <!-- 移动端抽屉专属关闭按钮 -->
        <button
          type="button"
          class="app-admin-drawer-close"
          aria-label="关闭菜单"
          onclick={() => closeMobileMenu(true)}
        >
          <Icon name="x" size={15} />
        </button>
      </div>
    </div>

    <!-- 侧栏导航内容区域 -->
    <div class="app-admin-side__scroll">
      <!-- 顶层常驻项：仪表盘（首位独立项） -->
      <div class="app-admin-side__primary">
        <a
          href={primaryItem.href}
          bind:this={mobileFirstLink}
          class="app-admin-nav-item is-dashboard"
          class:is-active={isItemActive(primaryItem)}
          aria-current={isItemActive(primaryItem) ? 'page' : undefined}
          onclick={() => closeMobileMenu()}
          onkeydown={handleAdminKeydown}
        >
          <span class="app-admin-nav-icon">
            <Icon name={primaryItem.icon} size={15} />
          </span>
          <span class="app-admin-nav-text">{primaryItem.label}</span>
          {#if isItemActive(primaryItem)}
            <span class="app-admin-active-indicator" aria-hidden="true"></span>
          {/if}
        </a>
      </div>

      <!-- 分组导航树 -->
      <div class="app-admin-side__groups">
        {#each visibleNavGroups as group, index}
          {@const isExpanded = isGroupExpanded(index)}
          <div class="app-admin-side__group" class:is-expanded={isExpanded}>
            <button
              type="button"
              class="app-admin-side__label"
              data-admin-group-toggle=""
              aria-controls={'admin-group-items-' + index}
              aria-expanded={isExpanded}
              onclick={() => toggleGroup(index)}
              onkeydown={handleAdminKeydown}
            >
              <span class="app-admin-side__label-text">{group.label}</span>
              <Icon name="chevron-down" size={12} class="app-admin-chevron" />
            </button>
            <div
              class="app-admin-side__items"
              id={'admin-group-items-' + index}
              aria-hidden={isExpanded ? undefined : 'true'}
              inert={isExpanded ? undefined : true}
            >
              {#each group.items as item}
                <a
                  href={item.href}
                  class="app-admin-nav-item"
                  class:is-active={isItemActive(item)}
                  aria-current={isItemActive(item) ? 'page' : undefined}
                  onclick={() => closeMobileMenu()}
                  onkeydown={handleAdminKeydown}
                >
                  <span class="app-admin-nav-icon">
                    <Icon name={item.icon} size={14} />
                  </span>
                  <span class="app-admin-nav-text">{item.label}</span>
                  {#if isItemActive(item)}
                    <span class="app-admin-active-indicator" aria-hidden="true"></span>
                  {/if}
                </a>
              {/each}
            </div>
          </div>
        {/each}
      </div>
    </div>

    <!-- 侧栏底部操作与审计提示 -->
    <div class="app-admin-side__foot">
      <a
        class="app-admin-side__exit"
        href="/"
        onclick={() => closeMobileMenu()}
        onkeydown={handleAdminKeydown}
      >
        <Icon name="house" size={14} />
        <span>返回社区前台</span>
      </a>
      <div class="app-admin-side__foot-meta">
        <span class="app-admin-side__foot-badge">控制台安全在线</span>
        <span class="app-admin-side__foot-audit">操作均计入审计日志</span>
      </div>
    </div>
  </aside>

  <!-- 右侧主区域：顶栏 + 页面内容 -->
  <div class="app-admin-main-wrap">
    <!-- 后台专属 Header / Topbar -->
    <header class="app-admin-topbar">
      <div class="app-admin-topbar__left">
        <!-- 移动端管理菜单抽屉按钮 -->
        <button
          type="button"
          class="app-admin-menu"
          bind:this={mobileMenuButton}
          aria-label="管理菜单"
          aria-controls="admin-navigation"
          aria-expanded={mobileMenuOpen}
          aria-describedby="admin-menu-context"
          onclick={toggleMobileMenu}
          onkeydown={handleAdminKeydown}
        >
          <Icon name={mobileMenuOpen ? 'x' : 'menu'} size={16} />
          <span class="app-admin-menu__text">管理菜单</span>
          <span class="app-admin-menu__current">{activeLabel}</span>
        </button>
        <span id="admin-menu-context" class="u-visually-hidden">
          当前页面：{activeGroupLabel} / {activeLabel}。按 Escape 关闭菜单。
        </span>

        <!-- 面包屑导航 -->
        <nav class="app-admin-breadcrumb" aria-label="后台层级导航">
          <a href="/admin" class="app-admin-breadcrumb__item app-admin-breadcrumb__root">
            <Icon name="layout-dashboard" size={13} />
            <span>管理后台</span>
          </a>
          {#if activeGroupLabel && activeGroupLabel !== '概览'}
            <span class="app-admin-breadcrumb__sep is-group" aria-hidden="true">/</span>
            <span class="app-admin-breadcrumb__item is-group">{activeGroupLabel}</span>
          {/if}
          {#if activeLabel && activeLabel !== '管理后台'}
            <span class="app-admin-breadcrumb__sep" aria-hidden="true">/</span>
            <span class="app-admin-breadcrumb__item is-current" aria-current="page">{activeLabel}</span>
          {/if}
        </nav>
      </div>

      <div class="app-admin-topbar__right">
        <!-- 快捷暗色/亮色模式切换 -->
        <button
          type="button"
          class="app-admin-icon-btn"
          title={themePref === 'system' ? '主题：跟随系统' : resolvedDark ? '主题：暗色模式' : '主题：亮色模式'}
          aria-label="切换主题偏好"
          onclick={toggleTheme}
        >
          <Icon name={resolvedDark ? 'sun' : 'moon'} size={15} />
        </button>

        <!-- 通知中心快捷入口 -->
        <a
          href="/admin/notifications"
          class="app-admin-icon-btn app-admin-notif-btn"
          title={unreadCount > 0 ? `有 ${unreadCount} 条未读通知` : '通知与邮件'}
          aria-label="通知中心"
        >
          <Icon name="bell" size={15} />
          {#if unreadCount > 0}
            <span class="app-admin-notif-dot" aria-hidden="true"></span>
          {/if}
        </a>

        <!-- 快捷跳转前台 -->
        <a href="/" class="app-admin-topbar__btn" title="查看前台社区">
          <Icon name="house" size={13} />
          <span class="app-admin-topbar__btn-label">返回前台</span>
        </a>

        <div class="app-admin-topbar__divider" aria-hidden="true"></div>

        <!-- 当前管理员状态与操作 -->
        <div class="app-admin-user">
          <div class="app-admin-user__avatar-wrap">
            <Avatar
              name={currentUser?.display_name || currentUser?.username || '管理员'}
              attachmentId={currentUser?.avatar_attachment_id}
              size={28}
              seed={currentUser?.username ?? currentUser?.id}
            />
          </div>
          <div class="app-admin-user__meta">
            <span class="app-admin-user__name">{currentUser?.display_name || currentUser?.username || '管理员'}</span>
            <span class="app-admin-user__role-badge">{userRoleLabel}</span>
          </div>
          <button
            type="button"
            class="app-admin-user__logout"
            title="退出登录"
            aria-label="退出登录"
            onclick={handleLogout}
          >
            <Icon name="log-out" size={14} />
          </button>
        </div>
      </div>
    </header>

    <!-- 主页面内容承载容器 -->
    <div
      class="app-admin-main"
      inert={mobileMenuOpen ? true : undefined}
      aria-hidden={mobileMenuOpen ? 'true' : undefined}
    >
      <div class="app-admin-main__inner">
        {@render children?.()}
      </div>
    </div>
  </div>
</div>