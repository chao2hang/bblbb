<script lang="ts">
  // M18-ADMIN-BI：BI 数据看板（对齐原型 #admin-bi 时间 Tab 置顶与指标单行结构）。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import { page } from '$app/state';
  import type { AdminBiPageData } from './+page.server';

  let { data }: { data: AdminBiPageData } = $props();

  const PERIOD_TABS = [
    { value: 'day', label: '今日' },
    { value: 'week', label: '本周' },
    { value: 'month', label: '本月' },
    { value: 'year', label: '今年' }
  ];

  function getPeriod(): string {
    try {
      return page.url.searchParams.get('period') ?? data.period ?? 'day';
    } catch {
      return data.period ?? 'day';
    }
  }

  const currentPeriod = $derived(getPeriod());


  const isRealData = $derived(
    Boolean(data.metrics && Array.isArray(data.metrics.metrics) && data.metrics.metrics.length > 0)
  );

  // P0 整改：API 无数据/失败必须显示空态，禁止伪造 KPI（此前回退固定演示快照）。
  const activeMetricsData = $derived.by(() => {
    if (data.state === 'ok' && data.metrics && Array.isArray(data.metrics.metrics)) {
      return {
        metrics: data.metrics.metrics,
        generatedAt: data.metrics.generated_at
      };
    }
    return { metrics: [], generatedAt: 0 };
  });

  const displayList = $derived(
    activeMetricsData.metrics.map((m) => {
      const target = m.target > 0 ? m.target : 100;
      const pct = Math.min(100, Math.max(0, Math.round((m.value / target) * 100)));
      const deltaSign = m.delta_pct > 0 ? '+' : '';
      return {
        key: m.key,
        label: m.label,
        current: Number(m.value).toLocaleString('zh-CN'),
        total: Number(target).toLocaleString('zh-CN'),
        pct,
        delta: `${deltaSign}${m.delta_pct}% 环比`,
        isPositive: m.delta_pct >= 0
      };
    })
  );

  function formatTime(ts: number): string {
    if (!ts) return '刚刚';
    const d = new Date(ts > 1e11 ? ts : ts * 1000);
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')} ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
  }

  function periodName(p: string): string {
    const tab = PERIOD_TABS.find((t) => t.value === p);
    return tab?.label ?? p;
  }
</script>

<svelte:head>
  <title>BI 数据看板 — BBLBB Admin</title>
</svelte:head>

<PageHeader title="BI 数据看板" />

<!-- 原型对齐：周期 Tabs 独立置顶于卡片外部 -->
<div class="tabs" role="tablist" aria-label="统计周期" style="margin-bottom:14px;background:var(--color-bg-subtle);border-radius:var(--radius-sm);padding:2px;">
  {#each PERIOD_TABS as tab}
    <a
      role="tab"
      aria-selected={currentPeriod === tab.value}
      href="/admin/bi?period={tab.value}"
      class="tab {currentPeriod === tab.value ? 'is-active' : ''}"
      style="padding:8px 16px;font-size:13px;text-decoration:none;"
    >
      {tab.label}
    </a>
  {/each}
</div>

<!-- 关键指标卡片 -->
{#if displayList.length === 0}
  <section class="app-card" style="margin-bottom:14px;">
    <div class="app-card__body">
      <EmptyState icon="bar-chart" title="暂无统计数据" desc="当前周期下暂无聚合分析指标" />
    </div>
  </section>
{:else}
  <section class="app-card" style="margin-bottom:14px;">
    <header class="app-card__head" style="display:flex;justify-content:space-between;align-items:center;">
      <h2>关键指标（{periodName(currentPeriod)}）</h2>
      {#if !isRealData}
        <span class="sbadge sb-brand" style="font-size:11px;">演示数据</span>
      {:else}
        <span class="sbadge sb-success" style="font-size:11px;">实时数据</span>
      {/if}
    </header>
    <div class="app-card__body" style="display:flex;flex-direction:column;gap:14px;">
      {#each displayList as m (m.key)}
        <div class="app-card" style="padding:14px 16px;border:1px solid var(--color-border);">
          <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:8px;">
            <b style="font-size:14px;">{m.label}</b>
            <span style="font-size:13px;color:var(--color-text-secondary);font-weight:500;">
              {m.current} / {m.total}
            </span>
          </div>
          <!-- 进度条 -->
          <div style="height:6px;background:var(--color-bg-subtle);border-radius:3px;overflow:hidden;margin-bottom:8px;">
            <div style="width:{m.pct}%;height:100%;background:var(--color-brand);border-radius:3px;"></div>
          </div>
          <div style="font-size:12px;color:{m.isPositive ? 'var(--color-success)' : 'var(--color-danger)'};font-weight:500;">
            {m.delta}
          </div>
        </div>
      {/each}
    </div>
    <footer class="app-card__foot" style="padding:10px 16px;font-size:11px;color:var(--color-text-secondary);">
      数据时间戳：{formatTime(activeMetricsData.generatedAt)} · 聚合周期：{periodName(currentPeriod)} · {isRealData ? '实时数据库统计聚合' : '独立演示周期快照'}
    </footer>
  </section>
{/if}
