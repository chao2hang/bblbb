<!-- 商品详情与购买确认：原生 POST + 渐进增强，成功后进入可核验的订单结果页。 -->
<script lang="ts">
  import { enhance } from '$app/forms';
  import { currencyLabel, formatMoney, newClientRequestId, productKindLabel } from '$lib/api/client';
  import { projectEntitlementTokens, slotLabel } from '$lib/components/wardrobe/tokens';
  import Icon from '$lib/components/ui/Icon.svelte';
  import UserHoverCard, { type HoverCardUser } from '$lib/components/UserHoverCard.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import type { ShopActionData, ShopProductPageData } from './+page.server';
  import type { PublicPresentationTokens, ShopProduct, User } from '$lib/api/types';
  import PageTitle from '$lib/components/PageTitle.svelte';

  let { data, form }: { data: ShopProductPageData & { user?: User | null }; form?: ShopActionData | null } = $props();

  const product = $derived(data.product);
  const balance = $derived(data.balance);
  const level = $derived(data.level);
  const ownedCount = $derived(data.ownedCount);
  const error = $derived(data.error);
  const currentUser = $derived(data.user);

  let quantity = $state(1);
  let busy = $state(false);

  const idempotencyKey = $state(newClientRequestId());
  const locked = $derived(Boolean(product && typeof level === 'number' && product.required_level > level));
  const soldOut = $derived(Boolean(product && typeof product.stock_remaining === 'number' && product.stock_remaining === 0));
  const limitReached = $derived(Boolean(product && product.quantity_limit > 0 && ownedCount >= product.quantity_limit));
  const quantityMax = $derived(
    product
      ? Math.max(1, Math.min(99, product.quantity_limit > 0 ? product.quantity_limit - ownedCount : 99))
      : 1
  );
  const totalPrice = $derived((product?.unit_price ?? 0) * quantity);
  const balanceAfter = $derived((balance?.amount ?? 0) - totalPrice);
  const affordable = $derived(Boolean(product && balance && balance.amount >= totalPrice));
  const currency = $derived(
    product ? currencyLabel({ id: product.currency_id, code: product.currency_code, name: product.currency_name }) : ''
  );

  function iconFor(p: ShopProduct): string {
    const token = p.icon_token ?? '';
    const known = new Set(['shopping-bag', 'star', 'sparkles', 'heart', 'award', 'palette', 'trophy', 'wand-2']);
    return known.has(token) ? token : 'shopping-bag';
  }

  function visualTokens(p: ShopProduct): PublicPresentationTokens {
    const projected = projectEntitlementTokens(p.presentation_tokens, p.asset_attachment_id, p.slot).visual;
    for (const token of p.presentation_tokens ?? []) {
      if (token.startsWith('nickname.color.')) projected.nickname_color = token.slice('nickname.color.'.length);
      else if (token.startsWith('avatar.frame.')) projected.avatar_frame = token.slice('avatar.frame.'.length);
      else if (token.startsWith('profile.effect.')) projected.profile_effect = token.slice('profile.effect.'.length);
      else if (token.startsWith('post.effect.')) projected.post_effect = token.slice('post.effect.'.length);
      else if (token.startsWith('badge.')) {
        projected.profile_badges = [...(projected.profile_badges ?? []), token.slice('badge.'.length)];
      }
    }
    if (p.asset_attachment_id && p.slot === 'avatar_frame') projected.avatar_frame_attachment_id = p.asset_attachment_id;
    return projected;
  }

  const productVisualTokens = $derived.by(() => {
    return product ? visualTokens(product) : {};
  });

  const previewUser = $derived<HoverCardUser>({
    username: currentUser?.username ?? 'preview_user',
    display_name: currentUser?.display_name || currentUser?.username || 'BBLBB',
    level: typeof level === 'number' ? level : (currentUser?.level ?? 1),
    signature: currentUser?.signature ?? '在社区中展示此装扮',
    avatar_attachment_id: currentUser?.avatar_attachment_id ?? null,
    cover_attachment_id: currentUser?.cover_attachment_id ?? null
  });

  const basePresentation = $derived.by(() => {
    return currentUser?.presentation_tokens ?? null;
  });

  function updateQuantity(value: number): void {
    quantity = Math.min(quantityMax, Math.max(1, Math.trunc(value) || 1));
  }

  function saleDetail(p: ShopProduct): string {
    if (p.sale_end_at) return `销售至 ${new Date(p.sale_end_at).toLocaleDateString('zh-CN')}`;
    if (p.validity_seconds) {
      const days = Math.round(p.validity_seconds / 86400);
      return days >= 1 ? `购买后 ${days} 天有效` : `购买后 ${p.validity_seconds} 秒有效`;
    }
    return '永久有效';
  }
