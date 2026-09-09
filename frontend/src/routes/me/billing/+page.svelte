<script lang="ts">
  // 下载账单页（需登录）：统计卡（coin 余额/下载次数/累计支出）+ 下载流水
  // 表（时间/附件/金额/状态）+ 每行「重新下载」（sign action → toast 反馈）
  // + 安全提示卡（前端不接触存储 Secret，签名 URL 由后端签发）。
  import { enhance } from '$app/forms';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import Seo from '$lib/components/Seo.svelte';
  import Table from '$lib/components/ui/Table.svelte';
  import { show as showToast } from '$lib/ui/toast';
  import { formatRelative } from '$lib/utils';
  import type { BillingActionData, BillingPageData, BillingRow } from './+page.server';
  import { resolveSiteCopy, type SiteCopyView } from '$lib/site/copy';

  let { data, form }: { data: BillingPageData & { site?: SiteCopyView | null }; form?: BillingActionData | null } = $props();

  const summary = $derived(data.summary);
  const rows = $derived(data.rows);
  const totals = $derived(data.totals);
  const error = $derived(data.error);

  /** 行级幂等键：由行 id 确定性生成（SSR/hydration 一致；同键重放不重复扣费）。 */
  function rowKey(row: BillingRow): string {
    return `billing-sign-${row.id}`;
  }

  /** action 返回后 toast 反馈（成功/失败均提示）。 */
  $effect(() => {
    if (!form) return;
    if (form.ok) {
      showToast(form.message ?? '签名成功', 'success');
    } else if (form.message) {
      showToast(form.message, 'danger');
    }
  });

  function coinBalance(): string {
    const coin = (summary?.balances ?? []).find((b) => b.currency === 'coin');
    // 对齐原型：币种显示「B币」而非原始 key「COIN」。
    return coin ? `${coin.amount} B币` : '—';
  }

  function formatTs(ms: number): string {
    return new Date(ms).toLocaleString('zh-CN', { hour12: false });
  }
</script>

<Seo
  title="下载账单"
  description="下载授权、扣点记录与签名 URL 复用策略"
  og={{ type: 'website' }}
/>

<div class="container page-content">

  <header style="margin-bottom:var(--space-4);">
    <h1 style="display:flex;align-items:center;gap:var(--space-2);margin:0 0 var(--space-2);">
      <Icon name="download" size={24} />
      下载账单
    </h1>
    <p class="text-secondary" style="margin:0;">下载授权、扣点记录、签名 URL 和复用策略</p>
  </header>

  {#if error}
    <p class="input-hint is-error" role="alert">{error}</p>
  {/if}

  <!-- 统计卡 -->
  <div class="stats-grid stats-grid-3" style="margin-bottom:var(--space-4);">
    <div class="stat-card">
      <div class="stat-card-value">{coinBalance()}</div>
      <div class="stat-card-label">B 币余额</div>
    </div>
    <div class="stat-card">
      <div class="stat-card-value">{totals.count}</div>
      <div class="stat-card-label">下载次数</div>
    </div>
    <div class="stat-card">
      <div class="stat-card-value">{totals.spentCoin} B币</div>
      <div class="stat-card-label">累计下载支出</div>
    </div>
  </div>

  <!-- 下载流水表 -->
  <section class="card" style="margin-bottom:var(--space-4);" aria-labelledby="billing-transactions">
    <div class="card-header">
      <span class="card-title" id="billing-transactions">下载记录</span>
      {#if totals.lastAt}
        <span class="text-secondary" style="font-size:var(--text-sm);">
          最近一次：{formatRelative(totals.lastAt)}
        </span>
      {/if}
    </div>
    <div class="card-body">
      {#if rows.length === 0}
        <EmptyState
          icon="download"
          title="暂无下载记录"
          desc="在帖子页下载附件后，扣点与授权记录会显示在这里"
        />
      {:else}
        <Table
          caption="我的下载流水"
          columns={[
            { label: '时间' },
            { label: '附件' },
            { label: '金额', align: 'right' },
            { label: '状态' },
            { label: '操作' }
          ]}
        >
          {#each rows as row (row.id)}
            <tr>
              <td>{formatTs(row.created_at)}</td>
              <td>
                {#if row.attachment_name}
                  {row.attachment_name}
                {:else if row.attachment_id}
                  <span class="mono">{row.attachment_id.slice(0, 8)}…</span>
                {:else}
                  <span class="text-secondary">—</span>
                {/if}
              </td>
              <td class="table-cell-right">{row.amount} {row.currency.toUpperCase()}</td>
              <td>
                {#if row.balance_after !== null}
                  <span class="badge badge-success">已完成</span>
                  <span class="text-secondary" style="font-size:var(--text-xs);">
                    余额 {row.balance_after}
                  </span>
                {:else}
                  <span class="badge badge-success">已完成</span>
                {/if}
              </td>
              <td>
                <form
                  method="POST"
                  action="?/sign"
                  use:enhance={() => ({ update }) => update()}
                  style="display:inline;"
                >
                  <input type="hidden" name="authorization_id" value={row.authorization_id ?? ''} />
                  <input type="hidden" name="attachment_id" value={row.attachment_id ?? ''} />
                  <input type="hidden" name="client_request_id" value={rowKey(row)} />
                  <button type="submit" class="btn btn-ghost" style="font-size:var(--text-sm);">
                    <Icon name="download" size={14} />
                    重新下载
                  </button>
                </form>
              </td>
            </tr>
          {/each}
        </Table>
        <p class="text-secondary" style="font-size:var(--text-sm);margin:var(--space-2) 0 0;">
          重新下载走后端签名，不会重复扣费（有效授权只重签 URL）。
        </p>
      {/if}
    </div>
  </section>

  {#if form?.ok && form.url}
    <div class="card" style="margin-bottom:var(--space-4);" role="status">
      <div class="card-body" style="display:flex;align-items:center;gap:var(--space-3);flex-wrap:wrap;">
        <Icon name="check-circle" size={20} />
        <div style="flex:1;min-width:200px;">
          <strong>签名成功</strong>
          {#if form.expiresAt}
            <span class="text-secondary" style="font-size:var(--text-sm);">
              （有效期至 {new Date(form.expiresAt).toLocaleString('zh-CN', { hour12: false })}）
            </span>
          {/if}
        </div>
        <a class="btn btn-primary" href={form.url} rel="noopener">打开下载</a>
      </div>
    </div>
  {/if}

  <!-- 安全提示卡（照原型文案） -->
  <section class="card" aria-labelledby="billing-security">
    <div class="card-header">
      <span class="card-title" id="billing-security">安全提示</span>
    </div>
    <div class="card-body">
      <p class="text-secondary" style="margin:0;display:flex;align-items:flex-start;gap:var(--space-2);">
        <Icon name="shield-check" size={16} />
        前端永远不会接触对象存储 Secret；正式实现由后端返回一次性签名 URL，并记录下载交易。
      </p>
    </div>
  </section>
</div>
