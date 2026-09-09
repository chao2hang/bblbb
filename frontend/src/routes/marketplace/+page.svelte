<script lang="ts">
  // 市场目录页（需登录）：我的市场交易摘要 + 静态精选应用运营位。
  // 用户侧 offers 列表后端未开放（仅 Confidential Client 登记 + 按 id 读取），
  // 精选应用为静态运营位卡片；真实交易摘要来自 /marketplace/purchases。
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import Seo from '$lib/components/Seo.svelte';
  import Table from '$lib/components/ui/Table.svelte';
  import type { MarketplacePageData } from './+page.server';
  import { resolveSiteCopy, type SiteCopyView } from '$lib/site/copy';

  // data.site：根 layout 注入的全站文案（0065）；隔离渲染时兜底解析。
  let { data }: { data: MarketplacePageData & { site?: SiteCopyView | null } } = $props();

  const site = $derived<SiteCopyView>(data.site ?? resolveSiteCopy(null));

  const purchases = $derived(data.purchases);
  const totals = $derived(data.totals);
  const error = $derived(data.error);

  /** 精选应用静态运营位（由管理员配置接入，前端不臆造真实报价）。 */
  const featuredApps: Array<{
    icon: string;
    name: string;
    desc: string;
    badge: string;
    badgeTone: string;
    scopes: string;
  }> = [
    {
      icon: 'laptop',
      name: 'BBLBB CLI 工具',
      desc: '同步收藏、草稿和通知，使用 PKCE 与最小 scope 授权。',
      badge: '已审核',
      badgeTone: 'badge-success',
      scopes: 'openid · profile'
    },
    {
      icon: 'book-open',
      name: '团队知识库同步',
      desc: '将公开主题导出到团队知识库，支持每日限额。',
      badge: '待审批',
      badgeTone: 'badge-warning',
      scopes: 'openid · email'
    },
    {
      icon: 'webhook',
      name: 'Webhook 通知桥',
      desc: '把站内通知转发到团队 IM，事件订阅可配置。',
      badge: '规划中',
      badgeTone: 'badge-neutral',
      scopes: 'openid · notifications'
    }
  ];

  function statusLabel(status: string): string {
    switch (status) {
      case 'succeeded':
        return '交易成功';
      case 'partially_refunded':
        return '部分退款';
      case 'refunded':
        return '已退款';
      default:
        return status;
    }
  }

  function statusTone(status: string): string {
    switch (status) {
      case 'succeeded':
        return 'badge-success';
      case 'partially_refunded':
      case 'refunded':
        return 'badge-warning';
      default:
        return 'badge-neutral';
    }
  }

  function formatTs(ms: number): string {
    return new Date(ms).toLocaleString('zh-CN', { hour12: false });
  }
</script>

<Seo
  title="应用市场"
  description={`${site.siteName}应用市场：安全接入、Client 授权与交易状态`}
  og={{ type: 'website' }}
  jsonLd={{
    '@context': 'https://schema.org',
    '@type': 'CollectionPage',
    name: `应用市场 · ${site.siteName}`
  }}
/>

