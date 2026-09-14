<!-- M07-UI-02：商城列表——价格/库存（售罄标记）/等级门槛（未达锁定）/限购/
  有效期/图标 Token。
  规范：严格复用全局组件（Icon, Button, EmptyState, CosmeticAvatar, CosmeticName）与
  全局样式规范（.card, .tabs, .tab, .badge, .btn, .input-field），不使用独立自定义样式体系。
  保持 .shop-grid 与 .shop-card 约定以兼容 mobile.css。
-->
<script lang="ts">
  import Icon from '$lib/components/ui/Icon.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import CosmeticAvatar from '$lib/components/wardrobe/CosmeticAvatar.svelte';
  import CosmeticName from '$lib/components/wardrobe/CosmeticName.svelte';
  import PageTitle from '$lib/components/PageTitle.svelte';
  import { formatMoney, productKindLabel, productStatusLabel } from '$lib/api/client';
  import { projectEntitlementTokens } from '$lib/components/wardrobe/tokens';
  import type { PublicPresentationTokens, ShopProduct, User } from '$lib/api/types';
  import type { ShopPageData } from './+page.server';

  let { data }: { data: ShopPageData & { user?: User | null } } = $props();

  const products = $derived(data.products);
  const balance = $derived(data.balance);
  const level = $derived(data.level);
  const error = $derived(data.error);
  const currentUser = $derived(data.user);

  // 预览昵称与头像
  const previewName = $derived(currentUser?.display_name || currentUser?.username || '夜猫子');
  const previewAvatarId = $derived(currentUser?.avatar_attachment_id ?? null);
  const previewSeed = $derived(currentUser?.username ?? currentUser?.id ?? 'preview');

  // 分类与筛选状态
  type CategoryTab = 'all' | 'nickname' | 'avatar' | 'space' | 'badge';
  let activeTab = $state<CategoryTab>('all');
  let searchQuery = $state('');
  let sortOption = $state<'default' | 'price-asc' | 'price-desc'>('default');
  let onlyFree = $state(false);

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

  function resolveVisualTokens(p: ShopProduct): PublicPresentationTokens {
    const projected = projectEntitlementTokens(p.presentation_tokens, p.asset_attachment_id, p.slot).visual;
    for (const token of p.presentation_tokens ?? []) {
      if (token.startsWith('nickname.color.')) projected.nickname_color = token.slice('nickname.color.'.length);
      else if (token.startsWith('avatar.frame.')) projected.avatar_frame = token.slice('avatar.frame.'.length);
      else if (token.startsWith('avatar.attachment.')) projected.avatar_attachment = token.slice('avatar.attachment.'.length);
      else if (token.startsWith('profile.effect.')) projected.profile_effect = token.slice('profile.effect.'.length);
      else if (token.startsWith('post.effect.')) projected.post_effect = token.slice('post.effect.'.length);
      else if (token.startsWith('badge.')) {
        projected.profile_badges = [...(projected.profile_badges ?? []), token.slice('badge.'.length)];
      }
    }
    if (p.asset_attachment_id && p.slot === 'avatar_frame') projected.avatar_frame_attachment_id = p.asset_attachment_id;
    return projected;
  }

  function titlePrefixText(p: ShopProduct): string | null {
    for (const token of p.presentation_tokens ?? []) {
      if (token.startsWith('title.prefix.')) {
        const val = token.slice('title.prefix.'.length);
        if (val === 'night_owl') return '夜猫子';
        return val;
      }
    }
    if ((p.kind as string) === 'title_prefix' || p.slot === 'title_prefix') {
      const match = p.title.match(/称号[：:]\s*(.+)/);
      return match ? match[1] : p.title;
    }
    return null;
  }

  function isNicknameDeco(p: ShopProduct): boolean {
    return p.slot === 'nickname_decoration' || (p.presentation_tokens ?? []).some((t) => t.startsWith('nickname.decoration.'));
  }

  function categoryOf(p: ShopProduct): CategoryTab {
    const k = p.kind as string;
    if (
      k === 'cosmetic_nickname' ||
      p.slot === 'nickname_color' ||
      p.slot === 'nickname_decoration' ||
      k === 'title_prefix' ||
      p.slot === 'title_prefix'
    ) {
      return 'nickname';
    }
    if (
      k === 'cosmetic_avatar' ||
      k === 'cosmetic_avatar_attachment' ||
      p.slot === 'avatar_frame' ||
      p.slot === 'avatar_attachment'
    ) {
      return 'avatar';
    }
    if (
      k === 'profile_effect' ||
      k === 'post_effect' ||
      p.slot === 'profile_effect' ||
      p.slot === 'post_effect'
    ) {
      return 'space';
    }
    if (
      k === 'cosmetic_badge' ||
      p.slot === 'profile_badge' ||
      p.slot === 'profile_badges' ||
      k === 'utility'
    ) {
      return 'badge';
    }
    return 'nickname';
  }

  // 分类统计
  const counts = $derived.by(() => {
    const res: Record<CategoryTab, number> = { all: products.length, nickname: 0, avatar: 0, space: 0, badge: 0 };
    for (const p of products) {
      const c = categoryOf(p);
      res[c] = (res[c] ?? 0) + 1;
    }
    return res;
  });

  // 过滤与排序
  const filteredProducts = $derived.by(() => {
    let list = [...products];

    if (activeTab !== 'all') {
      list = list.filter((p) => categoryOf(p) === activeTab);
    }

    if (onlyFree) {
      list = list.filter((p) => p.unit_price === 0);
    }

    const q = searchQuery.trim().toLowerCase();
    if (q) {
      list = list.filter(
        (p) =>
          p.title.toLowerCase().includes(q) ||
          (p.description_safe && p.description_safe.toLowerCase().includes(q)) ||
          productKindLabel(p.kind).toLowerCase().includes(q)
      );
    }

    if (sortOption === 'price-asc') {
      list.sort((a, b) => a.unit_price - b.unit_price);
    } else if (sortOption === 'price-desc') {
      list.sort((a, b) => b.unit_price - a.unit_price);
    }

    return list;
  });

  function resetFilters(): void {
    activeTab = 'all';
    searchQuery = '';
    onlyFree = false;
    sortOption = 'default';
  }
