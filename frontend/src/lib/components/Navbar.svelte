<script lang="ts">
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import Icon from './ui/Icon.svelte';
  import Avatar from './ui/Avatar.svelte';
  import Button from './ui/Button.svelte';

  let {
    user,
    unread = 0,
    onlogout
  }: {
    user: { username: string; display_name?: string | null; level?: number; roles?: string[] } | null;
    unread?: number;
    onlogout?: () => void;
  } = $props();

  let userMenuOpen = $state(false);
  let mobileOpen = $state(false);

  const navItems = [
    { label: '首页', href: '/' },
    { label: '板块', href: '/boards' },
    { label: '搜索', href: '/search' }
  ];

  const path = $derived(page.url.pathname);

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
  }
</script>

<header class="navbar">
  <div class="container nav-container">
    <div class="nav-left">
      <button
        type="button"
        class="mobile-menu-btn"
        aria-label="菜单"
        onclick={() => (mobileOpen = !mobileOpen)}
      >
        <Icon name="menu" size={20} />
      </button>
      <a href="/" class="nav-logo" onclick={closeMenus}>BBLBB</a>
      <nav class="nav-items" aria-label="主导航">
        {#each navItems as item}
          <a
            href={item.href}
            class="nav-link {isActive(item.href) ? 'is-active' : ''}"
            aria-current={isActive(item.href) ? 'page' : undefined}
          >{item.label}</a>
        {/each}
      </nav>
    </div>

    <div class="nav-center">
      <form class="search-form" role="search" onsubmit={submitSearch}>
        <button type="submit" class="search-submit" aria-label="提交搜索">
          <Icon name="search" size={17} />
        </button>
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
      <Button text="发布" variant="primary" size="sm" icon="pen-line" href="/editor" />
      <a href="/notifications" class="nav-icon-btn" aria-label="通知">
        <Icon name="bell" size={18} />
        {#if unread > 0}
          <span class="nav-notif-dot">{unread > 99 ? '99+' : unread}</span>
        {/if}
      </a>
      <div class="user-menu-wrapper">
        {#if user}
          <button
            type="button"
            class="user-avatar-btn"
            aria-label="用户菜单"
            aria-expanded={userMenuOpen}
            onclick={(e) => {
              e.stopPropagation();
              userMenuOpen = !userMenuOpen;
            }}
          >
            <Avatar name={user.display_name || user.username} size="md" />
          </button>
          {#if userMenuOpen}
            <div class="dropdown user-menu">
              <div class="user-menu-header">
                <Avatar name={user.display_name || user.username} size="lg" />
                <div>
                  <div class="user-menu-name">{user.display_name || user.username}</div>
                  <div class="user-menu-level">LV.{user.level ?? 1}</div>
                </div>
              </div>
              <div class="dropdown-sep"></div>
              <a href="/me" class="dropdown-item" onclick={closeMenus}><Icon name="user" size={16} /><span>我的主页</span></a>
              <a href="/settings" class="dropdown-item" onclick={closeMenus}><Icon name="settings" size={16} /><span>账号设置</span></a>
              <div class="dropdown-sep"></div>
              <button type="button" class="dropdown-item is-danger" onclick={() => { closeMenus(); onlogout?.(); }}>
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
        {#each navItems as item}
          <a href={item.href} class="drawer-nav-item" onclick={closeMenus}>{item.label}</a>
        {/each}
        <div class="drawer-divider"></div>
        {#if user}
          <a href="/me" class="drawer-nav-item" onclick={closeMenus}>我的主页</a>
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

{#if userMenuOpen || mobileOpen}
  <button
    class="menu-scrim u-hidden"
    aria-label="关闭菜单"
    tabindex="-1"
    onclick={closeMenus}
  ></button>
{/if}
