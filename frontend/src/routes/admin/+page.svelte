// M03-UI-07：管理后台首页——各管理入口导航卡片。
<script lang="ts">
  import Icon from '$lib/components/ui/Icon.svelte';

  const sections = [
    { href: '/admin/boards', icon: 'layout-dashboard', title: '板块管理', desc: '创建/更新板块（slug、可见性、发帖模式）' },
    { href: '/admin/tags', icon: 'tag', title: '标签管理', desc: '创建/更新标签（颜色、分组）' },
    { href: '/admin/roles', icon: 'shield-check', title: '角色管理', desc: '系统角色与权限' },
    { href: '/admin/assignments', icon: 'users', title: 'Assignment', desc: '板块/用户角色授予与有效期' },
    { href: '/admin/shop', icon: 'shopping-bag', title: '商城管理', desc: '商品 CRUD/发布/停售、订单与退款' },
    { href: '/admin/storage', icon: 'monitor', title: '存储管理', desc: 'local/S3 配置、TTL、测试连接与迁移说明' },
    { href: '/admin/activity', icon: 'activity', title: '活跃管理', desc: '签到与任务奖励配置' }
  ];
</script>

<div class="admin-grid" style="display:grid;grid-template-columns:repeat(auto-fill,minmax(240px,1fr));gap:var(--space-4);">
  {#each sections as section}
    <a class="card" href={section.href} style="text-decoration:none;color:inherit;display:block;">
      <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-2);">
        <Icon name={section.icon} size={22} />
        <strong>{section.title}</strong>
        <span class="text-secondary" style="font-size:var(--text-sm);">{section.desc}</span>
      </div>
    </a>
  {/each}
</div>