</script>

<PageTitle title={product ? `${product.title} — 商城` : '商品 — 商城'} />

<div class="container page-content shop-page">
  {#if error}
    <div class="shop-alert shop-alert--danger" role="alert">
      <Icon name="lock" size={18} />
      <div><strong>商品暂不可用</strong><span>{error}</span></div>
    </div>
  {:else if !product}
    <div class="shop-alert shop-alert--danger" role="alert">
      <Icon name="lock" size={18} />
      <div><strong>商品不存在或已下架</strong><span>返回商城查看当前在售商品。</span></div>
    </div>
  {:else}
    <nav class="shop-breadcrumb" aria-label="当前位置">
      <a href="/shop">商城</a>
      <span aria-hidden="true">/</span>
      <span>{product.title}</span>
    </nav>

    <div class="shop-checkout">
      <section class="shop-showcase" aria-label="商品预览">
        <div class="showcase-topline">
          <span class="showcase-kicker">BBLBB / 装扮工作室</span>
          <span class="showcase-index">ITEM 01</span>
        </div>
        <div class="showcase-stage">
          <span class="stage-mark" aria-hidden="true">{product.slot === 'nickname_color' ? 'N' : '✦'}</span>
          <div class="stage-card-wrap">
            <UserHoverCard
              user={previewUser}
              presentation={basePresentation}
              overrideTokens={productVisualTokens}
              preview={true}
            />
          </div>
          <span class="stage-caption">试穿效果 · {product.title}</span>
          <span class="stage-rule" aria-hidden="true"></span>
        </div>
        <div class="showcase-facts">
          <div><span>展示位置</span><strong>{slotLabel(product.slot ?? '') || '装扮'}</strong></div>
          <div><span>有效期</span><strong>{saleDetail(product)}</strong></div>
          <div><span>购买方式</span><strong>一次确认 · 即时到账</strong></div>
        </div>
      </section>

      <section class="shop-details" aria-labelledby="product-title">
        <div class="product-heading">
          <div class="product-labels">
            <span class="shop-eyebrow">商品详情</span>
            <span class="shop-tag">{productKindLabel(product.kind)}</span>
          </div>
          <h1 id="product-title">{product.title}</h1>
          {#if product.description_safe}
            <p class="product-description">{product.description_safe}</p>
          {/if}
        </div>

        <div class="price-line">
          <span class="price-label">单价</span>
          {#if product.unit_price === 0}
            <strong>免费</strong>
          {:else}
            <strong>{product.unit_price}</strong>
            {#if currency}
              <span class="price-currency">{currency}</span>
            {/if}
          {/if}
        </div>

        <dl class="product-facts">
          <div><dt>库存</dt><dd>{#if soldOut}<span class="fact-danger">已售罄</span>{:else if typeof product.stock_remaining === 'number'}{product.stock_remaining} 件{:else}不限量{/if}</dd></div>
          <div><dt>等级门槛</dt><dd>{product.required_level > 1 ? `LV.${product.required_level} 起` : '无限制'}</dd></div>
          <div><dt>个人限购</dt><dd>{product.quantity_limit > 0 ? `${product.quantity_limit} 件 · 已购 ${ownedCount}` : '不限购'}</dd></div>
          <div><dt>交付</dt><dd>购买成功后立即进入衣柜</dd></div>
        </dl>

        <div class="purchase-panel">
          <div class="purchase-panel__heading">
            <div>
              <span class="shop-eyebrow">购买信息</span>
              <h2>把它加入我的衣柜</h2>
            </div>
            {#if balance}<span class="balance-chip">余额 {formatMoney(balance.amount, { id: balance.currency, code: balance.currency })}</span>{/if}
          </div>

          {#if form?.message}
            <div class="purchase-state purchase-state--danger" role="alert">
              <Icon name="lock" size={16} />
              <span>{form.message}{#if form.code === 'product_version_changed' || form.code === 'version_conflict'} 请刷新页面后重新确认商品信息。{/if}</span>
            </div>
          {/if}

          {#if locked}
            <div class="purchase-state purchase-state--danger" role="alert">该商品需要 LV.{product.required_level}，你的当前等级是 LV.{level}。</div>
          {:else if soldOut}
            <div class="purchase-state purchase-state--danger" role="alert">这件商品已经售罄。</div>
          {:else if limitReached}
            <div class="purchase-state purchase-state--muted" role="status">已达到该商品购买上限，你已拥有这件商品。</div>
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
              <div class="purchase-controls">
                <div>
                  <label class="input-label" for="purchase-quantity">购买数量</label>
                  <div class="quantity-control">
                    <button type="button" class="quantity-button" aria-label="减少数量" onclick={() => updateQuantity(quantity - 1)} disabled={quantity <= 1}>−</button>
                    <input id="purchase-quantity" type="number" name="quantity" min="1" max={quantityMax} value={quantity} oninput={(e) => updateQuantity(Number((e.target as HTMLInputElement).value))} />
                    <button type="button" class="quantity-button" aria-label="增加数量" onclick={() => updateQuantity(quantity + 1)} disabled={quantity >= quantityMax}>+</button>
                  </div>
                </div>
                <div class="purchase-total">
                  <span>本次应付</span>
                  <strong>{formatMoney(totalPrice, { id: product.currency_id, code: product.currency_code, name: product.currency_name }, { free: true })}</strong>
                </div>
              </div>

              <div class="balance-summary">
                <div><span>当前余额</span><strong>{balance ? formatMoney(balance.amount, { id: balance.currency, code: balance.currency }) : '—'}</strong></div>
                <div><span>购买后余额</span><strong class:summary-danger={!affordable}>{balance ? formatMoney(balanceAfter, { id: product.currency_id, code: product.currency_code }) : '—'}</strong></div>
              </div>

              <Button
                text={busy ? '正在确认…' : affordable ? '确认购买' : '余额不足'}
                icon={busy ? 'clock' : 'check-circle'}
                variant="primary"
                size="lg"
                type="submit"
                block
                extraClass="shop-submit"
                disabled={!affordable || busy}
              />
            </form>
            <p class="purchase-note">
              {product.refund_policy === 'non_refundable'
                ? '数字装扮确认后不可退款。'
                : product.refund_policy === 'compensation_only'
                  ? '仅在权益未发放等平台异常时支持补偿。'
                  : '本商品支持退款。'}
              <span>订单会保留在积分明细中。</span>
            </p>
          {/if}
        </div>
      </section>
    </div>
  {/if}
</div>

<style>
  .shop-page { max-width: 1180px; }

  .shop-alert {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    padding: var(--space-4);
    border: 1px solid var(--color-border);
    background: var(--color-bg-card);
    color: var(--color-text-secondary);
  }

  .shop-alert strong,
  .shop-alert span { display: block; }
  .shop-alert strong { margin-bottom: 3px; color: var(--color-text-primary); }
  .shop-alert--danger { border-left: 3px solid var(--color-danger); }
  .shop-alert--danger :global(.icon) { color: var(--color-danger); }

  .shop-breadcrumb {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-bottom: var(--space-5);
    color: var(--color-text-tertiary);
    font-size: var(--text-xs);
  }

  .shop-breadcrumb a { color: var(--color-text-secondary); text-decoration: none; }
  .shop-breadcrumb a:hover { color: var(--color-brand); }

  .shop-checkout {
    display: grid;
    grid-template-columns: minmax(0, 1.08fr) minmax(360px, .92fr);
    gap: clamp(var(--space-6), 5vw, var(--space-12));
    align-items: start;
  }

  .shop-showcase {
    min-width: 0;
    padding: var(--space-5);
    background: var(--color-bg-inset);
    border: 1px solid var(--color-border);
  }

  .showcase-topline,
  .product-labels,
  .purchase-panel__heading,
  .price-line,
  .purchase-controls,
  .balance-summary,
  .showcase-facts,
  .shop-tag { display: flex; align-items: center; }

  .showcase-topline { justify-content: space-between; gap: var(--space-3); padding-bottom: var(--space-4); border-bottom: 1px solid var(--color-border-muted); }
  .showcase-kicker,
  .showcase-index,
  .shop-eyebrow,
  .price-label,
  .showcase-facts span,
  .purchase-total span,
  .balance-summary span { color: var(--color-text-tertiary); font-size: 11px; letter-spacing: .08em; text-transform: uppercase; }
  .showcase-index { font-family: var(--font-family-mono); }

  .showcase-stage {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    min-height: 410px;
    margin: var(--space-5) 0;
    padding: var(--space-6) var(--space-4);
    overflow: hidden;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border-muted);
  }

  .stage-mark { position: absolute; top: var(--space-4); left: var(--space-4); color: var(--color-brand); font-family: var(--font-family-mono); font-size: var(--text-sm); }
  .stage-rule { position: absolute; right: var(--space-5); bottom: var(--space-5); left: var(--space-5); height: 1px; background: var(--color-border-muted); }
  .stage-card-wrap {
    width: 100%;
    max-width: 320px;
    margin: var(--space-4) auto var(--space-3);
    z-index: 1;
    filter: drop-shadow(0 10px 24px rgba(0, 0, 0, 0.28));
  }
  .stage-card-wrap :global(.user-hover-card) {
    text-align: left;
    border-color: var(--color-border-strong);
  }
  .stage-caption {
    color: var(--color-text-secondary);
    font-size: var(--text-xs);
    letter-spacing: 0.04em;
    margin-bottom: var(--space-2);
    z-index: 1;
  }

  .showcase-facts { gap: var(--space-4); align-items: stretch; }
  .showcase-facts > div { flex: 1; min-width: 0; padding-right: var(--space-3); border-right: 1px solid var(--color-border-muted); }
  .showcase-facts > div:last-child { padding-right: 0; border-right: 0; }
  .showcase-facts strong { display: block; margin-top: 5px; color: var(--color-text-primary); font-size: var(--text-sm); font-weight: 600; }

  .shop-details { min-width: 0; padding-top: var(--space-2); }
  .product-labels { gap: var(--space-2); margin-bottom: var(--space-3); }
  .shop-eyebrow { display: block; color: var(--color-brand); font-weight: 700; }
  .shop-tag { min-height: 24px; padding: 0 var(--space-2); color: var(--color-text-secondary); background: var(--color-bg-subtle); border: 1px solid var(--color-border); font-size: var(--text-xs); }
  .product-heading h1 { margin: 0; color: var(--color-text-primary); font-size: clamp(28px, 4vw, 44px); line-height: 1.12; }
  .product-description { max-width: 46ch; margin: var(--space-3) 0 0; color: var(--color-text-secondary); font-size: var(--text-base); line-height: 1.7; }

  .price-line { gap: var(--space-2); margin: var(--space-6) 0 var(--space-5); padding-bottom: var(--space-5); border-bottom: 1px solid var(--color-border); }
  .price-line strong { color: var(--color-text-primary); font-family: var(--font-family-mono); font-size: 32px; line-height: 1; }
  .price-currency { color: var(--color-text-secondary); font-family: var(--font-family-mono); font-size: var(--text-xs); }

  .product-facts { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--space-3) var(--space-6); margin: 0 0 var(--space-6); }
  .product-facts > div { display: flex; justify-content: space-between; gap: var(--space-3); padding-bottom: var(--space-2); border-bottom: 1px solid var(--color-border-muted); }
  .product-facts dt { color: var(--color-text-tertiary); font-size: var(--text-sm); }
  .product-facts dd { margin: 0; color: var(--color-text-primary); font-size: var(--text-sm); text-align: right; }
  .fact-danger { color: var(--color-danger); font-weight: 700; }

  .purchase-panel { padding: var(--space-5); background: var(--color-bg-card); border: 1px solid var(--color-border-strong); box-shadow: var(--shadow-control); }
  .purchase-panel__heading { justify-content: space-between; gap: var(--space-3); margin-bottom: var(--space-5); align-items: flex-end; }
  .purchase-panel__heading h2 { margin: 5px 0 0; color: var(--color-text-primary); font-size: var(--text-lg); }
  .balance-chip { padding: 5px 8px; color: var(--color-text-secondary); background: var(--color-bg-subtle); font-family: var(--font-family-mono); font-size: var(--text-xs); white-space: nowrap; }

  .purchase-state { display: flex; gap: var(--space-2); margin-bottom: var(--space-4); padding: var(--space-3); font-size: var(--text-sm); line-height: 1.5; }
  .purchase-state--danger { color: var(--color-danger); background: color-mix(in srgb, var(--color-danger) 9%, var(--color-bg-card)); border-left: 2px solid var(--color-danger); }
  .purchase-state--muted { color: var(--color-text-secondary); background: var(--color-bg-subtle); border-left: 2px solid var(--color-border-strong); }

  .purchase-controls { justify-content: space-between; gap: var(--space-4); margin-bottom: var(--space-5); }
  .quantity-control { display: flex; align-items: center; width: 132px; height: 40px; border: 1px solid var(--color-border-strong); background: var(--color-bg-subtle); }
  .quantity-control input { width: 52px; height: 100%; padding: 0; color: var(--color-text-primary); background: transparent; border: 0; outline: 0; font-family: var(--font-family-mono); font-size: var(--text-base); text-align: center; }
  .quantity-button { display: grid; width: 40px; height: 100%; place-items: center; color: var(--color-text-secondary); background: transparent; border: 0; cursor: pointer; font-size: 20px; }
  .quantity-button:hover:not(:disabled) { color: var(--color-brand); background: var(--color-bg-hover); }
  .quantity-button:disabled { color: var(--color-text-tertiary); cursor: not-allowed; opacity: .5; }
  .purchase-total { text-align: right; }
  .purchase-total strong { display: block; margin-top: 4px; color: var(--color-text-primary); font-family: var(--font-family-mono); font-size: 24px; }

  .balance-summary { display: grid; grid-template-columns: 1fr 1fr; gap: var(--space-3); margin-bottom: var(--space-4); padding: var(--space-3) 0; border-top: 1px solid var(--color-border-muted); border-bottom: 1px solid var(--color-border-muted); }
  .balance-summary div { display: flex; flex-direction: column; gap: 5px; }
  .balance-summary div + div { padding-left: var(--space-3); border-left: 1px solid var(--color-border-muted); }
  .balance-summary strong { color: var(--color-text-primary); font-family: var(--font-family-mono); font-size: var(--text-sm); }
  .balance-summary .summary-danger { color: var(--color-danger); }
  :global(.shop-submit) { width: 100%; }
  .purchase-note { display: flex; justify-content: space-between; gap: var(--space-3); margin: var(--space-3) 0 0; color: var(--color-text-tertiary); font-size: var(--text-xs); line-height: 1.5; }
  .purchase-note span { text-align: right; }

  @media (max-width: 850px) {
    .shop-checkout { grid-template-columns: 1fr; }
    .shop-showcase { order: 2; }
    .shop-details { order: 1; }
    .showcase-stage { min-height: 320px; }
  }

  @media (max-width: 520px) {
    .shop-page { padding-top: var(--space-5); }
    .shop-showcase, .purchase-panel { padding: var(--space-4); }
    .product-facts { grid-template-columns: 1fr; }
    .showcase-facts { display: grid; grid-template-columns: 1fr 1fr; }
    .showcase-facts > div { border-right: 0; }
    .showcase-facts > div:last-child { grid-column: 1 / -1; padding-top: var(--space-2); border-top: 1px solid var(--color-border-muted); }
    .purchase-note { display: block; }
    .purchase-note span { display: block; margin-top: 3px; text-align: left; }
  }
</style>
