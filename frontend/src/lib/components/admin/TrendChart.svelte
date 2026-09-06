<script lang="ts">
  // GAP-FIX（M17-GAPFIX-07·组件封装）：运营趋势柱状图——纯 CSS 柱条
  // （无图表库依赖），指标内切换（内容/活跃/审核），标签按本地时区渲染。
  // 数据形状：GET /api/v1/admin/stats/trend 的 buckets（start/end 毫秒）。
  import type { AdminStatsTrend, AdminStatsTrendBucket } from '$lib/api/types';

  let { trend }: { trend: AdminStatsTrend | null } = $props();

  type MetricKey = 'posts' | 'active_users' | 'reports';

  const METRICS: Array<{ key: MetricKey; label: string }> = [
    { key: 'posts', label: '内容' },
    { key: 'active_users', label: '活跃' },
    { key: 'reports', label: '审核' }
  ];

  let metric = $state<MetricKey>('posts');
  const metricLabel = $derived(METRICS.find((m) => m.key === metric)?.label ?? '');

  const buckets = $derived<AdminStatsTrendBucket[]>(trend?.buckets ?? []);
  const values = $derived(buckets.map((b) => b[metric]));
  const max = $derived(Math.max(1, ...values));
  const total = $derived(values.reduce((sum, v) => sum + v, 0));

  /** 桶标签：时桶显示起始整点，日及以上显示 M/D。 */
  function bucketLabel(start: number, end: number): string {
    const s = new Date(start);
    if (end - start <= 3 * 3_600_000) {
      return `${String(s.getHours()).padStart(2, '0')}:00`;
    }
    return `${s.getMonth() + 1}/${s.getDate()}`;
  }
</script>

<div class="trend-chart">
  <div class="trend-chart__tabs" role="tablist" aria-label="趋势指标">
    {#each METRICS as m (m.key)}
      <button
        type="button"
        role="tab"
        aria-selected={metric === m.key}
        class:is-active={metric === m.key}
        onclick={() => (metric = m.key)}
      >
        {m.label}
      </button>
    {/each}
  </div>

  {#if !trend || buckets.length === 0}
    <p class="trend-chart__empty">暂无趋势数据。</p>
  {:else}
    <div class="trend-chart__bars" role="img" aria-label="最近 {buckets.length} 桶{metricLabel}趋势，合计 {total}">
      {#each buckets as b, i (b.start)}
        <div class="trend-chart__col" title="{bucketLabel(b.start, b.end)} · {metricLabel} {values[i]}">
          <div class="trend-chart__bar-zone">
            <div class="trend-chart__bar" style="height:{Math.max(2, Math.round((values[i] / max) * 100))}%"></div>
          </div>
          <span class="trend-chart__label">{bucketLabel(b.start, b.end)}</span>
        </div>
      {/each}
    </div>
    <p class="trend-chart__foot">{metricLabel}合计 <b>{total}</b> · 按周期起始时间聚合</p>
  {/if}
</div>

<style>
  .trend-chart__tabs {
    display: flex;
    gap: 4px;
    margin-bottom: 12px;
  }
  .trend-chart__tabs button {
    min-height: 28px;
    padding: 0 12px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-full);
    background: transparent;
    color: var(--color-text-secondary);
    font-size: 12px;
    cursor: pointer;
  }
  .trend-chart__tabs button.is-active {
    border-color: var(--color-brand);
    color: var(--color-brand);
    background: var(--color-brand-soft);
  }
  .trend-chart__bars {
    display: flex;
    align-items: stretch;
    gap: 10px;
    height: 170px;
  }
  .trend-chart__col {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .trend-chart__bar-zone {
    flex: 1;
    display: flex;
    align-items: flex-end;
    border-bottom: 1px solid var(--color-border);
  }
  .trend-chart__bar {
    width: 100%;
    border-radius: 4px 4px 0 0;
    background: linear-gradient(180deg, var(--color-brand) -40%, var(--color-brand-soft) 90%);
    min-height: 3px;
  }
  .trend-chart__label {
    text-align: center;
    color: var(--color-text-secondary);
    font-size: 11px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .trend-chart__foot {
    margin: 10px 0 0;
    color: var(--color-text-secondary);
    font-size: 12px;
  }
  .trend-chart__foot b {
    color: var(--color-text-primary, inherit);
  }
  .trend-chart__empty {
    padding: var(--space-4);
    color: var(--color-text-secondary);
  }
</style>
