<!-- M07-UI-02：商城列表——价格/库存（售罄标记）/等级门槛（未达锁定）/限购/
  有效期/图标 Token。图标只渲染内置 icon allowlist（Icon.svelte），不解释
  任意资源；icon_token 未知时回退默认购物袋图标。
  布局约定：卡片自带 padding；预览区固定高度居中（称号/颜色/头像框挂件
  预览高度不一，保证标题行/价格行跨卡对齐）；价格行固定卡底（margin-top:auto）；
  货币只经 currencyLabel/formatMoney 展示，绝不渲染 currency_id 原文（UUID）。
-->
<script lang="ts">
  import Icon from '$lib/components/ui/Icon.svelte';
  import CosmeticAvatar from '$lib/components/wardrobe/CosmeticAvatar.svelte';
  import CosmeticName from '$lib/components/wardrobe/CosmeticName.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import { formatMoney, productKindLabel, productStatusLabel } from '$lib/api/client';
  import { activityCoinBalance } from '$lib/api/types';
  import type { PublicPresentationTokens, ShopProduct } from '$lib/api/types';
  import type { ShopPageData } from './+page.server';
  import PageTitle from '$lib/components/PageTitle.svelte';

  let { data }: { data: ShopPageData } = $props();

  const products = $derived(data.products);
  const balance = $derived(data.balance);
  const level = $derived(data.level);
  const error = $derived(data.error);

  function priceLabel(p: ShopProduct): string {
    return formatMoney(p.unit_price, { id: p.currency_id, code: p.currency_code, name: p.currency_name }, { free: true });
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

  function visualTokens(p: ShopProduct): PublicPresentationTokens {
    const out: PublicPresentationTokens = {};
    for (const token of p.presentation_tokens ?? []) {
      if (token.startsWith('nickname.color.')) out.nickname_color = token.slice('nickname.color.'.length);
      if (token.startsWith('avatar.frame.')) out.avatar_frame = token.slice('avatar.frame.'.length);
    }
    if (p.asset_attachment_id && p.slot === 'avatar_frame') out.avatar_frame_attachment_id = p.asset_attachment_id;
    return out;
  }
</script>

  <PageTitle title="商城与积分" />

<div class="container page-content" id="page-shop">
  <h1 class="u-visually-hidden">商城与积分</h1>

  {#if error}
    <div class="shop-access-state app-notice is-danger" role="alert">
      <span class="shop-access-state__icon" aria-hidden="true"><Icon name="lock" size={18} /></span>
      <div>
        <strong>商城暂不可用</strong>
        <p>{error}</p>
        <a href="/me">查看账户状态</a>
      </div>
    </div>
  {:else}
    <div class="card" style="margin-bottom:var(--space-4);">
      <div class="card-body" style="display:flex;flex-wrap:wrap;gap:var(--space-4);align-items:center;">
        <div style="display:flex;align-items:baseline;gap:var(--space-2);">
          <span class="badge badge-success">我的余额</span>
          <strong style="font-size:var(--text-lg);">
            {balance ? formatMoney(balance.amount, { id: balance.currency, code: balance.currency }) : '—'}
          </strong>
        </div>
        {#if typeof level === 'number'}
          <div>
            <span class="badge badge-level">LV.{level}</span>
          </div>
        {/if}
        <div style="margin-left:auto;display:flex;gap:var(--space-2);flex-wrap:wrap;">
          <a class="btn btn-secondary btn-sm" href="/me/wardrobe">我的衣柜</a>
          <a class="btn btn-secondary btn-sm" href="/me/balance">积分明细</a>
          <!-- M18-MISC-03：查看账单入口（对齐原型） -->
          <a class="btn btn-secondary btn-sm" href="/me/billing">查看账单</a>
        </div>
      </div>
    </div>
  {/if}

  {#if !error && products.length === 0}
    <div class="card">
      <div class="card-body">
        <EmptyState icon="shopping-bag" title="商城暂未上架" desc="敬请期待更多装扮与道具" />
      </div>
    </div>
  {:else if !error}
    <div class="shop-grid">
      {#each products as product (product.id)}
        {@const out = isSoldOut(product)}
        {@const lockedP = locked(product)}
        <a
          class="card shop-card"
          class:is-dim={lockedP || out}
          href={lockedP || out ? undefined : `/shop/${product.id}`}
          aria-disabled={lockedP || out || undefined}
        >
          <div class="shop-card-preview">
            {#if product.slot === 'avatar_frame'}
              <CosmeticAvatar name="装扮" size="lg" presentation={visualTokens(product)} />
            {:else if product.slot === 'nickname_color'}
              <CosmeticName name="昵称预览" presentation={visualTokens(product)} />
            {:else}
              <Icon name={iconFor(product)} size={34} />
            {/if}
          </div>
          <div class="shop-card-title">
            <strong>{product.title}</strong>
            <span class="badge badge-neutral">{productKindLabel(product.kind)}</span>
          </div>
          {#if product.description_safe}
            <p class="shop-card-desc">{product.description_safe}</p>
          {/if}
          <div class="shop-card-footer">
            <span class="shop-price">{priceLabel(product)}</span>
            {#if out}
              <span class="badge badge-danger">已售罄</span>
            {:else if lockedP}
              <span class="badge badge-warning">需 LV.{product.required_level}</span>
            {/if}
          </div>
          <div class="shop-card-meta">
            <span>{saleWindowLabel(product)}</span>
            {#if typeof product.stock_remaining === 'number'}
              <span>· 库存 {product.stock_remaining}</span>
            {/if}
            {#if product.quantity_limit > 1}
              <span>· 限购 {product.quantity_limit} 件</span>
            {/if}
            {#if product.status !== 'published' && product.status}
              <span>· {productStatusLabel(product.status)}</span>
            {/if}
          </div>
        </a>
      {/each}
    </div>
  {/if}
</div>

<style>
  .shop-access-state {
    align-items: flex-start;
    gap: var(--space-3);
    margin-bottom: var(--space-4);
  }

  .shop-access-state__icon {
    display: grid;
    width: 32px;
    height: 32px;
    flex: 0 0 32px;
    place-items: center;
    border: 1px solid currentColor;
    color: var(--color-danger);
  }

  .shop-access-state strong {
    display: block;
    color: var(--color-text-primary);
    font-size: var(--text-base);
  }

  .shop-access-state p {
    margin: var(--space-1) 0 var(--space-2);
    color: var(--color-text-secondary);
  }

  .shop-access-state a {
    color: var(--color-link);
    text-decoration: underline;
    text-underline-offset: 3px;
  }

  /* 商品网格：卡片自带 padding（.card 无内边距，此前内容贴边）。 */
  .shop-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
    gap: var(--space-4);
  }

  .shop-card {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-4);
    text-decoration: none;
    color: inherit;
  }

  .shop-card.is-dim {
    opacity: 0.65;
    cursor: not-allowed;
  }

  /* 预览区固定高度：称号图标/昵称颜色预览/头像框挂件预览高度不一，
     固定后标题行与价格行跨卡对齐。 */
  .shop-card-preview {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 76px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-bg-subtle);
    overflow: hidden;
  }

  .shop-card-title {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }

  .shop-card-desc {
    margin: 0;
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
  }

  .shop-card-footer {
    margin-top: auto;
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .shop-price {
    font-size: var(--text-lg);
    font-weight: var(--weight-bold, 700);
  }

  .shop-card-meta {
    font-size: var(--text-xs);
    color: var(--color-text-secondary);
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }
</style>
