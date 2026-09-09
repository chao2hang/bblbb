<script lang="ts">
  // M18-ADMIN-POINTS：积分与货币管理页（对齐原型 #admin-points 账户积分卡与流水筛选）。
  import { enhance } from '$app/forms';
  import { invalidateAll } from '$app/navigation';
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import { show as showToast } from '$lib/ui/toast';
  import { toastActionResult } from '$lib/ui/action-toast';
  import type { AdminPointsPageData } from './+page.server';

  let { data, form }: { data: AdminPointsPageData; form?: { message?: string } | null } = $props();

  const ledger = $derived(data.ledger);

  let showAdjustForm = $state(false);
  let adjusting = $state(false);

  // JS 启用：动作结果走全局 Toast 浮窗（成功绿/失败红）；顶部内联横幅仅保留为
  // 无 JS 回退（SSR HTML 仍渲染，见 hasJs）。
  let hasJs = $state(false);
  $effect(() => {
    hasJs = true;
  });

  const fallbackLedger = [
    { id: '1', user: 'Chaos', act: '每日签到', asset: '经验', change: '+5', time: '刚刚' },
    { id: '2', user: 'Yuwen', act: '发布主题', asset: '经验', change: '+10', time: '10分钟前' },
    { id: '3', user: 'Nina', act: '商城消费', asset: 'B币', change: '-45', time: '1小时前' },
    { id: '4', user: 'Mark', act: '商城消费', asset: 'B币', change: '-30', time: '2小时前' },
    { id: '5', user: 'Alice', act: '下载附件', asset: 'B币', change: '-10', time: '3小时前' },
    { id: '6', user: 'Reo', act: '每日签到', asset: '经验', change: '+5', time: '5小时前' }
  ];

  const displayRows = $derived.by(() => {
    const raw = data.ledger?.items;
    if (Array.isArray(raw) && raw.length > 0) {
      return raw.map((r) => ({
        id: r.id,
        user: r.username,
        act: r.memo || (r.kind === 'credit' ? '系统入账' : '消费/扣减'),
        asset: r.currency === 'b_coin' ? 'B币' : r.currency === 'exp' ? '经验' : r.currency,
        change: `${r.amount > 0 ? '+' : ''}${r.amount}`
      }));
    }
    return fallbackLedger;
  });

  const filters = $derived(data.ledger?.filters ?? { username: '', asset: '', kind: '', from: '', to: '' });
</script>

<svelte:head>
  <title>积分与货币 — BBLBB Admin</title>
</svelte:head>

<PageHeader title="积分与货币" />

