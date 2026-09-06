<script lang="ts">
  // M18-ADMIN-BI：BI 数据看板（对齐原型 #admin-bi 时间 Tab 置顶与指标单行结构）。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { page } from '$app/state';
  import type { AdminBiPageData } from './+page.server';

  let { data }: { data: AdminBiPageData } = $props();

  const PERIOD_TABS = [
    { value: 'day', label: '今日' },
    { value: 'week', label: '本周' },
    { value: 'month', label: '本月' },
    { value: 'year', label: '今年' }
  ];

  const currentPeriod = $derived(page.url.searchParams.get('period') ?? 'day');

  const mockMetrics = [
    { label: '活跃成员', current: '421', total: '1,284', pct: 33, delta: '+2.6% 环比' },
    { label: '新增内容', current: '56', total: '200', pct: 28, delta: '+8.1% 环比' },
    { label: '积分流水', current: '1,830', total: '5,000', pct: 37, delta: '+3.4% 环比' },
    { label: '审核通过率', current: '88', total: '100', pct: 88, delta: '+1.2% 环比' }
  ];
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
<section class="app-card" style="margin-bottom:14px;">
  <header class="app-card__head">
    <h2>关键指标</h2>
  </header>
  <div class="app-card__body" style="display:flex;flex-direction:column;gap:14px;">
    {#each mockMetrics as m}
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
        <div style="font-size:12px;color:var(--color-success);font-weight:500;">
          {m.delta}
        </div>
      </div>
    {/each}
  </div>
  <footer class="app-card__foot" style="padding:10px 16px;font-size:11px;color:var(--color-text-secondary);">
    数据时间戳：初始快照 · 指标为 Mock 投影
  </footer>
</section>
