<script lang="ts">
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { getCurrencyNameContext } from '$lib/site/currency-context.svelte';
  import { formatLedgerAction } from '$lib/points/format';
  import type { AdminPointsLogsPageData } from './+page.server';

  let { data }: { data: AdminPointsLogsPageData } = $props();

  const ledger = $derived(data.ledger);
  const currencyName = $derived(getCurrencyNameContext()?.currencyName ?? '金币');

  function formatTime(ts: number): string {
    if (!ts) return '-';
    const d = new Date(ts);
    const pad = (n: number) => String(n).padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
  }

  const displayRows = $derived.by(() => {
    const raw = data.ledger?.items;
    if (Array.isArray(raw) && raw.length > 0) {
      return raw.map((r) => ({
        id: r.id,
        user: r.username,
        act: formatLedgerAction(r.memo, r.kind, r.amount),
        asset: currencyName,
        change: `${r.amount > 0 ? '+' : ''}${r.amount}`,
        time: formatTime(r.created_at)
      }));
    }
    return [];
  });

  const filters = $derived(data.ledger?.filters ?? { username: '', asset: '', kind: '', from: '', to: '' });
</script>

<svelte:head>
  <title>积分日志 — BBLBB Admin</title>
</svelte:head>

<PageHeader title="积分日志" description="查看全站积分变动与调账明细流水" />

<!-- 顶部业务分区导航 -->
<nav class="tabs" aria-label="积分功能分区" style="margin-bottom:16px;">
  <a href="/admin/points" class="tab">
    <Icon name="coins" size={15} />
    <span>用户积分</span>
  </a>
  <a href="/admin/points/logs" class="tab is-active" aria-current="page">
    <Icon name="file-text" size={15} />
    <span>积分日志</span>
  </a>
  <a href="/admin/points/rules" class="tab">
    <Icon name="list" size={15} />
    <span>积分规则</span>
  </a>
</nav>

<!-- 全站积分流水卡片 -->
<section class="app-card" style="margin-bottom:14px;">
  <header class="app-card__head" style="display:flex;align-items:center;justify-content:space-between;">
    <h2>全站积分流水</h2>
    <span class="text-secondary" style="font-size:12px;">共 {displayRows.length} 条记录</span>
  </header>
  <div class="app-card__body">
    <!-- GET 查询表单 -->
    <form method="GET" class="stack" style="gap:10px;margin-bottom:14px;">
      <div style="display:grid;grid-template-columns:repeat(auto-fit, minmax(180px, 1fr));gap:8px;">
        <label style="display:flex;flex-direction:column;gap:4px;font-size:12px;font-weight:600;">
          用户名过滤
          <input type="text" name="username" class="app-field" value={filters.username} placeholder="用户名..." />
        </label>
        <label style="display:flex;flex-direction:column;gap:4px;font-size:12px;font-weight:600;">
          资产类型
          <select name="asset" class="app-select">
            <option value="" selected={!filters.asset}>全部资产</option>
            <option value="coin" selected={filters.asset === 'coin' || filters.asset === 'b_coin'}>{currencyName}</option>
          </select>
        </label>
        <label style="display:flex;flex-direction:column;gap:4px;font-size:12px;font-weight:600;">
          流水类型
          <select name="kind" class="app-select">
            <option value="" selected={!filters.kind}>全部类型</option>
            <option value="credit" selected={filters.kind === 'credit'}>收入 (+)</option>
            <option value="debit" selected={filters.kind === 'debit'}>支出 (-)</option>
            <option value="award" selected={filters.kind === 'award'}>奖励发放</option>
            <option value="shop_purchase" selected={filters.kind === 'shop_purchase'}>商城购买</option>
            <option value="consume" selected={filters.kind === 'consume'}>消费扣减</option>
            <option value="adjust" selected={filters.kind === 'adjust'}>人工调账</option>
            <option value="reversal" selected={filters.kind === 'reversal'}>冲正退款</option>
            <option value="transfer" selected={filters.kind === 'transfer'}>积分转账</option>
            <option value="freeze" selected={filters.kind === 'freeze'}>积分冻结</option>
            <option value="unfreeze" selected={filters.kind === 'unfreeze'}>积分解冻</option>
          </select>
        </label>
        <label style="display:flex;flex-direction:column;gap:4px;font-size:12px;font-weight:600;">
          起始日期
          <input type="date" name="from" class="app-field" value={filters.from} />
        </label>
        <label style="display:flex;flex-direction:column;gap:4px;font-size:12px;font-weight:600;">
          截止日期
          <input type="date" name="to" class="app-field" value={filters.to} />
        </label>
      </div>

      <div style="display:flex;align-items:center;gap:8px;margin-top:2px;">
        <button type="submit" class="btn secondary sm" style="width:100px;">查询流水</button>
        <a href="/admin/points/logs" class="btn ghost sm">重置条件</a>
      </div>
    </form>

    {#if ledger.state === 'forbidden'}
      <div class="alert alert-danger" role="alert" style="margin-bottom:12px;padding:10px 14px;font-size:13px;">
        暂无权限查看积分流水记录（需要 points.adjust 权限）。
      </div>
    {:else if ledger.state === 'error'}
      <div class="alert alert-danger" role="alert" style="margin-bottom:12px;padding:10px 14px;font-size:13px;">
        {ledger.message || '获取积分流水失败，请稍后重试'}
      </div>
    {/if}

    <div class="app-table-wrap">
      <table class="app-table" aria-label="全站流水">
        <thead>
          <tr>
            <th>账号</th>
            <th>行为与备注</th>
            <th>资产</th>
            <th>数值变化</th>
            <th>时间</th>
          </tr>
        </thead>
        <tbody>
          {#each displayRows as row (row.id)}
            <tr>
              <td>
                <a
                  href={`/admin/points?q=${encodeURIComponent(row.user)}`}
                  class="text-link"
                  title="查看并管理该用户积分"
                  style="font-weight:600;"
                >
                  {row.user}
                </a>
              </td>
              <td><span style="font-size:13px;">{row.act}</span></td>
              <td><span class="badge badge-gray">{row.asset}</span></td>
              <td>
                <b style="color:{row.change.startsWith('+') ? 'var(--color-success)' : 'inherit'};">
                  {row.change}
                </b>
              </td>
              <td><span class="text-secondary" style="font-size:12px;">{row.time}</span></td>
            </tr>
          {/each}
          {#if displayRows.length === 0}
            <tr>
              <td colspan="5" style="text-align:center;padding:24px;color:var(--color-text-secondary);">
                暂无符合条件的流水记录
              </td>
            </tr>
          {/if}
        </tbody>
      </table>
    </div>

    {#if data.ledger?.nextCursor}
      <div style="display:flex;justify-content:flex-end;margin-top:12px;">
        <a
          class="btn secondary sm"
          href={`/admin/points/logs?after=${encodeURIComponent(data.ledger.nextCursor)}${filters.username ? `&username=${encodeURIComponent(filters.username)}` : ''}${filters.asset ? `&asset=${encodeURIComponent(filters.asset)}` : ''}${filters.kind ? `&kind=${encodeURIComponent(filters.kind)}` : ''}${filters.from ? `&from=${encodeURIComponent(filters.from)}` : ''}${filters.to ? `&to=${encodeURIComponent(filters.to)}` : ''}`}
        >
          下一页 →
        </a>
      </div>
    {/if}
  </div>
</section>
