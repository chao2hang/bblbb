<!-- Steam 点数商店 2000+ 款正版动效头像框浏览与一键选品弹窗 -->
<script lang="ts">
  import Icon from '$lib/components/ui/Icon.svelte';
  // 目录数据：内置快照兜底 + 打开时拉取实时镜像（/api/v1/shop/steam-catalog）。
  import framesFallback from '$lib/data/steam-avatar-frames.json';

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
    onselect
  }: {
    open: boolean;
    onselect: (item: {
      name: string;
      url: string;
      cost: number;
      appid: number;
      shape: 'circle' | 'rounded';
      scale: number;
    }) => void;
  } = $props();

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
  let searchKeyword = $state('');
  let selectedCategory = $state('all');
  let currentPage = $state(1);
  const pageSize = 32;

  const CATEGORIES = [
    { id: 'all', label: '全部 (2071款)' },
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
    return (framesData as SteamFrameItem[]).filter((item) => {
      // 类别筛选
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

      // 关键词搜索
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

  function handleSelect(item: SteamFrameItem) {
    const url = getFrameUrl(item);
    onselect({
      name: item.name,
      url,
      cost: item.cost,
      appid: item.appid,
      shape: item.shape || 'rounded',
      scale: item.scale || 120
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
  <!-- 模态背景遮罩 -->
  <div class="sf-backdrop" onclick={() => (open = false)} role="presentation"></div>

  <!-- 选品抽屉/弹窗 -->
  <div class="sf-modal" role="dialog" aria-modal="true" aria-labelledby="sf-title">
    <header class="sf-header">
      <div class="sf-header__title-group">
        <h3 id="sf-title" class="sf-header__title">
          <span class="sf-badge">Steam 官方点数商店</span>
          海量 APNG 动效头像框库 ({framesData.length} 款)
        </h3>
        <p class="sf-header__desc">
          直接浏览并搜索 Steam 平台全量动效透明头像框，点击即可实时穿戴试穿，并一键导入到装扮工作台！
        </p>
      </div>
      <button type="button" class="sf-close-btn" onclick={() => (open = false)} aria-label="关闭窗口">
        <Icon name="x" size={18} />
      </button>
    </header>

    <!-- 工具与筛选栏 -->
    <div class="sf-toolbar">
      <div class="sf-search-box">
        <Icon name="search" size={16} />
        <input
          type="text"
          class="sf-search-input"
          placeholder="搜索头像框名称、游戏 AppID 或风格（例如: Fire, Cat, Dragon, 赛博...）"
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

    <!-- 头像框网格展示区 -->
    <div class="sf-body">
      {#if pageItems.length === 0}
        <div class="sf-empty">
          <Icon name="palette" size={40} />
          <p>未找到匹配 “{searchKeyword}” 的头像框</p>
          <button type="button" class="btn btn-secondary sm" onclick={() => { searchKeyword = ''; selectedCategory = 'all'; }}>
            重置筛选条件
          </button>
        </div>
      {:else}
        <div class="sf-grid">
          {#each pageItems as item (item.id)}
            {@const localUrl = getFrameUrl(item)}
            {@const fallbackUrl = getFallbackCdnUrl(item)}
            <div class="sf-card" onclick={() => handleSelect(item)} role="button" tabindex="0" onkeydown={(e) => e.key === 'Enter' && handleSelect(item)}>
              <div class="sf-card__preview">
                <!-- 内部头像占位底图，用来衬托透明边框的动态环绕效果 -->
                <div class="sf-card__avatar-mock"></div>
                <!-- 真实 Steam APNG 动图播放（优先本地，未就绪自动回退 CDN） -->
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
                  <span class="sf-card__points">🪙 {item.cost} 点数</span>
                  <button type="button" class="sf-use-btn" onclick={(e) => { e.stopPropagation(); handleSelect(item); }}>
                    试穿
                  </button>
                </div>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>

    <!-- 分页底栏 -->
    <footer class="sf-footer">
      <div class="sf-footer__count">
        共找到 <strong>{filteredItems.length}</strong> 款头像框 · 第 {currentPage} / {totalPages} 页
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
    border-radius: 50%;
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
