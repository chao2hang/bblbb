<script lang="ts">
  import Icon from '$lib/components/ui/Icon.svelte';

  export type SettingsNavTab =
    | 'profile'
    | 'appearance'
    | 'security'
    | 'devices'
    | 'notifications'
    | 'oauth'
    | 'privacy';

  interface Props {
    active?: SettingsNavTab | string;
    profileLabel?: string;
    onSelectTab?: (tab: SettingsNavTab) => void;
  }

  let { active = 'devices', profileLabel = '个人资料', onSelectTab }: Props = $props();

  const isTabMode = $derived(typeof onSelectTab === 'function');

  interface NavItem {
    id: SettingsNavTab;
    label: string;
    icon: string;
    href: string;
    isExternalRoute?: boolean;
  }

  const items = $derived<NavItem[]>([
    { id: 'profile', label: profileLabel, icon: 'user', href: '/settings' },
    { id: 'appearance', label: '外观与主题', icon: 'palette', href: '/settings#settings-appearance' },
    { id: 'security', label: '账号安全', icon: 'shield', href: '/settings#settings-security' },
    { id: 'devices', label: '登录设备', icon: 'monitor', href: '/me/security', isExternalRoute: true },
    { id: 'notifications', label: '通知设置', icon: 'bell', href: '/settings#settings-notifications' },
    { id: 'oauth', label: 'OAuth 授权', icon: 'key', href: '/settings#settings-oauth' },
    { id: 'privacy', label: '隐私设置', icon: 'eye-off', href: '/settings/privacy', isExternalRoute: true }
  ]);

  const currentItem = $derived(items.find((i) => i.id === active) ?? items[0]);
  let isMobileDropdownOpen = $state(false);

  function toggleMobileDropdown() {
    isMobileDropdownOpen = !isMobileDropdownOpen;
  }

  function handleSelect(item: NavItem) {
    isMobileDropdownOpen = false;
    if (!item.isExternalRoute && isTabMode) {
      onSelectTab?.(item.id);
    }
  }
</script>

