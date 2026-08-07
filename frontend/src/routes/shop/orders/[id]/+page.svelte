<!-- M07-UI-04：订单结果——成功态（entitlement 已发放）、补偿待处理态
  （entitlement_status=pending）、退款态与错误态。
-->
<script lang="ts">
  import { productKindLabel } from '$lib/api/client';
  import Button from '$lib/components/ui/Button.svelte';
  import type { ShopOrderPageData } from './+page.server';

  let { data }: { data: ShopOrderPageData } = $props();

  const order = $derived(data.order);
  const error = $derived(data.error);

  function statusBadge(status: string): string {
    switch (status) {
      case 'succeeded':
        return 'badge-success';
      case 'refunded':
        return 'badge-neutral';
      case 'partially_refunded':
        return 'badge-warning';
      default:
        return 'badge-neutral';
    }
  }

  function statusLabel(status: string): string {
    switch (status) {
      case 'succeeded':
        return '交易成功';
      case 'refunded':
        return '已退款';
      case 'partially_refunded':
        return '部分退款';
      default:
        return status;
    }
  }
</script>

<svelte:head>
  <title>订单结果 — BBLBB</title>
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <a href="/shop" class="breadcrumb-link">积分商城</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">订单结果</span>
  </nav>

  {#if error}
    <p class="input-hint is-error" role="alert">{error}</p>
  {:else if !order}
    <p class="input-hint is-error" role="alert">订单不存在或无权查看</p>
  {:else}
    <div class="card">
      <div class="card-body">
        <div style="display:flex;align-items:center;gap:var(--space-3);margin-bottom:var(--space-4);">
          <h1 style="margin:0;font-size:var(--text-xl);">{statusLabel(order.status)}</h1>
          <span class="badge {statusBadge(order.status)}">{order.status}</span>
        </div>

        {#if order.entitlement_status === 'pending'}
          <div class="input-hint" role="status" style="margin-bottom:var(--space-3);">
            权益正在发放中（平台补偿流程处理中），稍后可在“我的衣柜”查看；重复提交不会重复扣款。
          </div>
        {/if}

        <dl class="shop-meta" style="display:grid;grid-template-columns:auto 1fr;gap:var(--space-2) var(--space-4);margin:0;">
          <dt>订单号</dt><dd>{order.id}</dd>
          <dt>商品</dt><dd>{order.product_title ?? order.product_id}</dd>
          <dt>数量</dt><dd>{order.quantity}</dd>
          <dt>单价</dt><dd>{order.unit_price} {order.currency.toUpperCase()}</dd>
          <dt>实付</dt><dd><strong>{order.total_amount} {order.currency.toUpperCase()}</strong></dd>
          <dt>下单时间</dt><dd>{new Date(order.created_at).toLocaleString('zh-CN')}</dd>
        </dl>

        {#if order.entitlement_id}
          <p class="input-hint" style="margin-top:var(--space-3);">
            权益已发放，可在 <a href="/me/wardrobe">我的衣柜</a> 中装备。
          </p>
        {/if}

        <div style="display:flex;gap:var(--space-2);margin-top:var(--space-4);">
          <Button text="返回商城" variant="secondary" size="sm" href="/shop" />
          <Button text="去衣柜" variant="primary" size="sm" href="/me/wardrobe" />
        </div>
      </div>
    </div>
  {/if}
</div>
