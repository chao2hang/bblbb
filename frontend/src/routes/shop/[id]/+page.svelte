<!-- M07-UI-03：商品详情 + 购买确认。
  - 显示准确价格、当前余额、扣除后余额、不可退款说明、库存/等级门槛/限购/
    有效期/展示槽位。
  - 购买表单：原生 form[method=POST]（无 JS 可用），use:enhance 渐进增强；
    隐藏幂等键保持稳定（重试不重复扣款）。
  - 失败恢复：余额不足/商品换版/售罄/限购/429 各态给出指引与重试。
-->
<script lang="ts">
  import { enhance } from '$app/forms';
  import { newClientRequestId, productKindLabel } from '$lib/api/client';
  import { slotLabel } from '$lib/components/wardrobe/tokens';
  import Icon from '$lib/components/ui/Icon.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import type { ShopActionData, ShopProductPageData } from './+page.server';
  import type { ShopProduct } from '$lib/api/types';

  let { data, form }: { data: ShopProductPageData; form?: ShopActionData | null } = $props();

  const product = $derived(data.product);
  const balance = $derived(data.balance);
  const level = $derived(data.level);
  const ownedCount = $derived(data.ownedCount);
  const error = $derived(data.error);

  let quantity = $state(1);
  let busy = $state(false);

  const idempotencyKey = $state(newClientRequestId());
  const locked = $derived(Boolean(product && typeof level === 'number' && product.required_level > level));
  const soldOut = $derived(Boolean(product && typeof product.stock_remaining === 'number' && product.stock_remaining === 0));
  const limitReached = $derived(Boolean(product && product.quantity_limit > 0 && ownedCount >= product.quantity_limit));
  const affordable = $derived(Boolean(product && balance && balance.amount >= product.unit_price * quantity));

  const totalPrice = $derived((product?.unit_price ?? 0) * quantity);
  const balanceAfter = $derived((balance?.amount ?? 0) - totalPrice);

  function iconFor(p: ShopProduct): string {
    const token = p.icon_token ?? '';
    const known = new Set(['shopping-bag', 'star', 'sparkles', 'heart', 'award', 'palette', 'trophy', 'wand-2']);
    return known.has(token) ? token : 'shopping-bag';
  }
</script>

