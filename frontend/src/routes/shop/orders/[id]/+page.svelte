<!-- 订单结果：只展示服务端已提交的订单事实，并为待发放权益提供可恢复刷新。 -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { invalidate } from '$app/navigation';
  import { currencyLabel } from '$lib/api/client';
  import Icon from '$lib/components/ui/Icon.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import type { ShopOrderPageData } from './+page.server';
  import PageTitle from '$lib/components/PageTitle.svelte';

  let { data }: { data: ShopOrderPageData } = $props();

  const order = $derived(data.order);
  const error = $derived(data.error);
  const pendingEntitlement = $derived(order?.entitlement_status === 'pending' && !order.entitlement_id);
  const refunded = $derived(order?.status === 'refunded' || order?.status === 'partially_refunded');
  const completed = $derived(Boolean(order && order.status === 'succeeded' && !pendingEntitlement));
  let refreshBusy = $state(false);
  let refreshMessage = $state('');

  const currency = $derived(
    order ? currencyLabel({ id: order.currency_id, code: order.currency_code, name: order.currency_name }) : ''
  );

  function money(amount: number): string {
    if (amount === 0) return '免费';
    return currency ? `${amount} ${currency}` : `${amount}`;
  }

  function headline(): string {
    if (refunded) return order?.status === 'partially_refunded' ? '部分退款已完成' : '订单已退款';
    if (pendingEntitlement) return '订单已提交';
    return completed ? '购买成功' : '订单状态已更新';
  }

  function statusCopy(): string {
    if (refunded) return '这笔订单的资金状态已经完成处理。';
    if (pendingEntitlement) return '扣款已经完成，权益正在发放。系统会自动更新，不需要重复购买。';
    return '商品权益已经记入你的账户，可以前往衣柜装备。';
  }

  function statusIcon(): string {
    if (refunded) return 'rotate-cw';
    if (pendingEntitlement) return 'clock';
    return 'check-circle';
  }

  function statusClass(): string {
    if (refunded) return 'order-status--muted';
    if (pendingEntitlement) return 'order-status--pending';
    return 'order-status--success';
  }

  async function refreshOrder(): Promise<void> {
    refreshBusy = true;
    refreshMessage = '';
    try {
      await invalidate('app:shop-order');
      refreshMessage = '已更新订单状态';
    } catch {
      refreshMessage = '暂时无法更新，请稍后再试';
    } finally {
      refreshBusy = false;
    }
  }

  onMount(() => {
    if (!pendingEntitlement) return;
    let attempts = 0;
    const timer = window.setInterval(async () => {
      if (refreshBusy) return;
      attempts += 1;
      await invalidate('app:shop-order');
      if (attempts >= 6) window.clearInterval(timer);
    }, 2500);
    return () => window.clearInterval(timer);
  });
</script>

<PageTitle title="订单结果" />

