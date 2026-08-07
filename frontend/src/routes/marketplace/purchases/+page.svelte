<!-- M12-UI-04：我的 Marketplace 购买记录。
  - 只显示本人交易；商户/金额/状态来自服务端快照；
  - 退款状态展示（processed/requested + 金额），退款操作由商户/管理员完成。
-->
<script lang="ts">
  import { enhance } from '$app/forms';
  import Icon from '$lib/components/ui/Icon.svelte';
  import type { PurchasesActionData, PurchasesPageData } from './+page.server';

  let { data, form }: { data: PurchasesPageData; form?: PurchasesActionData | null } = $props();

  const purchases = $derived(data.purchases ?? []);
  const error = $derived(data.error);

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

  function formatTs(ms: number): string {
    return new Date(ms).toLocaleString('zh-CN', { hour12: false });
  }
</script>

<svelte:head>
  <title>Marketplace 购买记录 · BBLBB</title>
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <a href="/me" class="breadcrumb-link">我的</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">Marketplace 购买记录</span>
  </nav>

  {#if error}
    <p class="input-hint is-error" role="alert">{error}</p>
  {/if}

  <div style="display:flex;align-items:center;justify-content:space-between;flex-wrap:wrap;gap:var(--space-3);">
    <h1 style="margin:0;">Marketplace 购买记录</h1>
    <form method="POST" action="?/refresh" use:enhance={() => ({ update }) => update()}>
      <button type="submit" class="btn btn-secondary">刷新</button>
    </form>
  </div>
  {#if form?.message}
    <p class="input-hint">{form.message}</p>
  {/if}

  {#if purchases.length === 0}
    <div class="card">
      <div class="card-body">
        <Icon name="shopping-bag" size={32} />
        <p>暂无 Marketplace 购买记录。只有你本人通过商户结账页确认的交易会显示在这里。</p>
      </div>
    </div>
  {:else}
    <div class="card">
      <div class="card-body">
        <ul class="purchase-list" style="list-style:none;margin:0;padding:0;display:flex;flex-direction:column;gap:var(--space-3);">
          {#each purchases as p (p.id)}
            <li class="purchase-item" style="border:1px solid var(--color-border,#d0d7de);border-radius:var(--radius-md,8px);padding:var(--space-3);display:grid;gap:var(--space-1);">
              <div style="display:flex;justify-content:space-between;flex-wrap:wrap;gap:var(--space-2);">
                <strong>{p.amount} {p.currency_id.toUpperCase()} · {formatTs(p.created_at)}</strong>
                <span class="badge {p.status === 'succeeded' ? 'badge-neutral' : 'badge-warning'}">{statusLabel(p.status)}</span>
              </div>
              <div style="display:grid;grid-template-columns:auto 1fr;gap:var(--space-1) var(--space-4);">
                <span class="input-hint">购买 ID</span><span class="mono input-hint">{p.id}</span>
                <span class="input-hint">商户订单号</span><span class="mono input-hint">{p.merchant_order_id}</span>
                <span class="input-hint">商品版本</span><span class="input-hint">v{p.offer_version} × {p.quantity}</span>
                <span class="input-hint">平台费</span><span class="input-hint">{p.fee_amount} {p.currency_id.toUpperCase()}</span>
                {#if p.refunded_amount > 0}
                  <span class="input-hint">已退款</span><span class="input-hint">{p.refunded_amount} {p.currency_id.toUpperCase()}</span>
                {/if}
              </div>
              {#if p.refunds && p.refunds.length > 0}
                <details>
                  <summary class="input-hint" style="cursor:pointer;">退款记录（{p.refunds.length}）</summary>
                  <ul style="margin:var(--space-2) 0 0;padding-left:var(--space-4);">
                    {#each p.refunds as r (r.id)}
                      <li class="input-hint">
                        {r.status === 'processed' ? '已退款' : '退款处理中'} {r.amount} {p.currency_id.toUpperCase()}
                        {#if r.status === 'requested'}（待商户资金到位后由平台处理）{/if}
                      </li>
                    {/each}
                  </ul>
                </details>
              {/if}
            </li>
          {/each}
        </ul>
        <p class="input-hint">退款只能由商户或平台管理员发起；此处仅展示退款状态。</p>
      </div>
    </div>
  {/if}
</div>
