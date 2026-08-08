<script lang="ts">
  // M03-UI-07 + M13-ADMIN-01：管理后台布局——按功能域分组的导航。
  // 所有数据与授权裁决均来自后端 API；401 → 跳登录、403 → 无权限态。
  // 菜单隐藏不是安全边界：每个页面仍由服务端权限门强制。
  import type { Snippet } from 'svelte';
  import { page } from '$app/state';

  let { children }: { children?: Snippet } = $props();

  const path = $derived(page.url.pathname);

  const navGroups = [
    {
      label: '治理',
      items: [
        { label: '用户', href: '/admin/users' },
        { label: '角色', href: '/admin/roles' },
        { label: 'Assignment', href: '/admin/assignments' },
        { label: '板块', href: '/admin/boards' },
        { label: '标签', href: '/admin/tags' },
        { label: '审核', href: '/admin/moderation/cases' }
      ]
    },
    {
      label: '经济',
      items: [
        { label: '积分', href: '/admin/points' },
        { label: '等级', href: '/admin/levels' },
        { label: '商城', href: '/admin/shop' },
        { label: '活跃', href: '/admin/activity' }
      ]
    },
    {
      label: '存储与媒体',
      items: [
        { label: '存储', href: '/admin/storage' },
        { label: '附件', href: '/admin/attachments' },
        { label: '下载计费', href: '/admin/download-billing' },
        { label: 'AI', href: '/admin/ai' },
        { label: 'Video', href: '/admin/video' }
      ]
    },
    {
      label: '集成',
      items: [
        { label: 'OIDC', href: '/admin/oauth' },
        { label: 'Marketplace', href: '/admin/marketplace' }
      ]
    },
    {
      label: '主题与插件',
      items: [
        { label: '主题', href: '/admin/themes' },
        { label: '插件', href: '/admin/plugins' }
      ]
    },
    {
      label: '其他',
      items: [
        { label: '通知', href: '/admin/notifications' },
        { label: '审计', href: '/admin/audit' },
        { label: '内容', href: '/admin/content' },
        { label: '帖子', href: '/admin/posts' },
        { label: '设置', href: '/admin/settings' }
      ]
    }
  ];
</script>

<svelte:head>
  <title>管理后台 — BBLBB</title>
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">管理后台</span>
  </nav>

  <nav class="admin-nav" aria-label="管理后台导航" style="display:flex;gap:var(--space-2);margin-bottom:var(--space-5);flex-wrap:wrap;">
    {#each navGroups as group}
      <div style="display:flex;gap:var(--space-1);align-items:center;flex-wrap:wrap;">
        <span class="text-secondary" style="font-size:var(--text-xs);margin-right:var(--space-1);">{group.label}</span>
        {#each group.items as item}
          <a
            href={item.href}
            class="btn {path === item.href || path.startsWith(item.href + '/') ? 'btn-primary' : 'btn-secondary'} btn-sm"
            aria-current={path === item.href || path.startsWith(item.href + '/') ? 'page' : undefined}
          >{item.label}</a>
        {/each}
      </div>
    {/each}
  </nav>

  {@render children?.()}
</div>
