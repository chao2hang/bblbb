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
</script>

{#if isTabMode}
  <div class="app-settings-nav" role="tablist" aria-label="设置导航">
    {#each items as item (item.id)}
      {#if item.isExternalRoute}
        <a href={item.href} class:is-active={active === item.id}>
          <span class="app-settings-nav__icon" aria-hidden="true">
            <Icon name={item.icon} size={14} />
          </span>
          {item.label}
        </a>
      {:else}
        <button
          type="button"
          role="tab"
          aria-selected={active === item.id}
          aria-controls={`settings-panel-${item.id}`}
          class:is-active={active === item.id}
          onclick={() => onSelectTab?.(item.id)}
        >
          <span class="app-settings-nav__icon" aria-hidden="true">
            <Icon name={item.icon} size={14} />
          </span>
          {item.label}
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
        {item.label}
      </a>
    {/each}
  </nav>
{/if}

<style>
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

  :global(.app-settings-nav button:hover .app-settings-nav__icon),
  :global(.app-settings-nav button.is-active .app-settings-nav__icon),
  :global(.app-settings-nav a:hover .app-settings-nav__icon),
  :global(.app-settings-nav a.is-active .app-settings-nav__icon) {
    opacity: 1;
  }
</style>