<div class="container page-content order-page">
  {#if error}
    <div class="order-alert" role="alert">
      <Icon name="lock" size={18} />
      <div><strong>无法读取这笔订单</strong><span>{error}</span></div>
    </div>
  {:else if !order}
    <div class="order-alert" role="alert">
      <Icon name="lock" size={18} />
      <div><strong>订单不存在或无权查看</strong><span>返回商城查看你的商品。</span></div>
    </div>
  {:else}
    <nav class="order-breadcrumb" aria-label="当前位置">
      <a href="/shop">商城</a>
      <span aria-hidden="true">/</span>
      <span>订单结果</span>
    </nav>

    <section class="order-status {statusClass()}" aria-live="polite">
      <div class="order-status__icon"><Icon name={statusIcon()} size={30} /></div>
      <div class="order-status__copy">
        <span class="order-eyebrow">ORDER RESULT</span>
        <h1>{headline()}</h1>
        <p>{statusCopy()}</p>
      </div>
      <span class="order-status__number">#{order.id.slice(-8)}</span>
    </section>

    <div class="order-layout">
      <section class="order-card" aria-labelledby="order-detail-title">
        <div class="order-card__header">
          <div>
            <span class="order-eyebrow">TRANSACTION</span>
            <h2 id="order-detail-title">订单明细</h2>
          </div>
          <span class="order-state-label">{refunded ? '已处理' : pendingEntitlement ? '权益发放中' : '已完成'}</span>
        </div>

        <div class="order-product">
          <div class="order-product__mark"><Icon name="shopping-bag" size={26} /></div>
          <div>
            <strong>{order.product_title ?? '商城商品'}</strong>
            <span>{order.quantity} 件 · {money(order.unit_price)} / 件</span>
          </div>
          <strong class="order-product__total">{money(order.total_amount)}</strong>
        </div>

        <dl class="order-facts">
          <div><dt>订单号</dt><dd class="order-mono">{order.id}</dd></div>
          <div><dt>下单时间</dt><dd>{new Date(order.created_at).toLocaleString('zh-CN')}</dd></div>
          <div><dt>支付状态</dt><dd>{refunded ? '已退款' : '已扣款'}</dd></div>
          <div><dt>权益状态</dt><dd>{pendingEntitlement ? '正在发放' : order.entitlement_status === 'revoked' ? '已撤销' : '已到账'}</dd></div>
        </dl>
      </section>

      <aside class="order-next-step">
        <span class="order-eyebrow">NEXT STEP</span>
        {#if completed && order.entitlement_id}
          <h2>去衣柜装备</h2>
          <p>权益已到账。选择它，马上应用到你的昵称或个人资料。</p>
          <Button text="打开我的衣柜" icon="check-circle" variant="primary" size="lg" block href="/me/wardrobe" />
        {:else if pendingEntitlement}
          <h2>正在发放权益</h2>
          <p>这不会产生重复扣款。页面会短暂自动检查，也可以手动刷新状态。</p>
          <Button text={refreshBusy ? '正在更新…' : '刷新订单状态'} icon="rotate-cw" variant="secondary" size="md" block onclick={refreshOrder} disabled={refreshBusy} />
          {#if refreshMessage}<span class="refresh-message" role="status">{refreshMessage}</span>{/if}
        {:else}
          <h2>返回商城</h2>
          <p>继续挑选其他装扮，或查看你的积分明细。</p>
          <Button text="继续逛商城" icon="shopping-bag" variant="primary" size="lg" block href="/shop" />
        {/if}
        <div class="order-links">
          <a href="/shop">返回商城</a>
          <a href="/me/balance">查看积分明细</a>
        </div>
      </aside>
    </div>
  {/if}
</div>

<style>
  .order-page { max-width: 1000px; }
  .order-alert { display: flex; gap: var(--space-3); padding: var(--space-4); border: 1px solid var(--color-border); border-left: 3px solid var(--color-danger); color: var(--color-text-secondary); background: var(--color-bg-card); }
  .order-alert strong, .order-alert span { display: block; }
  .order-alert strong { margin-bottom: 3px; color: var(--color-text-primary); }
  .order-alert :global(.icon) { color: var(--color-danger); }

  .order-breadcrumb { display: flex; gap: var(--space-2); margin-bottom: var(--space-5); color: var(--color-text-tertiary); font-size: var(--text-xs); }
  .order-breadcrumb a { color: var(--color-text-secondary); text-decoration: none; }
  .order-breadcrumb a:hover { color: var(--color-brand); }

  .order-status { display: grid; grid-template-columns: 58px minmax(0, 1fr) auto; gap: var(--space-4); align-items: center; padding: var(--space-6); border: 1px solid var(--color-border); border-left: 4px solid var(--color-brand); background: var(--color-bg-card); }
  .order-status__icon { display: grid; width: 58px; height: 58px; place-items: center; color: var(--color-brand); background: var(--color-bg-subtle); }
  .order-status__copy h1 { margin: 4px 0 5px; color: var(--color-text-primary); font-size: clamp(26px, 4vw, 40px); line-height: 1.1; }
  .order-status__copy p { margin: 0; color: var(--color-text-secondary); line-height: 1.6; }
  .order-eyebrow { color: var(--color-brand); font-size: 11px; font-weight: 700; letter-spacing: .1em; }
  .order-status__number { align-self: start; color: var(--color-text-tertiary); font-family: var(--font-family-mono); font-size: var(--text-xs); }
  .order-status--success { border-left-color: var(--color-success); }
  .order-status--success .order-status__icon { color: var(--color-success); }
  .order-status--pending { border-left-color: var(--color-warning); }
  .order-status--pending .order-status__icon { color: var(--color-warning); }
  .order-status--muted { border-left-color: var(--color-border-strong); }
  .order-status--muted .order-status__icon { color: var(--color-text-secondary); }

  .order-layout { display: grid; grid-template-columns: minmax(0, 1.25fr) minmax(280px, .75fr); gap: var(--space-5); margin-top: var(--space-5); align-items: start; }
  .order-card, .order-next-step { border: 1px solid var(--color-border); background: var(--color-bg-card); }
  .order-card__header { display: flex; justify-content: space-between; gap: var(--space-3); align-items: end; padding: var(--space-5); border-bottom: 1px solid var(--color-border); }
  .order-card__header h2, .order-next-step h2 { margin: 5px 0 0; color: var(--color-text-primary); font-size: var(--text-lg); }
  .order-state-label { padding: 4px 7px; color: var(--color-text-secondary); background: var(--color-bg-subtle); font-size: var(--text-xs); }

  .order-product { display: grid; grid-template-columns: 48px minmax(0, 1fr) auto; gap: var(--space-3); align-items: center; padding: var(--space-5); border-bottom: 1px solid var(--color-border-muted); }
  .order-product__mark { display: grid; width: 48px; height: 48px; place-items: center; color: var(--color-brand); background: var(--color-brand-soft); border: 1px solid color-mix(in srgb, var(--color-brand) 40%, var(--color-border)); }
  .order-product strong, .order-product span { display: block; }
  .order-product strong { color: var(--color-text-primary); }
  .order-product span { margin-top: 4px; color: var(--color-text-secondary); font-size: var(--text-sm); }
  .order-product__total { font-family: var(--font-family-mono); }

  .order-facts { margin: 0; padding: var(--space-3) var(--space-5); }
  .order-facts > div { display: grid; grid-template-columns: 100px minmax(0, 1fr); gap: var(--space-3); padding: var(--space-3) 0; border-bottom: 1px solid var(--color-border-muted); }
  .order-facts > div:last-child { border-bottom: 0; }
  .order-facts dt { color: var(--color-text-tertiary); font-size: var(--text-sm); }
  .order-facts dd { margin: 0; color: var(--color-text-primary); font-size: var(--text-sm); overflow-wrap: anywhere; }
  .order-mono { font-family: var(--font-family-mono); font-size: var(--text-xs) !important; }

  .order-next-step { position: sticky; top: calc(var(--header-h) + var(--space-4)); padding: var(--space-5); }
  .order-next-step p { margin: var(--space-3) 0 var(--space-5); color: var(--color-text-secondary); font-size: var(--text-sm); line-height: 1.7; }
  .refresh-message { display: block; margin-top: var(--space-2); color: var(--color-success); font-size: var(--text-xs); text-align: center; }
  .order-links { display: flex; justify-content: space-between; gap: var(--space-3); margin-top: var(--space-5); padding-top: var(--space-4); border-top: 1px solid var(--color-border-muted); }
  .order-links a { color: var(--color-text-secondary); font-size: var(--text-xs); text-decoration: underline; text-underline-offset: 3px; }
  .order-links a:hover { color: var(--color-brand); }

  @media (max-width: 760px) {
    .order-status { grid-template-columns: 44px minmax(0, 1fr); padding: var(--space-4); }
    .order-status__icon { width: 44px; height: 44px; }
    .order-status__number { grid-column: 2; }
    .order-layout { grid-template-columns: 1fr; }
    .order-next-step { position: static; }
  }

  @media (max-width: 480px) {
    .order-product { grid-template-columns: 40px minmax(0, 1fr); padding: var(--space-4); }
    .order-product__mark { width: 40px; height: 40px; }
    .order-product__total { grid-column: 2; }
    .order-facts { padding: var(--space-3) var(--space-4); }
  }
</style>
