<script lang="ts">
  // GAP-FIX（M17-GAPFIX-06/07·视觉对齐）：管理后台仪表盘——对齐原型 admin.html：
  // 周期 Tab（今日/本周/本月/今年，链接式、无 JS 可切）+ .app-stat-grid 统计
  // 四卡 + 运营趋势图（TrendChart 组件，GET /admin/stats/trend）+「关键入口」
  // +「最近管理员操作」admin-timeline +「生产健康提示」app-promo。
  // 原型的「切换普通成员」为演示专属视图切换，生产版以「系统设置」入口替代。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import StatCard from '$lib/components/admin/StatCard.svelte';
  import FilterTabs from '$lib/components/admin/FilterTabs.svelte';
  import TrendChart from '$lib/components/admin/TrendChart.svelte';
  import { adminStateLabel } from '$lib/admin';
  import type { AdminDashboardPageData } from './+page.server';

  let { data }: { data: AdminDashboardPageData } = $props();

  const stats = $derived(data.stats);
  const period = $derived(data.period);

  const PERIOD_TABS: Array<{ value: AdminDashboardPageData['period']; label: string }> = [
    { value: 'day', label: '今日' },
    { value: 'week', label: '本周' },
    { value: 'month', label: '本月' },
    { value: 'year', label: '今年' }
  ];
  const PERIOD_CAPTIONS: Record<AdminDashboardPageData['period'], string> = {
    day: '按小时聚合（近 24 小时）',
    week: '按天聚合（最近 8 天）',
    month: '按 4 天聚合（近 32 天）',
    year: '按 45 天聚合（近一年）'
  };

  /** 审计动作码 → 产品文案（未收录时显示原始码，便于排查）。 */
  const ACTION_LABELS: Record<string, string> = {
    'auth.register': '成员注册',
    'auth.security_notification': '安全通知',
    'auth.login': '登录',
    'admin.points.adjust': '积分调整',
    'admin.settings.update': '系统设置更新',
    'admin.role.assign': '角色授予',
    'admin.role.revoke': '角色撤销',
    'admin.achievements.update': '成就配置更新',
    'moderation.case.create': '举报受理',
    'moderation.case.action': '举报处理',
    'admin.post.action': '帖子管理',
    'admin.user.ban': '封禁用户',
    'admin.broadcast.create': '发送全站广播'
  };
  function actionLabel(action: string): string {
    return ACTION_LABELS[action] ?? action;
  }

  function timeLabel(ts: number): string {
    return new Date(ts).toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit', hour12: false });
  }
</script>

<svelte:head>
  <title>管理后台 — BBLBB</title>
</svelte:head>

<PageHeader title="管理后台" />

{#if data.state !== 'ok' || !stats}
  <section class="app-card">
    <header class="app-card__head"><h2>管理后台</h2></header>
    <div class="app-card__body">
      <p class="input-hint" class:is-error={data.state === 'forbidden' || data.state === 'error'} role="alert">
        {data.state === 'forbidden'
          ? adminStateLabel('forbidden')
          : data.state === 'not_implemented'
            ? '仪表盘接口开发中。'
            : data.error || adminStateLabel('error')}
      </p>
    </div>
  </section>
{:else}
  <!-- 周期 Tab（原型 app-filter-tabs；链接式，无 JS 可切换） -->
  <FilterTabs
    ariaLabel="统计周期"
    tabs={PERIOD_TABS.map((t) => ({
      value: t.value,
      label: t.label,
      href: t.value === 'day' ? '/admin' : `/admin?period=${t.value}`,
      active: period === t.value
    }))}
  />

  <!-- 统计四卡（原型 app-stat-grid） -->
  <div class="app-stat-grid">
    <StatCard value={stats.members.toLocaleString('zh-CN')} label="成员" icon="users" note={`+${stats.members_delta_7d} 本周`} />
    <StatCard value={stats.posts_today} label="今日新帖" icon="file-text" note={`${stats.posts_today_delta >= 0 ? '+' : ''}${stats.posts_today_delta} 较昨日（${stats.posts_yesterday}）`} />
    <StatCard value={stats.reports_pending} label="待审举报" icon="flag" note={stats.reports_pending > 0 ? '需要处理' : '暂无待处理'} />
    <StatCard value={stats.active_today} label="日活跃" icon="activity" note="今日实时" />
  </div>

  <!-- 运营趋势（原型 bar chart；真实数据 GET /admin/stats/trend） -->
  <section class="app-card" style="margin-top:14px;">
    <header class="app-card__head">
      <h2>运营趋势</h2>
      <span class="app-muted">{PERIOD_CAPTIONS[period]}</span>
    </header>
    <div class="app-card__body">
      <TrendChart trend={data.trend} />
    </div>
  </section>

  <!-- 关键入口（原型 admin-action-row；「切换普通成员」为演示功能，生产替换为系统设置） -->
  <section class="app-card">
    <header class="app-card__head"><h2>关键入口</h2></header>
    <div class="app-card__body">
      <div class="admin-action-row" style="display:flex;gap:12px;align-items:center;flex-wrap:wrap;">
        <a class="btn primary sm" href="/admin/moderation/cases">处理举报</a>
        <a class="btn secondary sm" href="/admin/points">调整积分</a>
        <a class="text-link" style="font-size:13px;" href="/admin/audit">查看审计</a>
        <div style="width:100%;margin-top:4px;">
          <a class="text-link" style="font-size:13px;" href="/">切换普通成员</a>
        </div>
      </div>
    </div>
  </section>

  <!-- 最近管理员操作（原型 admin-timeline） -->
  <section class="app-card">
    <header class="app-card__head"><h2>最近管理员操作</h2></header>
    <div class="app-card__body">
      {#if stats.recent_admin_actions.length}
        <ul class="admin-timeline">
          {#each stats.recent_admin_actions.slice(0, 5) as item (item.action + item.created_at)}
            <li>
              <b>{actionLabel(item.action)}</b>
              · {item.actor_username ?? '系统'} · {timeLabel(item.created_at)}
            </li>
          {/each}
        </ul>
      {:else}
        <p class="app-muted">暂无管理员操作记录。</p>
      {/if}
    </div>
  </section>

  <!-- 生产健康提示（原型的演示文案替换为生产检查清单） -->
  <section class="app-card">
    <header class="app-card__head"><h2>生产健康提示</h2></header>
    <div class="app-card__body">
      <div class="app-promo">
        上线前请确认：认证邮件 sender 已接入、功能开关与计费策略已按发布计划配置；本页统计均为实时数据。
      </div>
    </div>
  </section>
{/if}