<svelte:head>
  <title>{product ? `${product.title} — 商城` : '商品 — 商城'} · BBLBB</title>
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <a href="/shop" class="breadcrumb-link">积分商城</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">{product?.title ?? '商品'}</span>
  </nav>

  {#if error}
    <p class="input-hint is-error" role="alert">{error}</p>
  {:else if !product}
    <p class="input-hint is-error" role="alert">商品不存在或已下架</p>
  {:else}
    <div class="card">
      <div class="card-body" style="display:grid;grid-template-columns:repeat(auto-fit,minmax(280px,1fr));gap:var(--space-5);">
        <div class="shop-detail-icon" style="display:flex;align-items:center;justify-content:center;background:var(--color-bg-subtle,#f6f8fa);border-radius:var(--radius-md,8px);min-height:200px;">
          <Icon name={iconFor(product)} size={72} />
        </div>
        <div style="display:flex;flex-direction:column;gap:var(--space-3);">
          <div style="display:flex;align-items:center;gap:var(--space-2);flex-wrap:wrap;">
            <h1 style="margin:0;font-size:var(--text-xl);">{product.title}</h1>
            <span class="badge badge-neutral">{productKindLabel(product.kind)}</span>
            {#if product.status !== 'published'}
              <span class="badge badge-warning">未在售</span>
            {/if}
          </div>

          {#if product.description_safe}
            <p>{product.description_safe}</p>
          {/if}

          <dl class="shop-meta" style="display:grid;grid-template-columns:auto 1fr;gap:var(--space-2) var(--space-4);margin:0;">
            <dt>价格</dt><dd><strong class="shop-price">{product.unit_price} {product.currency.toUpperCase()}</strong></dd>
            <dt>库存</dt>
            <dd>
              {#if soldOut}
                <span class="badge badge-danger">已售罄</span>
              {:else if typeof product.stock_remaining === 'number'}
                {product.stock_remaining}
              {:else}
                不限量
              {/if}
            </dd>
            <dt>等级门槛</dt>
            <dd>{product.required_level > 1 ? `LV.${product.required_level} 起` : '无限制'}</dd>
            <dt>限购</dt>
            <dd>{product.quantity_limit > 0 ? `每人 ${product.quantity_limit} 件（已购 ${ownedCount}）` : '不限购'}</dd>
            <dt>展示槽位</dt><dd>{slotLabel(product.slot ?? '') || '—'}</dd>
            <dt>有效期</dt>
            <dd>
              {#if product.sale_end_at}
                限时销售 · {new Date(product.sale_end_at).toLocaleDateString('zh-CN')} 截止
              {:else if product.validity_seconds}
                {Math.round(product.validity_seconds / 86400) >= 1
                  ? `购买后 ${Math.round(product.validity_seconds / 86400)} 天有效`
                  : `${product.validity_seconds} 秒有效`}
              {:else}
                永久有效
              {/if}
            </dd>
          </dl>

          <div class="purchase-box" style="border:1px solid var(--color-border,#d0d7de);border-radius:var(--radius-md,8px);padding:var(--space-4);display:flex;flex-direction:column;gap:var(--space-3);">
            {#if form?.message}
              <p class="input-hint is-error" role="alert">
                {form.message}
                {#if form.code === 'product_version_changed' || form.code === 'version_conflict'}
                  ——刷新页面后重新确认商品信息。
                {/if}
              </p>
            {/if}

            {#if locked}
              <p class="input-hint is-error" role="alert">该商品需要 LV.{product.required_level}，你的当前等级是 LV.{level}。</p>
            {:else if soldOut}
              <p class="input-hint is-error" role="alert">商品已售罄。</p>
            {:else if limitReached}
              <p class="input-hint is-error" role="alert">已达到该商品购买上限。</p>
            {:else}
              <form
                method="POST"
                action="?/purchase"
                use:enhance={() => {
                  busy = true;
                  return async ({ update }) => {
                    busy = false;
                    await update();
                  };
                }}
              >
                <input type="hidden" name="client_request_id" value={idempotencyKey} />
                <input type="hidden" name="expected_product_version" value={product.version} />
                <div class="input-wrapper">
                  <label class="input-label" for="purchase-quantity">购买数量</label>
                  <input
                    id="purchase-quantity"
                    type="number"
                    name="quantity"
                    class="input-field"
                    min="1"
                    max={Math.max(1, Math.min(99, product.quantity_limit - ownedCount))}
                    value={quantity}
                    oninput={(e) => {
                      const v = Number((e.target as HTMLInputElement).value);
                      if (Number.isInteger(v) && v > 0) quantity = v;
                    }}
                  />
                </div>
                <div class="purchase-summary" style="display:grid;grid-template-columns:auto 1fr;gap:var(--space-1) var(--space-4);margin:var(--space-2) 0;">
                  <span>当前余额</span><span>{balance ? `${balance.amount} ${balance.currency.toUpperCase()}` : '—'}</span>
                  <span>本次扣除</span><span>{totalPrice} {product.currency.toUpperCase()}</span>
                  <span>购买后余额</span>
                  <strong class={affordable ? '' : 'is-danger'} style="color:{affordable ? 'inherit' : 'var(--color-danger,#cf222e)'};">
                    {balance ? `${balanceAfter} ${product.currency.toUpperCase()}` : '—'}
                  </strong>
                </div>
                <Button
                  text={affordable ? '确认购买' : '余额不足'}
                  variant="primary"
                  size="md"
                  type="submit"
                  disabled={!affordable || busy}
                />
              </form>
              <p class="input-hint">
                {product.refund_policy === 'non_refundable'
                  ? '数字装扮默认不可退款，请确认后再购买。'
                  : product.refund_policy === 'compensation_only'
                    ? '仅在权益未发放等平台异常时支持补偿。'
                    : '本商品支持退款。'}
              </p>
            {/if}
          </div>
        </div>
      </div>
    </div>
  {/if}
</div>
