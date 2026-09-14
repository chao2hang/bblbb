<script lang="ts">
  // GAP-FIX（M17-GAPFIX-06/07·视觉对齐）+ M19（值班台优化）：管理后台仪表盘。
  // 结构：MetricGroup 统计条（去盒、发丝线分隔，状态走语义色调）→ 运营趋势
  // （含链接式周期控制与 TrendChart）→ 双栏值班区：
  // [最近管理员操作 | 关键入口 + 生产健康提示]（<1080px 折叠单列）。
  // 原型的「切换普通成员」为演示专属视图切换，生产版以「系统设置」入口替代。
  // 数据源：GET /api/v1/admin/stats 与 GET /api/v1/admin/stats/trend。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import FilterTabs from '$lib/components/admin/FilterTabs.svelte';
  import TrendChart from '$lib/components/admin/TrendChart.svelte';
  import MetricGroup from '$lib/components/ui/MetricGroup.svelte';
  import Meta from '$lib/components/ui/Meta.svelte';
  import { adminStateLabel } from '$lib/admin';
  import type { AdminDashboardPageData } from './+page.server';

  let { data }: { data: AdminDashboardPageData } = $props();

  const stats = $derived(data.stats);
  const period = $derived(data.period);
  const trendHref = $derived(period === 'day' ? '/admin' : `/admin?period=${period}`);

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

  /** 带符号数字：正 +N、负原样、零 ±0（供注记行使用）。 */
  function signed(n: number): string {
    return n > 0 ? `+${n}` : n < 0 ? `${n}` : '±0';
  }
  type NoteTone = 'default' | 'success' | 'warning' | 'danger';
  function deltaTone(n: number): NoteTone {
    return n > 0 ? 'success' : n < 0 ? 'danger' : 'default';
  }

  // 统计条（MetricGroup ≤4 项契约）：状态编码进注记/值色，不做装饰。
  const metricItems = $derived(
    stats
      ? [
          {
            label: '成员',
            icon: 'users',
            value: stats.members.toLocaleString('zh-CN'),
             href: '/admin/users',
            note: `本周 ${signed(stats.members_delta_7d)}`,
            note_tone: deltaTone(stats.members_delta_7d) as 'success' | 'danger' | 'default'
          },
          {
            label: '今日新帖',
            icon: 'file-text',
            value: stats.posts_today,
             href: '/admin/posts',
            note: `较昨日 ${signed(stats.posts_today_delta)}（昨日 ${stats.posts_yesterday}）`,
            note_tone: deltaTone(stats.posts_today_delta) as 'success' | 'danger' | 'default'
          },
          {
            label: '待审举报',
            icon: 'flag',
            value: stats.reports_pending,
             href: '/admin/moderation/cases',
            tone: (stats.reports_pending > 0 ? 'warning' : 'default') as 'warning' | 'default',
            note: stats.reports_pending > 0 ? '需要处理' : '暂无待处理',
            note_tone: (stats.reports_pending > 0 ? 'warning' : 'default') as 'warning' | 'default'
          },
          { label: '日活跃', icon: 'activity', value: stats.active_today, href: '/admin/activity', note: '今日实时' }
        ]
      : []
  );

  /** 审计动作码 → 产品文案（未收录时回退展示 mono 原始码，便于排查）。 */
  const ACTION_LABELS: Record<string, string> = {
    'auth.register': '成员注册',
    'auth.security_notification': '安全通知',
    'auth.login': '登录',
    'auth.mfa_disabled': '关闭 MFA',
    'auth.mfa_recovery_codes_generated': '重置恢复码',
    'me.password.change': '修改密码',
    'admin.points.adjust': '积分调整',
    'admin.settings.update': '系统设置更新',
    'admin.user.create': '创建用户',
    'admin.user.update': '更新用户',
    'admin.user.role.grant': '角色授予',
    'admin.user.role.revoke': '角色撤销',
    'admin.role.assign': '角色授予',
    'admin.role.revoke': '角色撤销',
    'admin.role.create': '角色创建',
    'admin.role.update': '角色更新',
    'admin.ban_user': '封禁用户',
    'admin.post.action': '帖子管理',
    'admin.post.feature': '帖子设精',
    'admin.post.unfeature': '取消精华',
    'post.moderate': '内容审核',
    'admin.tag_create': '标签创建',
    'admin.tag.merge': '标签合并',
    'admin.achievements.update': '成就配置更新',
    'admin.achievement.create': '成就创建',
    'admin.achievement.update': '成就更新',
    'admin.achievement.delete': '成就删除',
    'admin.achievement.grant': '发放成就',
    'admin.achievement.icon_upload': '成就图标上传',
    'admin.achievement.icon_remove': '成就图标移除',
    'admin.level.update': '等级规则更新',
    'admin.storage_config_update': '存储配置更新',
    'admin.storage_test': '存储连接测试',
    'admin.attachment_quota_update': '附件配额更新',
    'admin.notification.broadcast': '发送全站广播',
    'admin.broadcast.create': '发送全站广播',
    'admin.notification.recall': '撤回广播',
    'moderation.case.create': '举报受理',
    'moderation.case.action': '举报处理',
    'attachment.create': '附件上传',
    'attachment.complete': '附件就绪',
    'theme.upload': '主题上传',
    'theme.delete': '主题删除',
    'theme.default.update': '默认主题更新',
    'plugin.install': '插件安装',
    'plugin.uninstall': '插件卸载',
    'plugin.settings.update': '插件配置更新',
    'ai.config.update': 'AI 配置更新',
    'video_policy.update': '视频策略更新',
    'security.password_changed': '密码变更',
    'security.new_device': '新设备登录',
    'security.mfa_changed': 'MFA 变更',
    'marketplace.client.update': '应用配置更新',
    'marketplace.refund.retry': '退款重试',
    'marketplace.webhook.replay': 'Webhook 重放',
    'marketplace.reconciliation.run': '对账执行'
  };
  function actionLabel(action: string): string | null {
    return ACTION_LABELS[action] ?? null;
  }

  /** 非今日动作显示 M/D HH:MM，今日只显示 HH:MM。 */
  function timeLabel(ts: number): string {
    const d = new Date(ts);
    const now = new Date();
    const sameDay =
      d.getFullYear() === now.getFullYear() && d.getMonth() === now.getMonth() && d.getDate() === now.getDate();
    const hm = `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
    return sameDay ? hm : `${d.getMonth() + 1}/${d.getDate()} ${hm}`;
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
  <!-- 统计条：去盒仪表条（发丝线分隔），状态由注记/值色承载 -->
  <div class="dash-metrics">
    <MetricGroup items={metricItems} />
  </div>

  <!-- 运营趋势（真实数据 GET /admin/stats/trend；零周期给可行动空态） -->
  <section class="app-card">
    <header class="app-card__head">
      <div class="dash-trend-head__copy">
         <h2>运营趋势</h2>
      <span class="app-muted">{PERIOD_CAPTIONS[period]}</span>
       </div>
       <FilterTabs
         ariaLabel="趋势周期"
         style="margin:0;"
         tabs={PERIOD_TABS.map((t) => ({
           value: t.value,
           label: t.label,
           href: t.value === 'day' ? '/admin' : `/admin?period=${t.value}`,
           active: period === t.value
         }))}
       />
    </header>
    <div class="app-card__body">
      <TrendChart trend={data.trend} retryHref={trendHref} />
    </div>
  </section>

  <!-- 值班区：左 = 操作流水（宽），右 = 关键入口 + 上线检查 -->
  <div class="dash-cols">
    <!-- 最近管理员操作（原型 admin-timeline；mono 元信息 + 日期感知时间） -->
    <section class="app-card">
      <header class="app-card__head">
        <h2>最近管理员操作</h2>
        <a class="text-link dash-inline-action" href="/admin/audit">查看全部</a>
      </header>
      <div class="app-card__body">
        {#if stats.recent_admin_actions.length}
          <ul class="admin-timeline">
            {#each stats.recent_admin_actions.slice(0, 5) as item, index (item.action + item.created_at + (item.actor_username || '') + index)}
              <li>
                {#if actionLabel(item.action)}
                  <b>{actionLabel(item.action)}</b>
                {:else}
                  <b class="dash-code">{item.action}</b>
                {/if}
                <Meta items={[item.actor_username || '系统', timeLabel(item.created_at)]} />
              </li>
            {/each}
          </ul>
        {:else}
          <p class="app-muted">暂无管理员操作记录。</p>
        {/if}
      </div>
    </section>

    <div class="dash-side">
      <!-- 关键入口（原型 admin-action-row；「切换普通成员」为演示功能，生产以系统设置替代） -->
      <section class="app-card">
        <header class="app-card__head"><h2>关键入口</h2></header>
        <div class="app-card__body">
          <div class="admin-action-row">
            <a class="btn primary sm" href="/admin/moderation/cases">处理举报</a>
            <a class="btn secondary sm" href="/admin/points">调整积分</a>
            <a class="text-link dash-inline-action" href="/admin/settings">系统设置</a>
          </div>
        </div>
      </section>

      <!-- 生产健康提示（去盒注记；分组语义由章节标签承担，不再套 promo 盒） -->
      <section class="app-card">
        <header class="app-card__head"><h2>生产健康提示</h2></header>
        <div class="app-card__body">
          <p class="dash-note">
            上线前请确认：认证邮件 sender 已接入、功能开关与计费策略已按发布计划配置；本页统计均为实时数据。
          </p>
        </div>
      </section>
    </div>
  </div>
{/if}

<style>
  .dash-metrics {
    margin-bottom: var(--space-5);
  }

  .app-card__head:has(.dash-trend-head__copy) {
    align-items: center;
    gap: var(--space-4);
  }

  .dash-trend-head__copy {
    display: grid;
    min-width: 0;
    gap: 4px;
  }

  .dash-inline-action {
    display: inline-flex;
    align-items: center;
    min-height: 44px;
    padding: 0 4px;
  }

  @media (max-width: 767px) {
    .app-card__head:has(.dash-trend-head__copy) {
      align-items: flex-start;
      flex-direction: column;
      gap: 6px;
    }

    .app-card__head:has(.dash-trend-head__copy) :global(.app-filter-tabs) {
      width: 100%;
    }
  }

  /* 值班区双栏：流水为主（宽），入口与提示为辅；窄屏折叠单列。 */
  .dash-cols {
    display: grid;
    gap: var(--space-5);
    margin-top: var(--space-5);
  }

  .dash-side {
    display: grid;
    min-width: 0;
    align-content: start;
    gap: var(--space-5);
  }

  @media (min-width: 1080px) {
    .dash-cols {
      grid-template-columns: minmax(0, 1.6fr) minmax(0, 1fr);
    }
  }

  /* 未收录审计码：mono 原始码（排查用），不伪装成产品文案。 */
  .dash-code {
    font-family: var(--font-family-mono);
    font-size: 11px;
    font-weight: 500;
    letter-spacing: var(--meta-letter-spacing);
  }

  .dash-note {
    margin: 0;
    color: var(--color-text-secondary);
    font-size: 13px;
    line-height: 1.6;
  }
</style>