<!-- 移动端 Flarum / NoPJ 式顶栏标题下拉选择器 -->
<div class="settings-mobile-dropdown-bar" class:hide-on-mobile={true}>
  <a href="/me" class="settings-back-btn" aria-label="返回个人中心">
    <Icon name="chevron-left" size={18} />
  </a>

  <div class="settings-title-control">
    <button
      type="button"
      class="settings-title-btn"
      onclick={toggleMobileDropdown}
      aria-expanded={isMobileDropdownOpen}
      aria-haspopup="true"
    >
      <Icon name={currentItem.icon} size={15} />
      <span class="settings-title-text">{currentItem.label}</span>
      <span class="settings-title-caret">
        <Icon name="chevron-down" size={14} />
      </span>
    </button>

    {#if isMobileDropdownOpen}
      <button
        type="button"
        class="settings-dropdown-backdrop"
        onclick={() => (isMobileDropdownOpen = false)}
        aria-label="关闭选择菜单"
      ></button>

      <ul class="settings-dropdown-menu" role="menu">
        {#each items as item (item.id)}
          <li role="none">
            {#if item.isExternalRoute}
              <a
                href={item.href}
                class="settings-dropdown-item"
                class:is-active={active === item.id}
                role="menuitem"
                onclick={() => (isMobileDropdownOpen = false)}
              >
                <Icon name={item.icon} size={16} />
                <span>{item.label}</span>
                {#if active === item.id}
                  <span class="settings-dropdown-check">✓</span>
                {/if}
              </a>
            {:else if isTabMode}
              <button
                type="button"
                class="settings-dropdown-item"
                class:is-active={active === item.id}
                role="menuitem"
                onclick={() => handleSelect(item)}
              >
                <Icon name={item.icon} size={16} />
                <span>{item.label}</span>
                {#if active === item.id}
                  <span class="settings-dropdown-check">✓</span>
                {/if}
              </button>
            {:else}
              <a
                href={item.href}
                class="settings-dropdown-item"
                class:is-active={active === item.id}
                role="menuitem"
                onclick={() => (isMobileDropdownOpen = false)}
              >
                <Icon name={item.icon} size={16} />
                <span>{item.label}</span>
                {#if active === item.id}
                  <span class="settings-dropdown-check">✓</span>
                {/if}
              </a>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  <span class="settings-bar-spacer"></span>
</div>

<!-- 桌面端经典侧栏导航（在 mobile 上隐藏） -->
{#if isTabMode}
  <div class="app-settings-nav" role="tablist" aria-label="设置导航">
    {#each items as item (item.id)}
      {#if item.isExternalRoute}
        <a href={item.href} class:is-active={active === item.id}>
          <span class="app-settings-nav__icon" aria-hidden="true">
            <Icon name={item.icon} size={14} />
          </span>
          <span class="app-settings-nav__label">{item.label}</span>
        </a>
      {:else}
        <button
          type="button"
          role="tab"
          aria-selected={active === item.id}
          aria-controls={`settings-panel-${item.id}`}
          class:is-active={active === item.id}
          onclick={(e) => {
            onSelectTab?.(item.id);
            if (typeof (e.currentTarget as HTMLElement)?.scrollIntoView === 'function') {
              (e.currentTarget as HTMLElement).scrollIntoView({ behavior: 'smooth', inline: 'center', block: 'nearest' });
            }
          }}
        >
          <span class="app-settings-nav__icon" aria-hidden="true">
            <Icon name={item.icon} size={14} />
          </span>
          <span class="app-settings-nav__label">{item.label}</span>
        </button>
      {/if}
    {/each}
  </div>
{:else}
  <nav class="app-settings-nav" aria-label="设置导航">
    {#each items as item (item.id)}
      <a href={item.href} class:is-active={active === item.id}>
        <span class="app-settings-nav__icon" aria-hidden="true">
          <Icon name={item.icon} size={14} />
        </span>
        <span class="app-settings-nav__label">{item.label}</span>
      </a>
    {/each}
  </nav>
{/if}

<style>
  /* 移动端顶栏选择器（桌面隐藏） */
  .settings-mobile-dropdown-bar {
    display: none;
  }

  .app-settings-nav__icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    flex-shrink: 0;
    color: currentColor;
    opacity: 0.85;
    transition: opacity var(--duration-fast, 150ms) var(--ease-out, ease-out);
  }

  .app-settings-nav__label {
    white-space: nowrap;
  }

  :global(.app-settings-nav button:hover .app-settings-nav__icon),
  :global(.app-settings-nav button.is-active .app-settings-nav__icon),
  :global(.app-settings-nav a:hover .app-settings-nav__icon),
  :global(.app-settings-nav a.is-active .app-settings-nav__icon) {
    opacity: 1;
  }

  @media (max-width: 767px) {
    /* 移动端隐藏容易杂乱的横滑 Tags 轨 */
    .app-settings-nav {
      display: none !important;
    }

    /* 移动端使用全局统一顶栏接管导航 */
    .settings-mobile-dropdown-bar {
      display: none !important;
    }

    .settings-back-btn {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      width: 32px;
      height: 32px;
      color: var(--color-text-secondary);
      text-decoration: none;
      border-radius: var(--radius-sm);
    }
    .settings-back-btn:hover {
      color: var(--color-text-primary);
      background: var(--color-bg-subtle);
    }

    .settings-title-control {
      position: relative;
      flex: 1;
      display: flex;
      justify-content: center;
    }

    .settings-title-btn {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      height: 34px;
      padding: 0 10px;
      border: 1px solid transparent;
      border-radius: var(--radius-sm);
      background: transparent;
      color: var(--color-text-primary);
      font-size: 15px;
      font-weight: var(--weight-semibold, 600);
      cursor: pointer;
    }
    .settings-title-btn:hover {
      background: var(--color-bg-subtle);
    }

    .settings-title-caret {
      color: var(--color-text-tertiary);
      transition: transform 0.2s ease;
    }
    .settings-title-btn[aria-expanded="true"] .settings-title-caret {
      transform: rotate(180deg);
    }

    .settings-bar-spacer {
      width: 32px;
    }

    .settings-dropdown-backdrop {
      position: fixed;
      inset: 0;
      z-index: 99;
      background: transparent;
      border: none;
      padding: 0;
      cursor: default;
    }

    .settings-dropdown-menu {
      position: absolute;
      top: calc(100% + 6px);
      left: 50%;
      transform: translateX(-50%);
      width: 220px;
      padding: 4px;
      margin: 0;
      list-style: none;
      background: var(--color-bg-card);
      border: 1px solid var(--color-border);
      border-radius: var(--radius-md);
      box-shadow: var(--shadow-modal);
      z-index: 100;
    }

    .settings-dropdown-item {
      display: flex;
      align-items: center;
      gap: 10px;
      width: 100%;
      padding: 10px 12px;
      font-size: 13px;
      color: var(--color-text-primary);
      text-decoration: none;
      background: transparent;
      border: none;
      border-radius: var(--radius-sm);
      text-align: left;
      cursor: pointer;
    }
    .settings-dropdown-item:hover {
      background: var(--color-bg-subtle);
    }
    .settings-dropdown-item.is-active {
      color: var(--color-brand);
      font-weight: var(--weight-semibold, 600);
      background: var(--color-brand-soft);
    }

    .settings-dropdown-check {
      margin-left: auto;
      color: var(--color-brand);
      font-weight: 700;
    }
  }
</style>
