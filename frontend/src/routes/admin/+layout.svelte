<script lang="ts">
  // M03-UI-07 + M13-ADMIN-01：管理后台布局
  // 原型对齐：prototype/pages/admin.html / admin-*.html
  // 经典后台双栏布局：左侧 .app-admin-side 模块树，右侧 .app-admin-main 内容区
  import type { Snippet } from 'svelte';
  import { page } from '$app/state';
  import Icon from '$lib/components/ui/Icon.svelte';

  let { children }: { children?: Snippet } = $props();

  const path = $derived(page.url.pathname);

  const navGroups = [
    {
      label: '概览',
      items: [
        { label: '仪表盘', href: '/admin', icon: 'layout-dashboard', exact: true }
      ]
    },
    {
      label: '内容',
      items: [
        { label: '帖子与文章', href: '/admin/posts', icon: 'file-text' },
        { label: '板块管理', href: '/admin/boards', icon: 'workflow' },
        { label: '标签管理', href: '/admin/tags', icon: 'tag' },
        { label: '附件管理', href: '/admin/attachments', icon: 'paperclip' }
      ]
    },
    {
      label: '用户与权限',
      items: [
        { label: '用户管理', href: '/admin/users', icon: 'users' },
        { label: '角色与权限', href: '/admin/roles', icon: 'shield' },
        { label: '举报与审核', href: '/admin/moderation/cases', icon: 'flag' },
      ]
    },
    {
      label: '激励与交易',
      items: [
        { label: '成就管理', href: '/admin/achievements', icon: 'trophy' },
        { label: '积分与货币', href: '/admin/points', icon: 'coins' },
        { label: '等级管理', href: '/admin/levels', icon: 'award' },
        { label: '商城管理', href: '/admin/shop', icon: 'shopping-bag' },
        { label: '签到与活跃', href: '/admin/activity', icon: 'activity' },
        { label: '市场与交易', href: '/admin/marketplace', icon: 'shopping-cart' },
        { label: '下载计费', href: '/admin/download-billing', icon: 'download' }
      ]
    },
    {
      label: '扩展与集成',
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
      items: [
        { label: '文件存储', href: '/admin/storage', icon: 'server' },
        { label: '通知与邮件', href: '/admin/notifications', icon: 'bell' },
        { label: '审计日志', href: '/admin/audit', icon: 'file-text' },
        { label: '系统设置', href: '/admin/settings', icon: 'settings' },
        { label: 'BI 看板', href: '/admin/bi', icon: 'bar-chart' }
      ]
    }
  ];

  function isItemActive(item: { href: string; exact?: boolean }): boolean {
    if (item.exact) return path === item.href;
    return path === item.href || path.startsWith(item.href + '/');
  }

  // 移动端菜单当前项标题（桌面侧栏高亮同一来源）。
  const activeLabel = $derived(
    navGroups.flatMap((group) => group.items).find((item) => isItemActive(item))?.label ?? '管理后台'
  );

  const activeGroupIndex = $derived.by(() => {
    const idx = navGroups.findIndex((group) => group.items.some((item) => isItemActive(item)));
    return idx >= 0 ? idx : 0;
  });

  // 手风琴折叠状态：当前路由所在分组默认展开，点击头部展开对应组并折叠其他组
  let openGroupIndex = $state<number | null>(null);

  $effect(() => {
    openGroupIndex = activeGroupIndex;
  });

  function toggleGroup(index: number): void {
    if (openGroupIndex === index) {
      openGroupIndex = null;
    } else {
      openGroupIndex = index;
    }
  }

  let mobileMenuOpen = $state(false);

  function closeMobileMenu(): void {
    mobileMenuOpen = false;
  }
</script>

<svelte:head>
  <title>管理后台 — BBLBB</title>
</svelte:head>

<div class="container app-page app-admin-page-shell">
  <section class="page app-page app-route-admin" id="page-admin">
    <div class="app-admin-shell" class:admin-menu-open={mobileMenuOpen}>
      <!-- 原型对齐（全 25 页统一）：全宽卡片式「☰ 管理菜单」按钮，无下拉箭头。 -->
      <button
        type="button"
        class="app-admin-menu"
        aria-controls="admin-navigation"
        aria-expanded={mobileMenuOpen}
        onclick={() => (mobileMenuOpen = !mobileMenuOpen)}
      >
        <Icon name="menu" size={16} />
        <span>管理菜单</span>
      </button>

      <button
        type="button"
        class="app-admin-menu-backdrop"
        aria-label="关闭管理菜单"
        tabindex="-1"
        onclick={closeMobileMenu}
      ></button>

      <!-- 与原型 #page-admin 完全同构的后台导航树 -->
      <aside class="app-admin-side" id="admin-navigation" aria-label="后台模块导航">
        <div class="app-admin-side__brand">BBLBB Admin</div>
        {#each navGroups as group, index}
          {@const isExpanded = openGroupIndex === index}
          <div class="app-admin-side__group" class:is-expanded={isExpanded}>
            <button
              type="button"
              class="app-admin-side__label"
              data-admin-group-toggle=""
              aria-controls={`admin-group-items-${index}`}
              aria-expanded={isExpanded}
              onclick={() => toggleGroup(index)}
            >
              <span>{group.label}</span>
              <Icon name="chevron-down" size={12} />
            </button>
            <div class="app-admin-side__items" id={`admin-group-items-${index}`}>
              {#each group.items as item}
                <a
                  href={item.href}
                  class={isItemActive(item) ? 'is-active' : ''}
                  aria-current={isItemActive(item) ? 'page' : undefined}
                  onclick={closeMobileMenu}
                >
                  <Icon name={item.icon} size={14} />
                  {item.label}
                </a>
              {/each}
            </div>
          </div>
        {/each}
        <div class="app-admin-side__foot">
          管理员控制台 · 所有变更均需真实 API、权限与审计确认
        </div>
      </aside>

      <!-- 右侧主内容区 -->
      <div class="app-admin-main">
        {@render children?.()}
      </div>
    </div>
  </section>
</div>
