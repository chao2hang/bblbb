<!-- Steam 1000 款动态/静态个人资料背景库选品弹窗 -->
<script lang="ts">
  import Icon from '$lib/components/ui/Icon.svelte';
  import backgroundsData from '$lib/data/steam-profile-backgrounds.json';

  interface SteamBgItem {
    id: string;
    defid: number;
    appid: number;
    name: string;
    image: string;
    webm?: string | null;
    mp4?: string | null;
    animated?: boolean;
    cost: number;
  }

  let {
    open = $bindable(false),
    onselect
  }: {
    open: boolean;
    onselect: (item: {
      name: string;
      url: string;
      webm?: string | null;
      mp4?: string | null;
      cost: number;
      appid: number;
    }) => void;
  } = $props();

  let searchKeyword = $state('');
  let selectedCategory = $state('all');
  let currentPage = $state(1);
  const pageSize = 24;

  const CATEGORIES = [
    { id: 'all', label: '全部 (1000款)' },
    { id: 'animated', label: '🎬 动态背景' },
    { id: 'cyber', label: '⚡ 赛博科幻' },
    { id: 'scenery', label: '🌌 宇宙自然' },
    { id: 'anime', label: '🌸 二次元' },
    { id: 'dark', label: '🌑 暗黑深邃' }
  ];

  function getLocalImageUrl(item: SteamBgItem): string {
    return `/api/v1/steam-assets/backgrounds/${item.image}`;
  }

  function getFallbackCdnUrl(item: SteamBgItem): string {
    return `https://shared.fastly.steamstatic.com/community_assets/images/items/${item.appid}/${item.image}`;
  }

  const filteredItems = $derived.by(() => {
    const kw = searchKeyword.trim().toLowerCase();
    return (backgroundsData as SteamBgItem[]).filter((item) => {
      if (selectedCategory === 'animated' && !item.animated && !item.webm) return false;
      if (selectedCategory === 'cyber') {
        const kws = ['cyber', 'neon', 'city', 'tech', 'matrix', 'future', 'glitch'];
        if (!kws.some((k) => item.name.toLowerCase().includes(k))) return false;
      } else if (selectedCategory === 'scenery') {
        const kws = ['space', 'star', 'sun', 'sky', 'night', 'forest', 'mountain', 'sea', 'ocean'];
        if (!kws.some((k) => item.name.toLowerCase().includes(k))) return false;
      } else if (selectedCategory === 'anime') {
        const kws = ['girl', 'heart', 'pink', 'sakura', 'cute', 'cat', 'love', 'summer'];
        if (!kws.some((k) => item.name.toLowerCase().includes(k))) return false;
      } else if (selectedCategory === 'dark') {
        const kws = ['dark', 'black', 'hole', 'abyss', 'shadow', 'void', 'hell', 'demon'];
        if (!kws.some((k) => item.name.toLowerCase().includes(k))) return false;
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

  function handleSelect(item: SteamBgItem) {
    const url = getLocalImageUrl(item);
    onselect({
      name: item.name,
      url,
      webm: item.webm ? `https://shared.fastly.steamstatic.com/community_assets/images/items/${item.appid}/${item.webm}` : null,
      mp4: item.mp4 ? `https://shared.fastly.steamstatic.com/community_assets/images/items/${item.appid}/${item.mp4}` : null,
      cost: item.cost,
      appid: item.appid
    });
    open = false;
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      open = false;
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open}
  <div class="sf-backdrop" onclick={() => (open = false)} role="presentation"></div>

  <div class="sf-modal" role="dialog" aria-modal="true" aria-labelledby="sbg-title">
    <header class="sf-header">
      <div class="sf-header__title-group">
        <h3 id="sbg-title" class="sf-header__title">
          <span class="sf-badge">Steam 官方点数商店</span>
          动态个人资料背景库 ({backgroundsData.length} 款)
        </h3>
        <p class="sf-header__desc">
          来自 Steam 官方点数商店的动态个人资料背景，点击试穿即可实时呈现在个人主页与资料卡封面中！
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
          placeholder="搜索资料页背景名称、AppID 或风格（例如: Black Hole, Cyberpunk, Heart, Sunset...）"
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
            {cat.label}
          </button>
        {/each}
      </div>
    </div>

    <div class="sf-body">
      {#if pageItems.length === 0}
        <div class="sf-empty">
          <Icon name="palette" size={40} />
          <p>未找到匹配 “{searchKeyword}” 的背景</p>
          <button type="button" class="btn btn-secondary sm" onclick={() => { searchKeyword = ''; selectedCategory = 'all'; }}>
            重置筛选条件
          </button>
        </div>
      {:else}
        <div class="sbg-grid">
          {#each pageItems as item (item.id)}
            {@const localImg = getLocalImageUrl(item)}
            {@const fallbackImg = getFallbackCdnUrl(item)}
            <div class="sbg-card" onclick={() => handleSelect(item)} role="button" tabindex="0" onkeydown={(e) => e.key === 'Enter' && handleSelect(item)}>
              <div class="sbg-card__cover">
                <img
                  src={localImg}
                  alt={item.name}
                  loading="lazy"
                  class="sbg-card__img"
                  onerror={(e) => {
                    const target = e.currentTarget as HTMLImageElement;
                    if (target && !target.src.includes('steamstatic')) {
                      target.src = fallbackImg;
                    }
                  }}
                />
                {#if item.animated || item.webm}
                  <span class="sbg-card__anim-tag">动态</span>
                {/if}
              </div>
              <div class="sbg-card__info">
                <span class="sbg-card__name" title={item.name}>{item.name}</span>
                <div class="sbg-card__meta">
                  <span class="sbg-card__points">🪙 {item.cost} 点数</span>
                  <button type="button" class="sbg-use-btn" onclick={(e) => { e.stopPropagation(); handleSelect(item); }}>
                    试穿
                  </button>
                </div>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>

    <footer class="sf-footer">
      <div class="sf-footer__count">
        共找到 <strong>{filteredItems.length}</strong> 款资料背景 · 第 {currentPage} / {totalPages} 页
      </div>
      <div class="sf-pagination">
        <button
          type="button"
          class="sf-page-btn"
          disabled={currentPage <= 1}
          onclick={() => (currentPage = Math.max(1, currentPage - 1))}
        >
          上一页
        </button>
        <span class="sf-page-indicator">{currentPage}</span>
        <button
          type="button"
          class="sf-page-btn"
          disabled={currentPage >= totalPages}
          onclick={() => (currentPage = Math.min(totalPages, currentPage + 1))}
        >
          下一页
        </button>
      </div>
    </footer>
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
  .sbg-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(190px, 1fr));
    gap: 16px;
  }
  .sbg-card {
    border: 1px solid var(--color-border, #e5e7eb);
    border-radius: 12px;
    overflow: hidden;
    background: var(--color-bg-card, #fff);
    cursor: pointer;
    display: flex;
    flex-direction: column;
    transition: transform 0.15s ease, border-color 0.15s ease, box-shadow 0.15s ease;
    user-select: none;
  }
  .sbg-card:hover {
    transform: translateY(-3px);
    border-color: #66c0f4;
    box-shadow: 0 8px 16px -4px rgba(102, 192, 244, 0.25);
  }
  .sbg-card__cover {
    position: relative;
    width: 100%;
    height: 105px;
    background: #121824;
    overflow: hidden;
  }
  .sbg-card__img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .sbg-card__anim-tag {
    position: absolute;
    top: 6px;
    right: 6px;
    padding: 2px 6px;
    font-size: 10px;
    font-weight: 700;
    color: #fff;
    background: rgba(0, 0, 0, 0.65);
    border-radius: 4px;
    backdrop-filter: blur(2px);
  }
  .sbg-card__info {
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .sbg-card__name {
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--color-text, #222);
  }
  .sbg-card__meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: 2px;
  }
  .sbg-card__points {
    font-size: 11px;
    color: #d97706;
  }
  .sbg-use-btn {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 4px;
    background: rgba(102, 192, 244, 0.15);
    color: #0284c7;
    border: 1px solid rgba(102, 192, 244, 0.3);
    cursor: pointer;
    font-weight: 600;
  }
  .sbg-card:hover .sbg-use-btn {
    background: #0284c7;
    color: #fff;
  }
  .sf-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 60px 0;
    color: #888;
    gap: 12px;
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