<div class="container page-content">

  <header style="margin-bottom:var(--space-4);">
    <h1 style="display:flex;align-items:center;gap:var(--space-2);margin:0 0 var(--space-2);">
      <Icon name="shopping-bag" size={24} />
      应用市场
    </h1>
    <p class="text-secondary" style="margin:0;">安全接入、Client 授权、限额与交易状态</p>
  </header>

  {#if error}
    <p class="input-hint is-error" role="alert">{error}</p>
  {/if}

  <div class="card" style="margin-bottom:var(--space-4);">
    <div class="card-body">
      <p class="text-secondary" style="margin:0;display:flex;align-items:flex-start;gap:var(--space-2);">
        <Icon name="info" size={16} />
        市场应用由管理员配置，接入请联系平台。市场交易不直接改余额；Webhook、对账和退款由后端负责。
      </p>
    </div>
  </div>

  <!-- 精选应用（静态运营位） -->
  <section aria-labelledby="marketplace-featured" style="margin-bottom:var(--space-4);">
    <div class="card-header" style="margin-bottom:var(--space-3);">
      <span class="card-title" id="marketplace-featured">精选应用</span>
    </div>
    <div class="articles-grid">
      {#each featuredApps as app (app.name)}
        <div class="card" data-testid="featured-app">
          <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-2);">
            <div style="display:flex;align-items:center;justify-content:space-between;gap:var(--space-2);">
              <span
                aria-hidden="true"
                style="display:inline-flex;align-items:center;justify-content:center;width:40px;height:40px;border-radius:var(--radius-md);background:var(--color-accent-soft,#e8f0fe);color:var(--color-accent);"
              >
                <Icon name={app.icon} size={20} />
              </span>
              <span class="badge {app.badgeTone}">{app.badge}</span>
            </div>
            <h3 style="margin:0;font-size:var(--text-lg);">{app.name}</h3>
            <p class="text-secondary" style="margin:0;flex:1;">{app.desc}</p>
            <p
              class="text-secondary"
              style="margin:0;font-size:var(--text-sm);display:flex;align-items:center;gap:var(--space-2);"
            >
              <Icon name="key" size={14} />
              {app.scopes}
            </p>
          </div>
        </div>
      {/each}
    </div>
  </section>

  <!-- 我的市场交易摘要 -->
  <section aria-labelledby="marketplace-purchases">
    <div
      class="card-header"
      style="margin-bottom:var(--space-3);display:flex;align-items:center;justify-content:space-between;gap:var(--space-2);"
    >
      <span class="card-title" id="marketplace-purchases">我的市场交易</span>
      <a class="btn btn-secondary" href="/marketplace/purchases">查看全部购买</a>
    </div>

    <div class="stats-grid stats-grid-3" style="margin-bottom:var(--space-3);">
      <div class="stat-card">
        <div class="stat-card-value">{totals.count}</div>
        <div class="stat-card-label">累计交易</div>
      </div>
      <div class="stat-card">
        <div class="stat-card-value">{totals.currency ? `${totals.spent} ${totals.currency.toUpperCase()}` : totals.spent || '—'}</div>
        <div class="stat-card-label">累计支出</div>
      </div>
      <div class="stat-card">
        <div class="stat-card-value">{totals.currency ? `${totals.fees} ${totals.currency.toUpperCase()}` : totals.fees || '—'}</div>
        <div class="stat-card-label">平台费合计</div>
      </div>
    </div>

    {#if purchases.length === 0}
      <div class="card">
        <div class="card-body">
          <EmptyState
            icon="shopping-bag"
            title="暂无市场交易"
            desc="只有你本人通过商户结账页确认的交易会显示在这里"
          />
        </div>
      </div>
    {:else}
      <div class="card">
        <div class="card-body">
          <Table
            caption="我的市场交易摘要"
            columns={[
              { label: '商户 / 订单' },
              { label: '金额', align: 'right' },
              { label: '平台费', align: 'right' },
              { label: '状态' }
            ]}
          >
            {#each purchases as p (p.id)}
              <tr>
                <td>
                  <span class="mono" style="font-size:var(--text-sm);">{p.client_id.slice(0, 8)}…</span>
                  <div class="text-secondary" style="font-size:var(--text-xs);">
                    {formatTs(p.created_at)} · ×{p.quantity}
                  </div>
                </td>
                <td class="table-cell-right">{p.amount} {p.currency_id.toUpperCase()}</td>
                <td class="table-cell-right">{p.fee_amount} {p.currency_id.toUpperCase()}</td>
                <td><span class="badge {statusTone(p.status)}">{statusLabel(p.status)}</span></td>
              </tr>
            {/each}
          </Table>
          <p class="text-secondary" style="font-size:var(--text-sm);margin:var(--space-2) 0 0;">
            仅展示最近 {purchases.length} 笔，完整记录与退款状态见「查看全部购买」。
          </p>
        </div>
      </div>
    {/if}
  </section>
</div>
