<!-- 后台商城管理 Steam 头像框直接选品定价上架弹窗 -->
<script lang="ts">
  import Icon from '$lib/components/ui/Icon.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  // 目录数据：默认用内置快照兜底，弹窗打开时从 API 拉取实时镜像
  // （/api/v1/shop/steam-catalog，管理端「同步 Steam 目录」刷新）。
  import framesFallback from '$lib/data/steam-avatar-frames.json';
  import { enhance } from '$app/forms';
  import type { SubmitFunction } from '@sveltejs/kit';

  interface SteamFrameItem {
    id: string;
    defid: number;
    appid: number;
    name: string;
    image: string;
    cost: number;
    shape?: 'circle' | 'rounded';
    scale?: number;
  }

  let {
    open = $bindable(false),
    enhanceHandler
  }: {
    open: boolean;
    enhanceHandler: SubmitFunction;
  } = $props();

  let searchKeyword = $state('');
  let selectedCategory = $state('all');
  let currentPage = $state(1);
  const pageSize = 24;

  let activeItem = $state<SteamFrameItem | null>(null);
  let customTitle = $state('');
  let customPrice = $state(200);
  let customStock = $state<string | number>('');
  let validityPreset = $state<string>('0');
  let customValidityDays = $state<number>(30);
  let customLimit = $state<number>(1);
  let customLevel = $state<number>(1);
  let submitting = $state(false);

  let framesData = $state<SteamFrameItem[]>(framesFallback as SteamFrameItem[]);

  $effect(() => {
    if (!open) return;
    void (async () => {
      try {
        const res = await fetch('/api/v1/shop/steam-catalog?kind=frames&page_size=3000');
        if (!res.ok) return;
        const body = (await res.json()) as { items?: (SteamFrameItem & { extra?: Record<string, unknown> })[] };
        if (!body.items?.length) return;
        framesData = body.items.map((x) => ({ ...x, ...(x.extra ?? {}) }));
      } catch {
        // 网络失败保留内置快照兜底
      }
    })();
  });

  const totalFrameCount = $derived(framesData.length);
  const CATEGORIES = [
    { id: 'all', label: '全部' },
    { id: 'popular', label: '🔥 热门推荐' },
    { id: 'cyber', label: '⚡ 赛博科幻' },
    { id: 'cute', label: '🐱 萌系可爱' },
    { id: 'element', label: '🔥 元素魔法' },
    { id: 'oriental', label: '🐉 国风仙侠' }
  ];

  function getFrameUrl(item: SteamFrameItem): string {
    return `/api/v1/steam-assets/frames/${item.image}`;
  }

  function getFallbackCdnUrl(item: SteamFrameItem): string {
    return `https://shared.fastly.steamstatic.com/community_assets/images/items/${item.appid}/${item.image}`;
  }

  const filteredItems = $derived.by(() => {
    const kw = searchKeyword.trim().toLowerCase();
    return framesData.filter((item) => {
      if (selectedCategory === 'popular') {
        const popularKeywords = ['frame', 'avatar', 'fire', 'glow', 'dragon', 'star', 'whiskers'];
        const matches = popularKeywords.some((k) => item.name.toLowerCase().includes(k));
        if (!matches && item.cost < 2000) return false;
      } else if (selectedCategory === 'cyber') {
        const cyberKw = ['glitch', 'neon', 'cyber', 'matrix', 'signal', 'terminal', 'tech', 'bot', 'future'];
        if (!cyberKw.some((k) => item.name.toLowerCase().includes(k))) return false;
      } else if (selectedCategory === 'cute') {
        const cuteKw = ['whiskers', 'cat', 'dog', 'pet', 'bunny', 'pink', 'sweet', 'cute', 'fox'];
        if (!cuteKw.some((k) => item.name.toLowerCase().includes(k))) return false;
      } else if (selectedCategory === 'element') {
        const elemKw = ['fire', 'flame', 'ice', 'snow', 'water', 'lightning', 'thunder', 'storm', 'ocean', 'wave'];
        if (!elemKw.some((k) => item.name.toLowerCase().includes(k))) return false;
      } else if (selectedCategory === 'oriental') {
        const orKw = ['头像框', '龙', '剑', '云', '仙', '琉璃', '梦语', 'china', 'dragon', 'jade', 'lotus'];
        if (!orKw.some((k) => item.name.toLowerCase().includes(k))) return false;
      }

      if (kw) {
        return item.name.toLowerCase().includes(kw) || String(item.appid).includes(kw);
      }
      return true;
    });
  });

  const totalPages = $derived(Math.max(1, Math.ceil(filteredItems.length / pageSize)));
  const pageItems = $derived(
    filteredItems.slice((currentPage - 1) * pageSize, currentPage * pageSize)
  );

  function pickForPricing(item: SteamFrameItem) {
    activeItem = item;
    customTitle = item.name.slice(0, 32);
    customPrice = Math.max(10, Math.round(item.cost / 10));
    customStock = '';
    validityPreset = '0';
    customValidityDays = 30;
    customLimit = 1;
    customLevel = 1;
  }

  const cosmeticJson = $derived.by(() => {
    if (!activeItem) return '';
    return JSON.stringify({
      kind: 'avatar_frame',
      name: customTitle.trim() || activeItem.name.slice(0, 32),
      style: {
        mode: 'steam_frame',
        appid: activeItem.appid,
        shape: activeItem.shape || 'rounded',
        frameScale: activeItem.scale || 120,
        url: getFrameUrl(activeItem),
        image: activeItem.image,
        css: `/* Steam 动效头像框 - ${activeItem.name} (AppID: ${activeItem.appid}) */\nfilter: drop-shadow(0 0 6px rgba(102, 192, 244, 0.35));`
      }
    });
  });

  const productJson = $derived.by(() => {
    if (!activeItem) return '';

    let validity_seconds: number | null = null;
    if (validityPreset === 'custom') {
      const days = Number(customValidityDays) || 0;
      if (days > 0) validity_seconds = days * 86400;
    } else {
      const days = Number(validityPreset) || 0;
      if (days > 0) validity_seconds = days * 86400;
    }

    const stockNum = customStock === '' || customStock === null ? null : Number(customStock);
    const stock_remaining = stockNum != null && stockNum > 0 ? stockNum : null;

    return JSON.stringify({
      title: customTitle.trim() || activeItem.name.slice(0, 32),
      unit_price: Number(customPrice) || 0,
      stock_remaining,
      validity_seconds,
      quantity_limit: Math.max(1, Number(customLimit) || 1),
      required_level: Math.max(1, Number(customLevel) || 1),
      status: 'published',
      refund_policy: 'non_refundable'
    });
  });

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      if (activeItem) activeItem = null;
      else open = false;
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open}
  <div class="sf-backdrop" onclick={() => (open = false)} role="presentation"></div>

  <div class="sf-modal" role="dialog" aria-modal="true">
    <header class="sf-header">
      <div class="sf-header__title-group">
        <h3 class="sf-header__title">
          <span class="sf-badge">Steam 原生正版</span>
          一键选品上架 Steam 动效头像框 ({framesData.length} 款)
        </h3>
        <p class="sf-header__desc">
          无需配置任何样式！直接选择你中意的 Steam 动效头像框，输入定价即可一键直接上架到商城供用户购买。
        </p>
      </div>
      <button type="button" class="sf-close-btn" onclick={() => (open = false)} aria-label="关闭窗口">
        <Icon name="x" size={18} />
      </button>
    </header>

    <div class="sf-toolbar">
      <div class="sf-search-box">
        <Icon name="search" size={16} />
        <input
          type="text"
          class="sf-search-input"
          placeholder="快速搜索头像框名称、AppID 或类别..."
          bind:value={searchKeyword}
          oninput={() => (currentPage = 1)}
        />
        {#if searchKeyword}
          <button type="button" class="sf-clear-btn" onclick={() => { searchKeyword = ''; currentPage = 1; }}>
            <Icon name="x" size={14} />
          </button>
        {/if}
      </div>

      <div class="sf-categories">
        {#each CATEGORIES as cat (cat.id)}
          <button
            type="button"
            class="sf-cat-btn"
            class:is-active={selectedCategory === cat.id}
            onclick={() => { selectedCategory = cat.id; currentPage = 1; }}
          >
            {cat.id === 'all' ? `全部 (${framesData.length}款)` : cat.label}
          </button>
        {/each}
      </div>
    </div>

    <div class="sf-body">
      {#if activeItem}
        <!-- 定价上架确认抽屉 -->
        <div class="sf-pricing-box">
          <div class="sf-pricing-preview">
            <div class="sf-card__preview" style="width: 100px; height: 100px;">
              <div class="sf-card__avatar-mock" style="width: 72px; height: 72px;"></div>
              <img class="sf-card__frame-img" src={getFrameUrl(activeItem)} alt={activeItem.name} />
            </div>
            <span style="font-size: 13px; font-weight: 600; color: #66c0f4;">Steam 原画 APNG 动图</span>
          </div>

          <form
            class="sf-pricing-form"
            method="POST"
            action="?/quickPublish"
            use:enhance={async (opts) => {
              submitting = true;
              const fn = await enhanceHandler(opts);
              return async (ctx) => {
                submitting = false;
                if (typeof fn === 'function') await fn(ctx);
                if (ctx.result.type === 'success') {
                  activeItem = null;
                  open = false;
                }
              };
            }}
          >
            <input type="hidden" name="cosmetic" value={cosmeticJson} />
            <input type="hidden" name="product" value={productJson} />

            <div class="input-wrapper">
              <label class="input-label" for="sp-title">商品上架标题 *</label>
              <input id="sp-title" class="input-field" type="text" required maxlength="32" bind:value={customTitle} disabled={submitting} />
            </div>

            <div class="sf-form-row">
              <div class="input-wrapper">
                <label class="input-label" for="sp-price">商城售价 (社区金币) *</label>
                <div style="display:flex;align-items:center;gap:6px;">
                  <input id="sp-price" class="input-field" type="number" min="0" required bind:value={customPrice} disabled={submitting} style="flex:1;" />
                  <span style="font-size:11px;color:#d97706;white-space:nowrap;">(原: {activeItem.cost}点)</span>
                </div>
              </div>

              <div class="input-wrapper">
                <label class="input-label" for="sp-stock">上架数量 (库存)</label>
                <input id="sp-stock" class="input-field" type="number" min="0" placeholder="留空为不限数量" bind:value={customStock} disabled={submitting} />
              </div>
            </div>

            <div class="sf-form-row">
              <div class="input-wrapper">
                <label class="input-label" for="sp-validity">有效期限</label>
                <select id="sp-validity" class="input-field select-field" bind:value={validityPreset} disabled={submitting}>
                  <option value="0">永久有效 (默认)</option>
                  <option value="7">7 天 (体验版)</option>
                  <option value="30">30 天 (月度卡)</option>
                  <option value="90">90 天 (季度卡)</option>
                  <option value="365">365 天 (年度卡)</option>
                  <option value="custom">自定义有效天数...</option>
                </select>
                {#if validityPreset === 'custom'}
                  <div style="display:flex;align-items:center;gap:6px;margin-top:6px;">
                    <input type="number" class="input-field" min="1" max="3650" placeholder="输入天数" bind:value={customValidityDays} disabled={submitting} style="flex:1;" />
                    <span style="font-size:12px;color:var(--color-text-secondary);">天</span>
                  </div>
                {/if}
              </div>

              <div class="input-wrapper">
                <label class="input-label" for="sp-limit">每人限购数量</label>
                <input id="sp-limit" class="input-field" type="number" min="1" max="999" placeholder="默认 1" bind:value={customLimit} disabled={submitting} />
              </div>
            </div>

            <div class="input-wrapper">
              <label class="input-label" for="sp-level">最低购买等级门槛 (TL0-4)</label>
              <input id="sp-level" class="input-field" type="number" min="1" max="10" placeholder="默认 1" bind:value={customLevel} disabled={submitting} />
            </div>

            <div style="display:flex;gap:8px;margin-top:12px;">
              <Button text="返回选品列表" variant="secondary" size="sm" type="button" onclick={() => (activeItem = null)} disabled={submitting} />
              <Button text={submitting ? "正在上架…" : "确认并立即上架到商城"} variant="primary" size="sm" type="submit" disabled={submitting || !customTitle.trim()} />
            </div>
          </form>
        </div>
      {:else}
        <!-- 选品网格 -->
        <div class="sf-grid">
          {#each pageItems as item (item.id)}
            {@const localUrl = getFrameUrl(item)}
            {@const fallbackUrl = getFallbackCdnUrl(item)}
            <div class="sf-card" onclick={() => pickForPricing(item)} role="button" tabindex="0" onkeydown={(e) => e.key === 'Enter' && pickForPricing(item)}>
              <div class="sf-card__preview">
                <div class="sf-card__avatar-mock"></div>
                <img
                  class="sf-card__frame-img"
                  src={localUrl}
                  onerror={(e) => {
                    const target = e.currentTarget as HTMLImageElement;
                    if (target && !target.src.includes('steamstatic')) {
                      target.src = fallbackUrl;
                    }
                  }}
                  alt={item.name}
                  loading="lazy"
                />
              </div>
              <div class="sf-card__info">
                <span class="sf-card__name" title={item.name}>{item.name}</span>
                <div class="sf-card__meta">
                  <span class="sf-card__points">🪙 {item.cost} 点</span>
                  <button type="button" class="sf-use-btn" onclick={(e) => { e.stopPropagation(); pickForPricing(item); }}>
                    定价上架
                  </button>
                </div>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>

    {#if !activeItem}
      <footer class="sf-footer">
        <div class="sf-footer__count">
          共 <strong>{filteredItems.length}</strong> 款头像框 · 第 {currentPage} / {totalPages} 页
        </div>
        <div class="sf-pagination">
          <button type="button" class="sf-page-btn" disabled={currentPage <= 1} onclick={() => (currentPage = Math.max(1, currentPage - 1))}>
            上一页
          </button>
          <span class="sf-page-indicator">{currentPage}</span>
          <button type="button" class="sf-page-btn" disabled={currentPage >= totalPages} onclick={() => (currentPage = Math.min(totalPages, currentPage + 1))}>
            下一页
          </button>
        </div>
      </footer>
    {/if}
  </div>
{/if}

<style>
  .sf-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.65);
    backdrop-filter: blur(4px);
    z-index: 1000;
  }
  .sf-modal {
    position: fixed;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: min(94vw, 1100px);
    height: min(90vh, 760px);
    background: var(--color-bg-card, #fff);
    border: 1px solid var(--color-border, #ccc);
    border-radius: 16px;
    box-shadow: 0 25px 50px -12px rgba(0, 0, 0, 0.4);
    display: flex;
    flex-direction: column;
    z-index: 1001;
    overflow: hidden;
  }
  .sf-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    padding: 18px 24px 14px;
    border-bottom: 1px solid var(--color-border, #eee);
  }
  .sf-header__title {
    margin: 0;
    font-size: 18px;
    font-weight: 700;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .sf-badge {
    font-size: 11px;
    background: #171a21;
    color: #66c0f4;
    padding: 2px 8px;
    border-radius: 4px;
    font-weight: 600;
  }
  .sf-header__desc {
    margin: 4px 0 0;
    font-size: 13px;
    color: var(--color-text-secondary, #666);
  }
  .sf-close-btn {
    border: none;
    background: transparent;
    cursor: pointer;
    color: var(--color-text-tertiary, #999);
    padding: 4px;
    border-radius: 6px;
    display: flex;
  }
  .sf-close-btn:hover {
    background: var(--color-bg-subtle, #f5f5f5);
    color: var(--color-text, #222);
  }
  .sf-toolbar {
    padding: 12px 24px;
    background: var(--color-bg-subtle, #f8f9fa);
    border-bottom: 1px solid var(--color-border, #eee);
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .sf-search-box {
    display: flex;
    align-items: center;
    gap: 8px;
    background: var(--color-bg-card, #fff);
    border: 1px solid var(--color-border, #ccc);
    border-radius: 8px;
    padding: 6px 12px;
  }
  .sf-search-input {
    border: none;
    outline: none;
    flex: 1;
    font-size: 13px;
    background: transparent;
  }
  .sf-clear-btn {
    border: none;
    background: none;
    cursor: pointer;
    color: #999;
    padding: 0;
    display: flex;
  }
  .sf-categories {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .sf-cat-btn {
    padding: 4px 12px;
    font-size: 12px;
    border: 1px solid var(--color-border, #ddd);
    border-radius: 999px;
    background: var(--color-bg-card, #fff);
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .sf-cat-btn.is-active {
    background: #171a21;
    color: #66c0f4;
    border-color: #171a21;
    font-weight: 600;
  }
  .sf-body {
    flex: 1;
    overflow-y: auto;
    padding: 18px 24px;
  }
  .sf-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(135px, 1fr));
    gap: 16px;
  }
  .sf-card {
    border: 1px solid var(--color-border, #e5e7eb);
    border-radius: 12px;
    padding: 10px;
    background: var(--color-bg-card, #fff);
    cursor: pointer;
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    transition: transform 0.15s ease, border-color 0.15s ease, box-shadow 0.15s ease;
    user-select: none;
  }
  .sf-card:hover {
    transform: translateY(-3px);
    border-color: #66c0f4;
    box-shadow: 0 8px 16px -4px rgba(102, 192, 244, 0.25);
  }
  .sf-card__preview {
    position: relative;
    width: 84px;
    height: 84px;
    display: flex;
    align-items: center;
    justify-content: center;
    margin-bottom: 8px;
  }
  .sf-card__avatar-mock {
    width: 60px;
    height: 60px;
    border-radius: 4px;
    background: linear-gradient(135deg, #383e4a, #1f232b);
  }
  .sf-card__frame-img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: contain;
    pointer-events: none;
  }
  .sf-card__info {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .sf-card__name {
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--color-text, #222);
  }
  .sf-card__meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: 2px;
  }
  .sf-card__points {
    font-size: 11px;
    color: #d97706;
  }
  .sf-use-btn {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 4px;
    background: rgba(102, 192, 244, 0.15);
    color: #0284c7;
    border: 1px solid rgba(102, 192, 244, 0.3);
    cursor: pointer;
    font-weight: 600;
  }
  .sf-card:hover .sf-use-btn {
    background: #0284c7;
    color: #fff;
  }
  .sf-pricing-box {
    display: flex;
    align-items: flex-start;
    justify-content: center;
    gap: 36px;
    padding: 26px 30px;
    border: 1px solid var(--color-border);
    border-radius: 12px;
    background: var(--color-bg-subtle);
    max-width: 720px;
    margin: 16px auto;
  }
  .sf-pricing-preview {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    margin-top: 10px;
  }
  .sf-pricing-form {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .sf-form-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  @media (max-width: 640px) {
    .sf-pricing-box {
      flex-direction: column;
      align-items: center;
      gap: 16px;
      padding: 16px;
    }
    .sf-form-row {
      grid-template-columns: 1fr;
    }
  }
  .sf-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 24px;
    border-top: 1px solid var(--color-border, #eee);
    background: var(--color-bg-subtle, #fcfcfc);
  }
  .sf-footer__count {
    font-size: 13px;
    color: var(--color-text-secondary, #666);
  }
  .sf-pagination {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .sf-page-btn {
    padding: 4px 12px;
    font-size: 12px;
    border: 1px solid var(--color-border, #ccc);
    border-radius: 6px;
    background: var(--color-bg-card, #fff);
    cursor: pointer;
  }
  .sf-page-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .sf-page-indicator {
    font-size: 13px;
    font-weight: 600;
    min-width: 24px;
    text-align: center;
  }
</style>
