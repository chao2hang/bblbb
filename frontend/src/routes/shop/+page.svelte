<!-- M07-UI-02：商城列表——价格/库存（售罄标记）/等级门槛（未达锁定）/限购/
  有效期/图标 Token。图标只渲染内置 icon allowlist（Icon.svelte），不解释
  任意资源；icon_token 未知时回退默认购物袋图标。
-->
<script lang="ts">
  import Icon from '$lib/components/ui/Icon.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import { productKindLabel, productStatusLabel } from '$lib/api/client';
  import type { ShopProduct } from '$lib/api/types';
  import type { ShopPageData } from './+page.server';

  let { data }: { data: ShopPageData } = $props();

  const products = $derived(data.products);
  const balance = $derived(data.balance);
  const level = $derived(data.level);
  const error = $derived(data.error);

  function priceLabel(p: ShopProduct): string {
    return `${p.unit_price}`;
  }

  function isSoldOut(p: ShopProduct): boolean {
    return typeof p.stock_remaining === 'number' && p.stock_remaining === 0;
  }

  function locked(p: ShopProduct): boolean {
    return typeof level === 'number' && p.required_level > level;
  }

  function saleWindowLabel(p: ShopProduct): string {
    if (p.sale_end_at) {
      return `限时 · ${new Date(p.sale_end_at).toLocaleDateString('zh-CN')} 截止`;
    }
    if (p.validity_seconds) {
      const days = Math.round(p.validity_seconds / 86400);
      return days >= 1 ? `有效期 ${days} 天` : `有效期 ${p.validity_seconds} 秒`;
    }
    return '永久有效';
  }

  function iconFor(p: ShopProduct): string {
    const token = p.icon_token ?? '';
    const known = new Set(['shopping-bag', 'star', 'sparkles', 'heart', 'award', 'palette', 'trophy', 'wand-2']);
    return known.has(token) ? token : 'shopping-bag';
  }
</script>

<svelte:head>
  <title>积分商城 — BBLBB</title>
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">积分商城</span>
  </nav>

  {#if error}
    <p class="input-hint is-error" role="alert">{error}</p>
  {/if}

  <div class="card" style="margin-bottom:var(--space-4);">
    <div class="card-body" style="display:flex;flex-wrap:wrap;gap:var(--space-4);align-items:center;">
      <div>
        <span class="badge badge-success">我的余额</span>
        <strong style="margin-left:var(--space-2);font-size:var(--text-lg);">
          {balance ? `${balance.amount} ${balance.currency.toUpperCase()}` : '—'}
        </strong>
      </div>
      {#if typeof level === 'number'}
        <div>
          <span class="badge badge-level">LV.{level}</span>
        </div>
      {/if}
      <div style="margin-left:auto;">
        <a class="btn btn-secondary btn-sm" href="/me/wardrobe">我的衣柜</a>
        <a class="btn btn-secondary btn-sm" href="/me/balance" style="margin-left:var(--space-2);">积分明细</a>
      </div>
    </div>
  </div>

  {#if products.length === 0 && !error}
    <div class="card">
      <div class="card-body">
        <EmptyState icon="shopping-bag" title="商城暂未上架" desc="敬请期待更多装扮与道具" />
      </div>
    </div>
  {:else}
    <div class="shop-grid" style="display:grid;grid-template-columns:repeat(auto-fill,minmax(240px,1fr));gap:var(--space-4);">
      {#each products as product (product.id)}
        {@const out = isSoldOut(product)}
        {@const lockedP = locked(product)}
        <a
          class="card shop-card"
          href={lockedP || out ? undefined : `/shop/${product.id}`}
          style="text-decoration:none;color:inherit;display:flex;flex-direction:column;gap:var(--space-2);{lockedP || out ? 'opacity:.65;cursor:not-allowed;' : ''}"
          aria-disabled={lockedP || out || undefined}
        >
          <div class="shop-card-icon">
            <Icon name={iconFor(product)} size={36} />
          </div>
          <div>
            <div style="display:flex;align-items:center;gap:var(--space-2);flex-wrap:wrap;">
              <strong>{product.title}</strong>
              <span class="badge badge-neutral">{productKindLabel(product.kind)}</span>
            </div>
            {#if product.description_safe}
              <p class="text-secondary" style="font-size:var(--text-sm);margin:var(--space-1) 0 0;">{product.description_safe}</p>
            {/if}
          </div>
          <div style="display:flex;align-items:center;justify-content:space-between;gap:var(--space-2);">
            <span class="shop-price">{priceLabel(product)} <span class="text-secondary">{product.currency.toUpperCase()}</span></span>
            {#if out}
              <span class="badge badge-danger">已售罄</span>
            {:else if lockedP}
              <span class="badge badge-warning">需 LV.{product.required_level}</span>
            {/if}
          </div>
          <div class="text-secondary" style="font-size:var(--text-xs);display:flex;flex-wrap:wrap;gap:var(--space-1);">
            <span>{saleWindowLabel(product)}</span>
            {#if typeof product.stock_remaining === 'number'}
              <span>· 库存 {product.stock_remaining}</span>
            {/if}
            {#if product.quantity_limit > 1}
              <span>· 限购 {product.quantity_limit} 件</span>
            {/if}
          </div>
          {#if product.status !== 'published' && product.status}
            <span class="badge badge-neutral" style="align-self:flex-start;">{productStatusLabel(product.status)}</span>
          {/if}
        </a>
      {/each}
    </div>
  {/if}
</div>
