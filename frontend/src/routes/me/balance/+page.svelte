<!-- M07-UI-01：个人积分页面（优化版）。
  展示今日核心指标（可用余额、社区信任等级、签到奖励）与动态收支趋势折线图，
  以及近期积分明细与 7 天收支统计。
-->
<script lang="ts">
  import { enhance } from '$app/forms';
  import { newClientRequestId } from '$lib/api/client';
  import PageTitle from '$lib/components/PageTitle.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import type { BalanceActionData, BalancePageData } from './+page.server';
  import { getCurrencyNameContext } from '$lib/site/currency-context.svelte';
  import { formatLedgerAction } from '$lib/points/format';

  let { data, form }: { data: BalancePageData; form?: BalanceActionData | null } = $props();

  const summary = $derived(data.summary);
  const trust = $derived(data.trust);
  const transactions = $derived(data.transactions ?? []);
  const error = $derived(data.error);
  const message = $derived(form?.message ?? null);
  const retryAfter = $derived(form?.retryAfterSecs ?? null);

  let idempotencyKey = $state(newClientRequestId());
  let showBalanceInfo = $state(false);

  const coinBalance = $derived(
    (summary?.balances ?? []).find((b) => b.currency === 'coin') ??
      (summary ? { currency: 'coin', amount: 0, name: '金币' } : undefined)
  );
  const currencyName = $derived(getCurrencyNameContext()?.currencyName ?? coinBalance?.name ?? '金币');

  const summaryAny = $derived(summary as unknown as Record<string, unknown> | null);
  const lvlNum = $derived.by(() => {
    if (trust?.level !== undefined) return trust.level;
    if (!summaryAny?.level) return null;
    if (typeof summaryAny.level === 'number') return summaryAny.level;
    if (typeof summaryAny.level === 'object' && summaryAny.level !== null && 'sort_order' in summaryAny.level) {
      return Number((summaryAny.level as Record<string, unknown>).sort_order);
    }
    return null;
  });
  const lvlName = $derived.by(() => {
    if (trust?.name) return trust.name;
    if (!summaryAny) return null;
    if (typeof summaryAny.level === 'object' && summaryAny.level !== null && 'name' in summaryAny.level) {
      return String((summaryAny.level as Record<string, unknown>).name ?? '');
    }
    if (typeof summaryAny.level_name === 'string') return summaryAny.level_name;
    return null;
  });

  const trustLevel = $derived(lvlNum ?? 1);
  const trustName = $derived(lvlName ?? '基本用户');
  const trustNextLevel = $derived(trust?.next_level ?? null);

  const todayEarned = $derived(summary?.today_earned ?? form?.todayEarned ?? []);
  const todayEarnedTotal = $derived(todayEarned.reduce((sum, e) => sum + (e.amount || 0), 0));
  const streak = $derived(summary?.streak_days ?? form?.streakDays ?? 0);
  const checkedIn = $derived(summary?.checked_in_today ?? form?.ok ?? false);
  const checkInEnabled = $derived(summary?.check_in_enabled !== false);
  const autoCheckInEnabled = $derived(summary?.auto_check_in_enabled !== false);

  // 格式化当前时间为 HH:mm
  const currentTimeStr = $derived.by(() => {
    const d = new Date();
    const pad = (n: number) => n.toString().padStart(2, '0');
    return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
  });

  // 7 天日期与收支统计
  interface DayStat {
    date: string; // e.g. "9/13"
    fullDate: string;
    income: number;
    expense: number;
    total: number;
    incomePct: number;
    expensePct: number;
  }

  const past7Days = $derived.by<DayStat[]>(() => {
    const list: DayStat[] = [];
    const now = new Date();

    for (let i = 6; i >= 0; i--) {
      const target = new Date(now.getTime() - i * 86400000);
      const m = target.getMonth() + 1;
      const d = target.getDate();
      const dateStr = `${m}/${d}`;
      const fullDate = `${target.getFullYear()}-${String(m).padStart(2, '0')}-${String(d).padStart(2, '0')}`;

      let inc = 0;
      let exp = 0;

      for (const tx of transactions) {
        const txTimestamp = typeof tx.created_at === 'number' ? tx.created_at : Number(tx.created_at);
        const txDate = new Date(txTimestamp > 1e11 ? txTimestamp : txTimestamp * 1000);
        if (
          txDate.getFullYear() === target.getFullYear() &&
          txDate.getMonth() === target.getMonth() &&
          txDate.getDate() === target.getDate()
        ) {
          if (tx.amount > 0) inc += tx.amount;
          else if (tx.amount < 0) exp += Math.abs(tx.amount);
        }
      }

      // 今天入账打卡
      if (i === 0 && inc === 0 && todayEarnedTotal > 0) {
        inc += todayEarnedTotal;
      }

      list.push({
        date: dateStr,
        fullDate,
        income: inc,
        expense: exp,
        total: 0,
        incomePct: 0,
        expensePct: 0
      });
    }

    const maxIncome = Math.max(...list.map((d) => d.income), 10);
    const maxExpense = Math.max(...list.map((d) => d.expense), 10);

    let runningTotal = coinBalance?.amount ?? 0;
    for (let i = list.length - 1; i >= 0; i--) {
      list[i].total = Math.max(0, runningTotal);
      list[i].incomePct = list[i].income > 0 ? Math.min(100, Math.round((list[i].income / maxIncome) * 100)) : 0;
      list[i].expensePct = list[i].expense > 0 ? Math.min(100, Math.round((list[i].expense / maxExpense) * 100)) : 0;
      runningTotal -= (list[i].income - list[i].expense);
    }

    return list;
  });

  const total7dIncome = $derived(past7Days.reduce((acc, d) => acc + d.income, 0));
  const total7dExpense = $derived(past7Days.reduce((acc, d) => acc + d.expense, 0));

  // 活动列表（真实交易流水）
  interface ActivityItem {
    id: string;
    title: string;
    amountText: string;
    isPositive: boolean;
    time?: string;
    balanceAfter?: number;
  }

  function formatTxKind(kind: string): string {
    switch (kind) {
      case 'checkin':
        return '每日签到打卡';
      case 'admin_adjust':
        return '管理员调整';
      case 'shop_purchase':
        return '商城道具购买';
      case 'marketplace_order':
        return '应用市场消费';
      case 'content_unlock':
        return '付费内容解锁';
      case 'attachment_download':
        return '附件资源下载';
      case 'reward':
        return '激励奖励';
      default:
        return '积分变动';
    }
  }

  function formatTxTime(ts: number): string {
    if (!ts) return '';
    const num = typeof ts === 'number' ? ts : Number(ts);
    const d = new Date(num > 1e11 ? num : num * 1000);
    const pad = (n: number) => n.toString().padStart(2, '0');
    return `${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
  }

  const activities = $derived.by<ActivityItem[]>(() => {
    if (transactions.length > 0) {
      return transactions.slice(0, 30).map((t) => ({
        id: t.id,
        title: formatLedgerAction(t.memo, t.kind, t.amount),
        amountText: `${t.amount >= 0 ? '+' : ''}${t.amount} ${currencyName}`,
        isPositive: t.amount >= 0,
        time: formatTxTime(t.created_at),
        balanceAfter: t.balance_after
      }));
    }
    if (todayEarned.length > 0) {
      return todayEarned.map((e, idx) => ({
        id: `earned-${idx}`,
        title: '每日签到奖励',
        amountText: `+${e.amount} ${currencyName}`,
        isPositive: true,
        time: currentTimeStr
      }));
    }
    return [];
  });

  // SVG 趋势图坐标生成
  const chartWidth = 720;
  const startX = 40;
  const baselineY = 188;

  function buildPoints(values: number[], maxVal: number, minBoundY: number, maxBoundY: number) {
    const effectiveMax = Math.max(maxVal, 1);
    const n = values.length;
    return values.map((v, i) => {
      const x = Math.round(startX + (i * (chartWidth / (n - 1 || 1))));
      const ratio = Math.max(0, Math.min(1, v / effectiveMax));
      const y = Math.round(maxBoundY - ratio * (maxBoundY - minBoundY));
      return { x, y };
    });
  }

  function smoothPath(pts: { x: number; y: number }[]) {
    if (pts.length === 0) return '';
    if (pts.length === 1) return `M${pts[0].x},${pts[0].y}`;
    let d = `M${pts[0].x},${pts[0].y}`;
    for (let i = 0; i < pts.length - 1; i++) {
      const p0 = pts[Math.max(0, i - 1)];
      const p1 = pts[i];
      const p2 = pts[i + 1];
      const p3 = pts[Math.min(pts.length - 1, i + 2)];
      const cp1x = p1.x + (p2.x - p0.x) / 6;
      const cp1y = p1.y + (p2.y - p0.y) / 6;
      const cp2x = p2.x - (p3.x - p1.x) / 6;
      const cp2y = p2.y - (p3.y - p1.y) / 6;
      d += ` C${cp1x.toFixed(1)},${cp1y.toFixed(1)} ${cp2x.toFixed(1)},${cp2y.toFixed(1)} ${p2.x},${p2.y}`;
    }
    return d;
  }

  const chartData = $derived.by(() => {
    const totals = past7Days.map((d) => d.total);
    const incomes = past7Days.map((d) => d.income);
    const expenses = past7Days.map((d) => d.expense);

    const maxTotal = Math.max(...totals, 100);
    const maxFlow = Math.max(...incomes, ...expenses, 20);

    // 总额在上方区间（36 ~ 110）
    const totalPts = buildPoints(totals, maxTotal, 36, 110);
    // 收入与支出在下方区间（120 ~ 188）
    const incomePts = buildPoints(incomes, maxFlow * 1.5, 120, baselineY);
    const expensePts = buildPoints(expenses, maxFlow * 1.5, 120, baselineY);

    const totalCurve = smoothPath(totalPts);
    const totalArea = `${totalCurve} L${totalPts[totalPts.length - 1]?.x ?? 760},${baselineY} L${startX},${baselineY} Z`;

    const incomeCurve = smoothPath(incomePts);
    const incomeArea = `${incomeCurve} L${incomePts[incomePts.length - 1]?.x ?? 760},${baselineY} L${startX},${baselineY} Z`;

    const expenseCurve = smoothPath(expensePts);
    const expenseArea = `${expenseCurve} L${expensePts[expensePts.length - 1]?.x ?? 760},${baselineY} L${startX},${baselineY} Z`;

    return { totalPts, incomePts, expensePts, totalCurve, totalArea, incomeCurve, incomeArea, expenseCurve, expenseArea };
  });

  // 鼠标交互 Tooltip
  let hoverIndex = $state<number | null>(null);
  let hoverX = $state(0);
  let hoverY = $state(0);

  function handleMouseMove(event: MouseEvent) {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    const mouseX = event.clientX - rect.left;
    const ratio = Math.max(0, Math.min(1, (mouseX - startX * (rect.width / 800)) / (chartWidth * (rect.width / 800))));
    const idx = Math.min(past7Days.length - 1, Math.max(0, Math.round(ratio * (past7Days.length - 1))));
    hoverIndex = idx;
    hoverX = Math.round(startX + idx * (chartWidth / (past7Days.length - 1)));
    hoverY = chartData.totalPts[idx]?.y ?? 80;
  }

  function handleMouseLeave() {
    hoverIndex = null;
  }
</script>

<PageTitle title="我的积分" />

<div class="container page-content" id="page-balance">
  {#if error}
    <p class="input-hint is-error" role="alert">{error}</p>
  {/if}
  {#if message}
    <p class="input-hint {form?.ok ? 'is-success' : 'is-error'}" style={form?.ok ? 'color:var(--color-success);font-weight:500;' : ''} role="alert">{message}</p>
  {/if}

  {#if summary}
    <div class="linuxdo-credits credits-page py-6 space-y-10">
      <!-- 今天与积分趋势 -->
      <section class="credits-section credits-primary-section" aria-labelledby="balance-today-title">
        <div class="credits-primary-grid flex flex-col md:grid md:grid-cols-3 gap-8 md:gap-12">
          <!-- 积分趋势图表 -->
          <div class="credits-chart-column md:col-span-2 order-1 md:order-none">
            <div class="credits-chart-header flex items-center justify-between mb-3">
              <h3 class="text-sm text-muted-foreground font-medium">收支趋势</h3>
              <div class="flex items-center gap-4 text-xs font-normal text-muted-foreground">
                <span class="inline-flex items-center gap-1.5"><span class="w-2.5 h-2.5 rounded-full" style="background:var(--color-total);"></span>总积分</span>
                <span class="inline-flex items-center gap-1.5"><span class="w-2.5 h-2.5 rounded-full" style="background:var(--color-income);"></span>收入</span>
                <span class="inline-flex items-center gap-1.5"><span class="w-2.5 h-2.5 rounded-full" style="background:var(--color-expense);"></span>支出</span>
              </div>
            </div>

            <div
              data-slot="chart"
              data-chart="chart-_r_35_"
              class="flex aspect-video justify-center text-xs w-full h-[240px] font-medium"
              role="region"
              aria-label="积分趋势图"
              onmousemove={handleMouseMove}
              onmouseleave={handleMouseLeave}
            >
              <style>
                [data-chart="chart-_r_35_"] {
                  --color-total: var(--color-brand);
                  --color-income: var(--color-success);
                  --color-expense: var(--color-danger);
                }
                .dark [data-chart="chart-_r_35_"] {
                  --color-total: var(--color-brand);
                  --color-income: var(--color-success);
                  --color-expense: var(--color-danger);
                }
              </style>

              <div class="recharts-responsive-container" style="width: 100%; height: 100%; min-width: 0px;">
                <div class="recharts-wrapper chart-container" style="position: relative; cursor: default; width: 100%; height: 100%; max-height: 240px;">
                  <svg class="recharts-surface" width="100%" height="240" viewBox="0 0 800 240" preserveAspectRatio="none">
                    <title>积分趋势</title>
                    <desc>最近 7 天总积分、收入与支出趋势折线与渐变区域</desc>
                    <defs>
                      <clipPath id="recharts8-clip">
                        <rect x="0" y="0" height="200" width="800"></rect>
                      </clipPath>
                      <linearGradient id="colorTotal" x1="0" y1="0" x2="0" y2="1">
                        <stop offset="5%" stop-color="var(--color-total)" stop-opacity="0.25"></stop>
                        <stop offset="95%" stop-color="var(--color-total)" stop-opacity="0.01"></stop>
                      </linearGradient>
                      <linearGradient id="colorIncome" x1="0" y1="0" x2="0" y2="1">
                        <stop offset="5%" stop-color="var(--color-income)" stop-opacity="0.2"></stop>
                        <stop offset="95%" stop-color="var(--color-income)" stop-opacity="0"></stop>
                      </linearGradient>
                      <linearGradient id="colorExpense" x1="0" y1="0" x2="0" y2="1">
                        <stop offset="5%" stop-color="var(--color-expense)" stop-opacity="0.2"></stop>
                        <stop offset="95%" stop-color="var(--color-expense)" stop-opacity="0"></stop>
                      </linearGradient>
                    </defs>

                    <!-- 背景参考栅格网格 -->
                    <g class="recharts-cartesian-grid" opacity="0.3">
                      <line x1={startX} y1="40" x2={startX + chartWidth} y2="40" stroke="var(--color-border)" stroke-dasharray="3 3"></line>
                      <line x1={startX} y1="88" x2={startX + chartWidth} y2="88" stroke="var(--color-border)" stroke-dasharray="3 3"></line>
                      <line x1={startX} y1="138" x2={startX + chartWidth} y2="138" stroke="var(--color-border)" stroke-dasharray="3 3"></line>
                      <line x1={startX} y1={baselineY} x2={startX + chartWidth} y2={baselineY} stroke="var(--color-border)"></line>
                    </g>

                    <!-- 总积分 Area -->
                    <g class="recharts-layer recharts-area" clip-path="url(#recharts8-clip)">
                      <path fill="url(#colorTotal)" stroke="none" class="recharts-curve recharts-area-area" d={chartData.totalArea}></path>
                      <path fill="none" stroke="var(--color-total)" stroke-width="2.5" class="recharts-curve recharts-area-curve" d={chartData.totalCurve}></path>
                      {#each chartData.totalPts as pt}
                        <circle cx={pt.x} cy={pt.y} r="3" fill="var(--color-bg-card)" stroke="var(--color-total)" stroke-width="2"></circle>
                      {/each}
                    </g>

                    <!-- 收入 Area -->
                    <g class="recharts-layer recharts-area" clip-path="url(#recharts8-clip)">
                      <path fill="url(#colorIncome)" stroke="none" class="recharts-curve recharts-area-area" d={chartData.incomeArea}></path>
                      <path fill="none" stroke="var(--color-income)" stroke-width="1.8" stroke-dasharray="4 4" class="recharts-curve recharts-area-curve" d={chartData.incomeCurve}></path>
                    </g>

                    <!-- 支出 Area -->
                    <g class="recharts-layer recharts-area" clip-path="url(#recharts8-clip)">
                      <path fill="url(#colorExpense)" stroke="none" class="recharts-curve recharts-area-area" d={chartData.expenseArea}></path>
                      <path fill="none" stroke="var(--color-expense)" stroke-width="1.8" stroke-dasharray="4 4" class="recharts-curve recharts-area-curve" d={chartData.expenseCurve}></path>
                    </g>

                    <!-- X 轴日期刻度 -->
                    <g class="recharts-x-axis" opacity="0.85">
                      {#each past7Days as day, i}
                        <text
                          x={startX + i * (chartWidth / (past7Days.length - 1))}
                          y={baselineY + 22}
                          text-anchor="middle"
                          font-size="11"
                          fill="var(--color-text-secondary)"
                          font-family="inherit"
                        >{day.date}</text>
                      {/each}
                    </g>

                    <!-- Hover 指针线 -->
                    {#if hoverIndex !== null}
                      <line x1={hoverX} y1="20" x2={hoverX} y2={baselineY} stroke="var(--color-border-strong)" stroke-width="1.2" stroke-dasharray="3 3"></line>
                      <circle cx={hoverX} cy={hoverY} r="5" fill="var(--color-total)" stroke="var(--color-bg-card)" stroke-width="2.5"></circle>
                    {/if}
                  </svg>

                  <!-- 浮动 Tooltip -->
                  {#if hoverIndex !== null && past7Days[hoverIndex]}
                    <div
                      class="recharts-tooltip-wrapper"
                      style="pointer-events: none; position: absolute; top: {Math.max(8, hoverY - 68)}px; left: {Math.min(620, Math.max(10, hoverX - 70))}px; z-index: 10;"
                    >
                      <div class="bg-background border rounded-lg shadow-lg p-2.5 text-xs space-y-1" style="min-width: 140px; border-color: var(--color-border);">
                        <div class="font-semibold text-muted-foreground border-b pb-1 mb-1">{past7Days[hoverIndex].fullDate}</div>
                        <div class="flex justify-between items-center text-blue-600">
                          <span>总额:</span>
                          <span class="font-mono font-medium">{past7Days[hoverIndex].total.toFixed(2)}</span>
                        </div>
                        <div class="flex justify-between items-center text-success">
                          <span>收入:</span>
                          <span class="font-mono font-medium">+{past7Days[hoverIndex].income.toFixed(2)}</span>
                        </div>
                        <div class="flex justify-between items-center text-red-600">
                          <span>支出:</span>
                          <span class="font-mono font-medium">-{past7Days[hoverIndex].expense.toFixed(2)}</span>
                        </div>
                      </div>
                    </div>
                  {/if}
                </div>
              </div>
            </div>
          </div>

          <!-- 右侧三大核心指标与签到操作 -->
          <aside class="credits-metrics-column md:col-span-1 order-2 md:order-none flex flex-col divide-y divide-border/70 md:divide-y-0 md:pt-px" aria-label="账户概览">
            <!-- 1. 可用 B 币 / COIN -->
            <div class="balance-metric-item">
              <div class="balance-metric-top">
                <span class="balance-metric-label">可用{currencyName}</span>
                <a href="/shop" class="balance-metric-link">去商城 &rarr;</a>
              </div>
              <div class="balance-metric-main">
                <span data-slot="counting-number" class="balance-metric-value">{coinBalance?.amount ?? 0}</span>
                <span class="balance-metric-unit">{currencyName}</span>
              </div>
            </div>

            <!-- 2. 社区信任等级 -->
            <div class="balance-metric-item">
              <div class="balance-metric-top">
                <span class="balance-metric-label">社区信任等级</span>
                <a href="/me/level" class="balance-metric-link" title="行为信任标准体系">信任规则 &rarr;</a>
              </div>
              <div class="balance-metric-main">
                <span class="balance-metric-value text-blue-500">TL{trustLevel}</span>
                <span class="balance-metric-subname">{trustName}</span>
              </div>
              <div class="balance-metric-subtext">
                {#if trustNextLevel}
                  <span>下一等级：TL{trustNextLevel.level} {trustNextLevel.name}</span>
                {:else}
                  <span>行为信任评定状态</span>
                {/if}
                <a href="/me/level" class="balance-metric-link">要求 &rarr;</a>
              </div>
            </div>

            <!-- 3. 今日签到奖励与签到 -->
            <div class="balance-metric-item last:border-b-0">
              <div class="balance-metric-top">
                <span class="balance-metric-label">今日签到奖励</span>
                {#if checkInEnabled}
                  <span class="balance-metric-extra">连续 {streak} 天</span>
                {/if}
              </div>
              <div class="balance-metric-main justify-between">
                <div class="flex items-baseline gap-1">
                  <span data-slot="counting-number" class="balance-metric-value {todayEarnedTotal > 0 ? 'text-green-500' : ''}">
                    {todayEarnedTotal > 0 ? `+${todayEarnedTotal}` : '0'}
                  </span>
                  <span class="balance-metric-unit">{currencyName}</span>
                </div>

                <form
                  method="POST"
                  action="?/visit"
                  use:enhance={() => {
                    return async ({ update }) => {
                      idempotencyKey = newClientRequestId();
                      await update();
                    };
                  }}
                  class="shrink-0 flex items-center"
                >
                  <input type="hidden" name="client_request_id" value={idempotencyKey} />
                  {#if !checkInEnabled}
                    <button
                      type="button"
                      disabled
                      class="badge badge-neutral text-xs cursor-not-allowed"
                    >
                      未开启
                    </button>
                  {:else if checkedIn}
                    <button
                      type="button"
                      disabled
                      class="balance-checkin-btn is-done"
                    >
                      <span class="balance-checkin-dot"></span>
                      <span>今日已签到</span>
                    </button>
                  {:else}
                    <div class="flex items-center gap-2">
                      <span class="balance-checkin-status">
                        未签到
                      </span>
                      <button
                        type="submit"
                        class="balance-checkin-btn is-action"
                      >
                        <span>立即签到</span>
                      </button>
                    </div>
                  {/if}
                </form>
              </div>

              {#if retryAfter}
                <p class="input-hint is-error text-xs mt-1" role="alert">请约 {retryAfter} 秒后再试。</p>
              {/if}
            </div>
          </aside>
        </div>
      </section>

      <!-- 近期概览：真实积分明细与 7 天收支统计 -->
      <section class="credits-section credits-overview-section" aria-labelledby="balance-overview-title">
        <h1 id="balance-overview-title" class="credits-section-title text-2xl font-semibold border-b pb-2">近期概览</h1>
        <div class="credits-overview-surface bg-muted rounded-lg p-2.5 mt-3">
          <div class="credits-overview-grid grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-2.5">
            <!-- 卡片 1：积分明细 -->
            <div data-slot="card" class="text-card-foreground gap-6 py-4 bg-background border shadow-none rounded-lg min-h-[240px] flex flex-col h-full">
              <div data-slot="card-header" class="grid auto-rows-min grid-rows-[auto_auto] items-start gap-2 px-4 pb-2">
                <div class="flex items-center justify-between">
                  <div class="flex items-center gap-2.5 h-6">
                    <div data-slot="card-title" class="text-sm font-semibold">积分明细</div>
                    <span class="text-xs font-semibold px-2 py-0.5 rounded-full bg-muted text-muted-foreground">
                      <span data-slot="counting-number">{activities.length}</span>
                    </span>
                  </div>
                </div>
              </div>
              <div data-slot="card-content" class="px-4 relative flex-1">
                <div data-slot="scroll-area" class="relative">
                  <div data-slot="scroll-area-viewport" class="size-full">
                    {#if activities.length === 0}
                      <div class="empty-placeholder py-10 flex flex-col items-center justify-center text-center">
                        <svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" class="text-muted-foreground opacity-50" aria-hidden="true">
                          <circle cx="12" cy="12" r="10"></circle>
                          <path d="M12 6v6l4 2"></path>
                        </svg>
                        <p class="text-xs text-muted-foreground mt-2">暂无积分变动记录</p>
                      </div>
                    {:else}
                      <div class="credits-activity-list space-y-1.5 max-h-[280px] overflow-y-auto pr-1">
                        {#each activities as item}
                          <div class="flex items-center justify-between py-1.5 px-2.5 rounded-md bg-muted/40 hover:bg-muted/70 transition-colors">
                            <div class="flex-1 min-w-0">
                              <p class="text-xs font-medium truncate leading-tight text-foreground">{item.title}</p>
                              <div class="flex items-center gap-2 mt-0.5 text-[10px] text-muted-foreground leading-tight">
                                {#if item.time}
                                  <span>{item.time}</span>
                                {/if}
                                {#if item.balanceAfter !== undefined}
                                  <span>余额: {item.balanceAfter}</span>
                                {/if}
                              </div>
                            </div>
                            <span class="font-mono text-xs font-semibold ml-2 shrink-0 {item.isPositive ? 'text-green-700 dark:text-green-300' : 'text-red-600 dark:text-red-400'}">
                              {item.amountText}
                            </span>
                          </div>
                        {/each}
                      </div>
                    {/if}
                  </div>
                </div>
              </div>
              <div data-slot="card-footer" class="flex items-center px-4 border-t h-9">
                <div class="flex items-center justify-between text-xs text-muted-foreground w-full">
                  <span>更新时间：{currentTimeStr}</span>
                  {#if transactions.length > 0}
                    <span>共 {transactions.length} 条记录</span>
                  {/if}
                </div>
              </div>
            </div>

            <!-- 卡片 2：7天收入统计 -->
            <div data-slot="card" class="text-card-foreground gap-6 py-4 bg-background border shadow-none rounded-lg min-h-[240px] flex flex-col h-full">
              <div data-slot="card-header" class="grid auto-rows-min grid-rows-[auto_auto] items-start gap-2 px-4 pb-2">
                <div class="flex items-center justify-between">
                  <div class="flex items-center gap-4 h-6">
                    <div data-slot="card-title" class="text-sm font-semibold">7天收入统计</div>
                  </div>
                </div>
                <div class="pt-0.5">
                  <div class="text-xl font-bold tracking-tight">
                    {currencyName} <span data-slot="counting-number" class="text-success font-mono">+{total7dIncome.toFixed(2)}</span>
                  </div>
                </div>
              </div>
              <div data-slot="card-content" class="px-4 relative flex-1">
                <div data-slot="scroll-area" class="relative w-full">
                  <div data-slot="scroll-area-viewport" class="size-full">
                    <div class="credits-stat-list space-y-1.5 pr-2">
                      {#each past7Days as day}
                        <div class="space-y-1">
                          <div class="flex items-center justify-between">
                            <span class="text-[11px] text-muted-foreground">{day.date}</span>
                            <span class="text-[11px] text-green-700 dark:text-green-300 font-semibold">+{day.income.toFixed(2)}</span>
                          </div>
                          <div class="bg-muted rounded-full overflow-hidden h-1.5">
                            <div
                              aria-valuemax={100}
                              aria-valuemin={0}
                              aria-valuetext={`${day.date} 收入 ${day.income.toFixed(2)}`}
                              aria-label={`${day.date} 收入占七日最高值比例`}
                              role="progressbar"
                              data-slot="progress"
                              class="relative w-full overflow-hidden bg-muted h-full rounded-full"
                            >
                              <div
                                data-slot="progress-indicator"
                                class="bg-green-500/85 h-full rounded-full transition-all"
                                style="width: {day.incomePct}%;"
                              ></div>
                            </div>
                          </div>
                        </div>
                      {/each}
                    </div>
                  </div>
                </div>
              </div>
              <div data-slot="card-footer" class="flex items-center px-4 border-t h-9">
                <div class="flex items-center justify-between text-xs text-muted-foreground w-full">
                  <span>更新时间：{currentTimeStr}</span>
                </div>
              </div>
            </div>

            <!-- 卡片 3：7天支出统计 -->
            <div data-slot="card" class="text-card-foreground gap-6 py-4 bg-background border shadow-none rounded-lg min-h-[240px] flex flex-col h-full">
              <div data-slot="card-header" class="grid auto-rows-min grid-rows-[auto_auto] items-start gap-2 px-4 pb-2">
                <div class="flex items-center justify-between">
                  <div class="flex items-center gap-4 h-6">
                    <div data-slot="card-title" class="text-sm font-semibold">7天支出统计</div>
                  </div>
                </div>
                <div class="pt-0.5">
                  <div class="text-xl font-bold tracking-tight">
                    {currencyName} <span data-slot="counting-number" class="text-red-500 font-mono">-{total7dExpense.toFixed(2)}</span>
                  </div>
                </div>
              </div>
              <div data-slot="card-content" class="px-4 relative flex-1">
                <div data-slot="scroll-area" class="relative w-full">
                  <div data-slot="scroll-area-viewport" class="size-full">
                    <div class="credits-stat-list space-y-1.5 pr-2">
                      {#each past7Days as day}
                        <div class="space-y-1">
                          <div class="flex items-center justify-between">
                            <span class="text-[11px] text-muted-foreground">{day.date}</span>
                            <span class="text-[11px] text-red-500 font-semibold">-{day.expense.toFixed(2)}</span>
                          </div>
                          <div class="bg-muted rounded-full overflow-hidden h-1.5">
                            <div
                              aria-valuemax={100}
                              aria-valuemin={0}
                              aria-valuetext={`${day.date} 支出 ${day.expense.toFixed(2)}`}
                              aria-label={`${day.date} 支出占七日最高值比例`}
                              role="progressbar"
                              data-slot="progress"
                              class="relative w-full overflow-hidden bg-muted h-full rounded-full"
                            >
                              <div
                                data-slot="progress-indicator"
                                class="bg-red-500/85 h-full rounded-full transition-all"
                                style="width: {day.expensePct}%;"
                              ></div>
                            </div>
                          </div>
                        </div>
                      {/each}
                    </div>
                  </div>
                </div>
              </div>
              <div data-slot="card-footer" class="flex items-center px-4 border-t h-9">
                <div class="flex items-center justify-between text-xs text-muted-foreground w-full">
                  <span>更新时间：{currentTimeStr}</span>
                </div>
              </div>
            </div>
          </div>
        </div>
      </section>
    </div>
  {:else if !error}
    <p class="input-hint" role="status">加载中…</p>
  {/if}
</div>

<style>
  .balance-metric-item {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 14px 0;
    border-bottom: 1px solid var(--color-border);
  }

  .balance-metric-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .balance-metric-label {
    font-size: 13px;
    font-weight: 500;
    color: var(--color-text-secondary);
  }

  .balance-metric-extra {
    font-size: 11px;
    color: var(--color-text-tertiary);
  }

  .balance-metric-link {
    font-size: 12px;
    font-weight: 500;
    color: var(--color-brand);
    text-decoration: none;
    transition: opacity 0.15s ease;
  }

  .balance-metric-link:hover {
    text-decoration: underline;
  }

  .balance-metric-main {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }

  .balance-metric-value {
    font-family: var(--font-family-mono);
    font-size: 26px;
    font-weight: 700;
    line-height: 1.1;
    color: var(--color-text-primary);
    font-variant-numeric: tabular-nums;
    letter-spacing: -0.02em;
  }

  .balance-metric-unit {
    font-size: 12px;
    color: var(--color-text-secondary);
  }

  .balance-metric-subname {
    font-size: 15px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .balance-metric-subtext {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font-size: 12px;
    color: var(--color-text-tertiary);
  }

  .balance-checkin-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 12px;
    font-size: 12px;
    font-weight: 500;
    border-radius: var(--radius-sm, 4px);
    transition: all 0.15s ease;
    cursor: pointer;
  }

  .balance-checkin-btn.is-action {
    background: var(--color-brand);
    color: var(--on-brand);
    border: none;
  }

  .balance-checkin-btn.is-action:hover {
    filter: brightness(1.1);
  }

  .balance-checkin-btn.is-done {
    background: var(--color-success-soft);
    color: var(--color-success);
    border: 1px solid color-mix(in srgb, var(--color-success) 25%, transparent);
    cursor: default;
  }

  .balance-checkin-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: currentColor;
  }

  .balance-checkin-status {
    font-size: 11px;
    color: var(--color-warning);
  }
</style>
