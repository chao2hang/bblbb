<script lang="ts">
  // M18-ADMIN-POINTS：积分与货币管理页（对齐原型 #admin-points 账户积分卡与流水筛选）。
  import { enhance } from '$app/forms';
  import { invalidateAll } from '$app/navigation';
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import { show as showToast } from '$lib/ui/toast';
  import type { AdminPointsPageData } from './+page.server';

  let { data, form }: { data: AdminPointsPageData; form?: { message?: string } | null } = $props();

  const ledger = $derived(data.ledger);

  let showAdjustForm = $state(false);
  let adjusting = $state(false);

  const mockLedger = [
    { id: '1', user: 'Chaos', act: '签到', asset: '经验', change: '+5', time: '刚刚' },
    { id: '2', user: 'Yuwen', act: '发布主题', asset: '经验', change: '+10', time: '10分钟前' },
    { id: '3', user: 'Nina', act: '商城消费', asset: 'B币', change: '-45', time: '1小时前' },
    { id: '4', user: 'Mark', act: '商城消费', asset: 'B币', change: '-30', time: '2小时前' },
    { id: '5', user: 'Alice', act: '下载附件', asset: 'B币', change: '-10', time: '3小时前' },
    { id: '6', user: 'Reo', act: '每日签到', asset: '经验', change: '+5', time: '5小时前' }
  ];
</script>

<svelte:head>
  <title>积分与货币 — BBLBB Admin</title>
</svelte:head>

<PageHeader title="积分与货币" />

<!-- 卡片 1：账户积分（原型同款卡片） -->
<section class="app-card" style="margin-bottom:14px;">
  <header class="app-card__head">
    <h2>账户积分</h2>
  </header>
  <div class="app-card__body">
    <div class="app-card" style="border:1px solid var(--color-border);padding:16px;display:flex;flex-direction:column;gap:14px;">
      <div style="display:flex;align-items:center;gap:12px;">
        <div style="width:40px;height:40px;border-radius:50%;background:#388087;color:#fff;display:grid;place-items:center;font-weight:700;font-size:16px;">
          C
        </div>
        <div>
          <strong style="font-size:16px;">Chaos</strong>
          <div class="text-secondary" style="font-size:13px;margin-top:2px;">
            经验 2680 · B币 328 · 贡献 146
          </div>
        </div>
      </div>
      <div>
        <button
          type="button"
          class="btn primary sm"
          onclick={() => (showAdjustForm = !showAdjustForm)}
        >
          {showAdjustForm ? '收起表单' : '调整积分'}
        </button>
      </div>
    </div>

    {#if showAdjustForm}
      <form
        method="POST"
        action="?/adjust"
        use:enhance={() => {
          adjusting = true;
          return async ({ result, update }) => {
            adjusting = false;
            if (result.type === 'success') {
              showToast('积分调整成功', 'success');
              showAdjustForm = false;
              await update();
              await invalidateAll();
            } else {
              await update();
            }
          };
        }}
        style="display:flex;flex-direction:column;gap:10px;margin-top:14px;padding:14px;background:var(--color-bg-subtle);border-radius:var(--radius-sm);"
      >
        <input
          type="text"
          name="username"
          class="app-field"
          placeholder="目标用户名"
          required
          aria-label="目标用户名"
        />
        <div style="display:flex;gap:8px;">
          <select name="currency" class="app-select" style="width:120px;" aria-label="资产类型">
            <option value="coin">B币 (coin)</option>
            <option value="exp">经验 (exp)</option>
          </select>
          <input
            type="number"
            name="amount"
            class="app-field"
            placeholder="数额（正加负减）"
            required
            step="1"
            style="flex:1;"
            aria-label="调整数额"
          />
        </div>
        <input
          type="text"
          name="reason"
          class="app-field"
          placeholder="调整原因（审计必填）"
          required
          aria-label="调整原因"
        />
        <div>
          <Button text={adjusting ? '提交中…' : '确认调整'} variant="primary" size="sm" type="submit" disabled={adjusting} />
        </div>
      </form>
    {/if}
  </div>
</section>

<!-- 卡片 2：全站流水（原型同款紧凑筛选与表格） -->
<section class="app-card" style="margin-bottom:14px;">
  <header class="app-card__head" style="display:flex;justify-content:space-between;align-items:center;">
    <div>
      <h2 style="margin:0;">全站流水</h2>
      <span class="app-muted" style="font-size:12px;">默认展示所有账号的积分、经验与 B币变动，按时间倒序排列</span>
    </div>
    <span class="text-secondary" style="font-size:12px;">共 6 条</span>
  </header>
  <div class="app-card__body">
    <!-- 紧凑网格筛选 -->
    <div style="display:flex;flex-direction:column;gap:10px;margin-bottom:14px;">
      <div style="display:grid;grid-template-columns:repeat(2, 1fr);gap:8px;">
        <label style="display:flex;flex-direction:column;gap:4px;font-size:12px;font-weight:600;">
          账号
          <select class="app-select"><option>全部账号</option></select>
        </label>
        <label style="display:flex;flex-direction:column;gap:4px;font-size:12px;font-weight:600;">
          资产
          <select class="app-select"><option>全部资产</option></select>
        </label>
      </div>

      <div style="display:grid;grid-template-columns:repeat(2, 1fr);gap:8px;">
        <label style="display:flex;flex-direction:column;gap:4px;font-size:12px;font-weight:600;">
          类型
          <select class="app-select"><option>全部类型</option></select>
        </label>
        <label style="display:flex;flex-direction:column;gap:4px;font-size:12px;font-weight:600;">
          开始日期
          <input type="text" class="app-field" placeholder="mm/dd/yyyy" />
        </label>
      </div>

      <label style="display:flex;flex-direction:column;gap:4px;font-size:12px;font-weight:600;">
        结束日期
        <input type="text" class="app-field" placeholder="mm/dd/yyyy" />
      </label>

      <label style="display:flex;flex-direction:column;gap:4px;font-size:12px;font-weight:600;">
        关键词
        <input type="search" class="app-field" placeholder="搜索行为或来源" />
      </label>

      <div style="display:flex;align-items:center;justify-content:space-between;margin-top:2px;">
        <button type="button" class="btn secondary sm" style="width:120px;" onclick={() => showToast('已查询流水', 'info')}>查询</button>
        <button type="button" class="btn ghost sm">清除</button>
      </div>
    </div>

    <div class="app-table-wrap">
      <table class="app-table" aria-label="全站流水">
        <thead>
          <tr>
            <th>账号</th>
            <th>行为</th>
            <th>资产</th>
            <th>变化</th>
          </tr>
        </thead>
        <tbody>
          {#each mockLedger as row}
            <tr>
              <td><b>{row.user}</b></td>
              <td>{row.act}</td>
              <td>{row.asset}</td>
              <td style="font-weight:700;color:{row.change.startsWith('+') ? 'var(--color-success)' : 'inherit'};">
                {row.change}
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