{#if form?.message && !hasJs}
  <div class="alert alert-info" role="status" style="margin-bottom:12px;padding:10px 14px;background:var(--color-bg-subtle);border-radius:var(--radius-sm);font-size:13px;">
    {form.message}
  </div>
{/if}

<!-- 卡片 1：账户积分与调整 -->
<section class="app-card" style="margin-bottom:14px;">
  <header class="app-card__head">
    <h2>账户积分管理</h2>
  </header>
  <div class="app-card__body">
    <div class="app-card" style="border:1px solid var(--color-border);padding:16px;display:flex;flex-direction:column;gap:14px;">
      <div style="display:flex;align-items:center;gap:12px;">
        <div style="width:40px;height:40px;border-radius:50%;background:#388087;color:#fff;display:grid;place-items:center;font-weight:700;font-size:16px;">
          C
        </div>
        <div>
          <strong style="font-size:16px;">管理员控制台</strong>
          <div class="text-secondary" style="font-size:13px;margin-top:2px;">
            支持手动调账（经验、B币）、发放系统奖励或扣减违规积分。
          </div>
        </div>
      </div>
      <div>
        <button
          type="button"
          class="btn primary sm"
          onclick={() => (showAdjustForm = !showAdjustForm)}
        >
          {showAdjustForm ? '收起调账表单' : '调整指定用户积分'}
        </button>
      </div>

      {#if showAdjustForm}
        <form
          method="POST"
          action="?/adjust"
          use:enhance={() => {
            adjusting = true;
            return async ({ result, update }) => {
              adjusting = false;
              // 动作结果 → 全局 Toast（成功“已成功为 x 调整 +N B_COIN”/ 失败服务端文案）；
              // 顶部横幅为无 JS 回退，失败提示同样靠 Toast，避免结果不可见。
              toastActionResult(result);
              if (result.type === 'success') {
                showAdjustForm = false;
                await update();
                await invalidateAll();
              } else {
                await update();
              }
            };
          }}
          class="stack"
          style="gap:10px;padding-top:10px;border-top:1px dashed var(--color-border);"
        >
          <div style="display:grid;grid-template-columns:repeat(auto-fit, minmax(200px, 1fr));gap:10px;">
            <label>
              <span class="field-label" style="font-size:12px;font-weight:600;margin-bottom:4px;display:block;">目标用户名</span>
              <input type="text" name="username" class="input-field" placeholder="例如：alice" required />
            </label>
            <label>
              <span class="field-label" style="font-size:12px;font-weight:600;margin-bottom:4px;display:block;">货币种类</span>
              <select name="currency" class="app-select" style="width:100%;">
                <option value="b_coin">B币 (b_coin)</option>
                <option value="exp">经验值 (exp)</option>
              </select>
            </label>
            <label>
              <span class="field-label" style="font-size:12px;font-weight:600;margin-bottom:4px;display:block;">调整数值（正增负减）</span>
              <input type="number" name="amount" class="input-field" placeholder="如 50 或 -20" required />
            </label>
          </div>
          <label>
            <span class="field-label" style="font-size:12px;font-weight:600;margin-bottom:4px;display:block;">调整原因（写审计日志，必填）</span>
            <input type="text" name="reason" class="input-field" placeholder="如：活动达人奖励发放" required />
          </label>
          <div style="display:flex;gap:8px;">
            <Button text={adjusting ? '提交中…' : '确认调整'} variant="primary" size="sm" type="submit" disabled={adjusting} />
            <button type="button" class="btn ghost sm" onclick={() => (showAdjustForm = false)}>取消</button>
          </div>
        </form>
      {/if}
    </div>
  </div>
</section>

<!-- 卡片 2：全站流水 -->
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
            <option value="b_coin" selected={filters.asset === 'b_coin'}>B币</option>
            <option value="exp" selected={filters.asset === 'exp'}>经验值</option>
          </select>
        </label>
        <label style="display:flex;flex-direction:column;gap:4px;font-size:12px;font-weight:600;">
          流水类型
          <select name="kind" class="app-select">
            <option value="" selected={!filters.kind}>全部类型</option>
            <option value="credit" selected={filters.kind === 'credit'}>收入 (+)</option>
            <option value="debit" selected={filters.kind === 'debit'}>支出 (-)</option>
          </select>
        </label>
      </div>

      <div style="display:flex;align-items:center;gap:8px;margin-top:2px;">
        <button type="submit" class="btn secondary sm" style="width:100px;">查询流水</button>
        <a href="/admin/points" class="btn ghost sm">重置条件</a>
      </div>
    </form>

    <div class="app-table-wrap">
      <table class="app-table" aria-label="全站流水">
        <thead>
          <tr>
            <th>账号</th>
            <th>行为与备注</th>
            <th>资产</th>
            <th>数值变化</th>
          </tr>
        </thead>
        <tbody>
          {#each displayRows as row (row.id)}
            <tr>
              <td><b>{row.user}</b></td>
              <td><span style="font-size:13px;">{row.act}</span></td>
              <td><span class="badge badge-gray">{row.asset}</span></td>
              <td>
                <b style="color:{row.change.startsWith('+') ? 'var(--color-success)' : 'inherit'};">
                  {row.change}
                </b>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </div>
</section>

<!-- 底层配置（折叠收纳） -->
<details class="app-card">
  <summary class="app-card__head" style="cursor:pointer;user-select:none;">
    <h2 style="display:inline-block;font-size:15px;margin:0;">积分 / 活跃配置（只读）</h2>
  </summary>
  <div class="app-card__body" style="padding-top:12px;font-size:12px;color:var(--color-text-secondary);">
    积分/活跃奖励配置由服务端账本裁决；本页只读展示配置，禁止直接修改余额或历史流水。
  </div>
</details>