</script>

<PageTitle title="商城与积分" />

<div class="container page-content" id="page-shop">
  <header style="margin-bottom:var(--space-4);">
    <h1 style="display:flex;align-items:center;gap:var(--space-2);margin:0 0 var(--space-2);font-size:var(--text-2xl);">
      <Icon name="shopping-bag" size={26} />
      商城与积分
    </h1>
    <p class="text-secondary" style="margin:0;">使用社区活动金币兑换专属头像框、流光特效与个性身份装扮</p>
  </header>

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
    <!-- 全局 Card：个人账户资产与快捷入口 -->
    <div class="card" style="margin-bottom:var(--space-4);">
      <div class="card-body" style="display:flex;flex-wrap:wrap;gap:var(--space-4);align-items:center;justify-content:space-between;">
        <div style="display:flex;align-items:center;gap:var(--space-3);flex-wrap:wrap;">
          <CosmeticAvatar
            name={currentUser?.display_name || currentUser?.username || '当前用户'}
            size="md"
            presentation={currentUser?.presentation_tokens}
            avatarAttachmentId={currentUser?.avatar_attachment_id}
            seed={currentUser?.username ?? currentUser?.id}
          />
          <div style="display:flex;align-items:baseline;gap:var(--space-2);">
            <strong style="font-size:var(--text-base);">{currentUser?.display_name || currentUser?.username || '访客用户'}</strong>
            {#if typeof level === 'number'}
              <span class="badge badge-level">LV.{level}</span>
            {/if}
          </div>
          <div style="display:inline-flex;align-items:center;gap:var(--space-2);padding-left:var(--space-3);border-left:1px solid var(--color-border);">
            <span class="badge badge-success">我的余额</span>
            <strong style="font-size:var(--text-lg);font-family:var(--aui-font-mono, monospace);">
              {balance ? formatMoney(balance.amount, { id: balance.currency, code: balance.currency }) : '—'}
            </strong>
          </div>
        </div>

        <div style="display:flex;gap:var(--space-2);flex-wrap:wrap;">
          <a class="btn secondary sm" href="/me/wardrobe">
            <Icon name="palette" size={14} />
            <span>我的衣柜</span>
          </a>
          <a class="btn secondary sm" href="/me/balance">
            <Icon name="coins" size={14} />
            <span>积分明细</span>
          </a>
          <a class="btn secondary sm" href="/me/billing">
            <Icon name="file-text" size={14} />
            <span>查看账单</span>
          </a>
        </div>
      </div>
    </div>

    <!-- 全局 Card + Tabs：分类切换与搜索筛选 -->
    <div class="card" style="margin-bottom:var(--space-4);">
      <div class="tabs">
        <button
          type="button"
          class="tab"
          class:is-active={activeTab === 'all'}
          onclick={() => (activeTab = 'all')}
        >
          <Icon name="store" size={15} />
          <span>全部商品</span>
          <span class="tab-count">{counts.all}</span>
        </button>
        <button
          type="button"
          class="tab"
          class:is-active={activeTab === 'nickname'}
          onclick={() => (activeTab = 'nickname')}
        >
          <Icon name="wand-2" size={15} />
          <span>昵称装扮</span>
          <span class="tab-count">{counts.nickname}</span>
        </button>
        <button
          type="button"
          class="tab"
          class:is-active={activeTab === 'avatar'}
          onclick={() => (activeTab = 'avatar')}
        >
          <Icon name="award" size={15} />
          <span>头像装扮</span>
          <span class="tab-count">{counts.avatar}</span>
        </button>
        <button
          type="button"
          class="tab"
          class:is-active={activeTab === 'space'}
          onclick={() => (activeTab = 'space')}
        >
          <Icon name="sparkles" size={15} />
          <span>空间与帖子</span>
          <span class="tab-count">{counts.space}</span>
        </button>
        {#if counts.badge > 0}
          <button
            type="button"
            class="tab"
            class:is-active={activeTab === 'badge'}
            onclick={() => (activeTab = 'badge')}
          >
            <Icon name="trophy" size={15} />
            <span>徽章道具</span>
            <span class="tab-count">{counts.badge}</span>
          </button>
        {/if}
      </div>

      <div class="card-body" style="display:flex;align-items:center;justify-content:space-between;gap:var(--space-3);flex-wrap:wrap;padding:var(--space-3) var(--space-4);">
        <div style="flex:1 1 240px;max-width:360px;position:relative;display:flex;align-items:center;">
          <span style="position:absolute;left:10px;color:var(--color-text-muted);pointer-events:none;display:grid;">
            <Icon name="search" size={15} />
          </span>
          <input
            type="text"
            class="input-field"
            style="padding-left:34px !important; padding-right:32px !important;"
            placeholder="搜索装扮名称或描述..."
            bind:value={searchQuery}
            aria-label="搜索装扮"
          />
          {#if searchQuery}
            <button
              type="button"
              style="position:absolute;right:8px;border:none;background:none;color:var(--color-text-muted);cursor:pointer;"
              onclick={() => (searchQuery = '')}
              aria-label="清空搜索"
            >
              <Icon name="x" size={14} />
            </button>
          {/if}
        </div>

        <div style="display:flex;align-items:center;gap:var(--space-2);flex-wrap:wrap;">
          <button
            type="button"
            class="btn sm"
            class:primary={onlyFree}
            class:secondary={!onlyFree}
            onclick={() => (onlyFree = !onlyFree)}
          >
            <span>只看免费</span>
          </button>

          <select class="input-field" style="height:32px;padding:0 var(--space-2);font-size:var(--text-xs);" bind:value={sortOption} aria-label="排序方式">
            <option value="default">默认推荐</option>
            <option value="price-asc">价格：从低到高</option>
            <option value="price-desc">价格：从高到低</option>
          </select>
        </div>
      </div>
    </div>
  {/if}

  <!-- 商品列表 -->
  {#if !error && products.length === 0}
    <div class="card">
      <div class="card-body">
        <EmptyState icon="shopping-bag" title="商城暂未上架商品" desc="管理员正在上架更多精美装扮与道具，敬请期待！" />
      </div>
    </div>
  {:else if !error && filteredProducts.length === 0}
    <div class="card">
      <div class="card-body">
        <EmptyState icon="search" title="未找到匹配的装扮" desc="请尝试更换分类标签或清除关键词筛选。" />
        <div style="margin-top:var(--space-3);text-align:center;">
          <Button text="重置所有筛选" variant="secondary" size="sm" onclick={resetFilters} />
        </div>
      </div>
    </div>
  {:else if !error}
    <div class="shop-grid">
      {#each filteredProducts as product (product.id)}
        {@const out = isSoldOut(product)}
        {@const lockedP = locked(product)}
        {@const tokens = resolveVisualTokens(product)}
        {@const prefix = titlePrefixText(product)}
        {@const isDeco = isNicknameDeco(product)}
        {@const category = categoryOf(product)}

        <a
          class="card shop-card"
          class:is-dim={lockedP || out}
          href={lockedP || out ? undefined : `/shop/${product.id}`}
          aria-disabled={lockedP || out || undefined}
        >
          <!-- 预览舞台：使用全局组件展示真实装扮效果 -->
          <div class="shop-card-preview">
            {#if category === 'avatar'}
              <CosmeticAvatar
                name={previewName}
                size="xl"
                avatarAttachmentId={previewAvatarId}
                seed={previewSeed}
                presentation={tokens}
              />
            {:else if prefix}
              <div style="display:inline-flex;align-items:center;gap:var(--space-2);">
                <span class="badge badge-primary">[{prefix}]</span>
                <strong style="font-size:var(--text-sm);">{previewName}</strong>
              </div>
            {:else if isDeco}
              <div style="display:inline-flex;align-items:center;gap:var(--space-1);">
                <span style="font-size:18px;">👑</span>
                <strong style="font-size:var(--text-sm);">{previewName}</strong>
              </div>
            {:else if product.slot === 'nickname_color'}
              <CosmeticName
                name={previewName}
                presentation={tokens}
                class="shop-preview-nickname"
              />
            {:else if product.kind === 'profile_effect' || product.slot === 'profile_effect'}
              <div style="display:inline-flex;align-items:center;gap:var(--space-2);">
                <Icon name="sparkles" size={20} />
                <span class="badge badge-neutral">星芒空间装扮</span>
              </div>
            {:else if product.kind === 'post_effect' || product.slot === 'post_effect'}
              <div style="display:inline-flex;align-items:center;gap:var(--space-2);">
                <span style="font-size:16px;">❤️</span>
                <span class="badge badge-neutral">感谢作者</span>
              </div>
            {:else if product.kind === 'cosmetic_badge' || product.slot === 'profile_badge'}
              <div style="display:inline-flex;align-items:center;gap:var(--space-2);">
                <Icon name="award" size={20} />
                <span class="badge badge-neutral">{product.title.replace(/徽章[：:]\s*/, '')}</span>
              </div>
            {:else}
              <Icon name={iconFor(product)} size={30} />
            {/if}
          </div>

          <!-- 标题与类型 -->
          <div class="shop-card-title">
            <strong style="font-size:var(--text-sm);font-weight:var(--weight-semibold);color:var(--color-text-primary);">{product.title}</strong>
            <span class="badge badge-neutral">{productKindLabel(product.kind)}</span>
          </div>

          {#if product.description_safe}
            <p class="shop-card-desc">{product.description_safe}</p>
          {:else}
            <p class="shop-card-desc" style="color:var(--color-text-muted);">佩戴后展示在个人资料与社区发言中</p>
          {/if}

          <!-- 卡片底栏：价格与状态/操作 -->
          <div class="shop-card-footer">
            <span class="shop-price">{priceLabel(product)}</span>
            {#if out}
              <span class="badge badge-danger">已售罄</span>
            {:else if lockedP}
              <span class="badge badge-warning">需 LV.{product.required_level}</span>
            {:else}
              <span class="btn primary sm" style="height:26px;min-height:26px;padding:0 10px;font-size:var(--text-xs);">
                兑换
              </span>
            {/if}
          </div>

          <!-- 元信息标签 -->
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

  /* 商品网格（对齐 mobile.css 规范） */
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
    transition: border-color var(--duration-fast) var(--ease-out), box-shadow var(--duration-fast) var(--ease-out);
  }

  .shop-card:hover {
    border-color: var(--color-border-strong);
    box-shadow: var(--shadow-sm);
  }

  .shop-card.is-dim {
    opacity: 0.65;
    cursor: not-allowed;
  }

  /* 预览舞台：固定高度使卡片对齐 */
  .shop-card-preview {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 84px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-bg-subtle);
    overflow: hidden;
  }

  :global(.shop-preview-nickname) {
    font-size: var(--text-lg);
    font-weight: 700;
  }

  .shop-card-title {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    flex-wrap: wrap;
  }

  .shop-card-desc {
    margin: 0;
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    min-height: 2.8em;
  }

  .shop-card-footer {
    margin-top: auto;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    padding-top: var(--space-2);
    border-top: 1px dashed var(--color-border);
  }

  .shop-price {
    font-size: var(--text-base);
    font-weight: var(--weight-bold, 700);
    color: var(--color-text-primary);
  }

  .shop-card-meta {
    font-size: var(--text-xs);
    color: var(--color-text-secondary);
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }
</style>
