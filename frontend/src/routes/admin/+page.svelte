<script lang="ts">
  // M03-UI-07 + M13-ADMIN-01：管理后台首页——各管理入口导航卡片。
  import Icon from '$lib/components/ui/Icon.svelte';

  const sections = [
    { href: '/admin/users', icon: 'users', title: '用户管理', desc: '用户状态/角色（user.manage）' },
    { href: '/admin/roles', icon: 'shield-check', title: '角色管理', desc: '系统角色与权限（role.manage）' },
    { href: '/admin/assignments', icon: 'user-plus', title: 'Assignment', desc: '板块/用户角色授予与有效期' },
    { href: '/admin/boards', icon: 'layout-dashboard', title: '板块管理', desc: '创建/更新板块（slug、可见性、发帖模式）' },
    { href: '/admin/tags', icon: 'tag', title: '标签管理', desc: '创建/更新标签（颜色、分组）' },
    { href: '/admin/moderation/cases', icon: 'inbox', title: '审核案件', desc: '举报案件/分配/处置（moderation.review）' },
    { href: '/admin/shop', icon: 'shopping-bag', title: '商城管理', desc: '商品 CRUD/发布/停售、订单与退款' },
    { href: '/admin/storage', icon: 'monitor', title: '存储管理', desc: 'local/S3 配置、TTL、测试连接与迁移说明' },
    { href: '/admin/activity', icon: 'activity', title: '活跃管理', desc: '签到与任务奖励配置' },
    { href: '/admin/points', icon: 'coins', title: '积分/活跃', desc: '奖励开关与积分策略（只读配置）' },
    { href: '/admin/levels', icon: 'bar-chart', title: '等级/配额', desc: '等级附件配额策略' },
    { href: '/admin/ai', icon: 'sparkles', title: 'AI 管理', desc: 'Provider 脱敏状态/任务重试取消' },
    { href: '/admin/video', icon: 'play', title: 'Video 管理', desc: 'Direct/HLS/Xigua Provider 策略' },
    { href: '/admin/oauth', icon: 'key', title: 'OIDC 管理', desc: 'OAuth Client（secret 只显示一次）' },
    { href: '/admin/marketplace', icon: 'store', title: 'Marketplace', desc: 'Client/Scope/Offer/Webhook/对账' },
    { href: '/admin/download-billing', icon: 'download', title: '下载计费', desc: 'Download Billing 策略（默认关闭）' },
    { href: '/admin/themes', icon: 'palette', title: '主题管理', desc: '数据型主题上传/预览/默认/Token 编辑' },
    { href: '/admin/plugins', icon: 'puzzle', title: '插件管理', desc: 'v1 配置型插件安装/启停/设置' },
    { href: '/admin/notifications', icon: 'bell', title: '通知', desc: '通知相关配置入口' },
    { href: '/admin/audit', icon: 'scroll-text', title: '审计', desc: '审计与文档入口' },
    { href: '/admin/settings', icon: 'settings', title: '后台设置', desc: '跨域设置总览' }
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
