<!-- M07-UI-08 & M18-ADMIN-BATCH：管理端商城——商品列表/新建/编辑/发布/停售 + 订单/退款。
  约定 A：所有写操作 = 按钮 → Dialog（表单在弹层内，reason/version 按 server 契约携带，
  成功后 toastActionResult → update → 关闭弹层并清理 target）。
  约定 B：商品列表选择列 + BatchBar →「批量上架」「批量下架」Dialog（循环单条端点）。
  约定 D：商品行「操作」按钮 → 「⋯」三点菜单（编辑/上架|下架，菜单项决定动作，
  Dialog 按 tab 渲染对应表单节）；订单行「退款」同样收进「⋯」菜单，行内操作有且只有「⋯」触发按钮。
  版本冲突（409）提示刷新；退款要求 reason 必填；后端裁决 403/501/5xx 状态。
-->
<script lang="ts">
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { enhance } from '$app/forms';
  import type { SubmitFunction } from '@sveltejs/kit';
  import { adminStateLabel } from '$lib/admin';
  import { currencyLabel, formatMoney, productKindLabel, productStatusLabel } from '$lib/api/client';
  import Button from '$lib/components/ui/Button.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import Dialog from '$lib/components/ui/Dialog.svelte';
  import BatchBar from '$lib/components/admin/BatchBar.svelte';
  import RowActionsMenu from '$lib/components/admin/RowActionsMenu.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import CosmeticAvatar from '$lib/components/wardrobe/CosmeticAvatar.svelte';
  import { toastActionResult } from '$lib/ui/action-toast';
  import QuickNicknameModal from '$lib/components/admin/shop/QuickNicknameModal.svelte';
  import SteamFramePricingModal from '$lib/components/admin/shop/SteamFramePricingModal.svelte';
  import SteamBackgroundPricingModal from '$lib/components/admin/shop/SteamBackgroundPricingModal.svelte';
  import CosmeticPreviewThumbnail from '$lib/components/admin/shop/CosmeticPreviewThumbnail.svelte';
  import steamBackgrounds from '$lib/data/steam-profile-backgrounds.json';
  import steamFrames from '$lib/data/steam-avatar-frames.json';
  import type { ShopProduct, ShopOrder, CosmeticDef } from '$lib/api/types';
  import type { AdminShopActionData, AdminShopPageData } from './+page.server';

  let { data, form }: { data: AdminShopPageData; form?: AdminShopActionData | null } = $props();

  /** 退款策略本地化（配置行展示用；表单 option 已是中文）。 */
  function refundPolicyLabel(policy: string): string {
    switch (policy) {
      case 'non_refundable':
        return '不可退款';
      case 'compensation_only':
        return '仅补偿';
      case 'full_refund':
        return '可退款';
      default:
        return policy;
    }
  }

  const products = $derived(data.products);
  const orders = $derived(data.orders);
  const config = $derived(data.config);
  const cosmetics = $derived(data.cosmetics ?? []);
  const message = $derived(form?.message ?? null);

  // ── 分类与视图切换状态（卡片展示 / 列表展示 + 类型 Tag 筛选）──
  type CategoryTab = 'all' | 'avatar' | 'space' | 'nickname' | 'other';
  let activeCategory = $state<CategoryTab>('all');
  let viewMode = $state<'grid' | 'list'>('grid');
  let searchQuery = $state('');

  $effect(() => {
    try {
      const saved = localStorage.getItem('bblbb_admin_shop_view_mode');
      if (saved === 'grid' || saved === 'list') {
        viewMode = saved;
      }
    } catch {}
  });

  function setViewMode(mode: 'grid' | 'list') {
    viewMode = mode;
    try {
      localStorage.setItem('bblbb_admin_shop_view_mode', mode);
    } catch {}
  }

  function getProductCategory(p: ShopProduct): CategoryTab {
    const k = p.kind as string;
    if (
      k === 'cosmetic_avatar' ||
      k === 'avatar_frame' ||
      p.slot === 'avatar_frame' ||
      p.slot === 'avatar_attachment' ||
      (p.presentation_tokens ?? []).some((t) => t.startsWith('avatar.frame.') || t.startsWith('avatar.attachment.'))
    ) {
      return 'avatar';
    }
    if (
      k === 'profile_effect' ||
      p.slot === 'profile_effect' ||
      (p.presentation_tokens ?? []).some((t) => t.startsWith('profile.effect.'))
    ) {
      return 'space';
    }
    if (
      k === 'cosmetic_nickname' ||
      k === 'nickname_color' ||
      p.slot === 'nickname' ||
      p.slot === 'nickname_color' ||
      (p.presentation_tokens ?? []).some((t) => t.startsWith('nickname.color.'))
    ) {
      return 'nickname';
    }
    return 'other';
  }

  function getCosmeticCategory(c: CosmeticDef): CategoryTab {
    if (c.kind === 'avatar_frame') return 'avatar';
    if (c.kind === 'profile_effect') return 'space';
    if (c.kind === 'nickname_color') return 'nickname';
    return 'other';
  }

  const productCounts = $derived.by(() => {
    const items = products.state === 'ok' ? products.items : [];
    const res: Record<CategoryTab, number> = { all: items.length, avatar: 0, space: 0, nickname: 0, other: 0 };
    for (const p of items) {
      const cat = getProductCategory(p);
      res[cat] = (res[cat] ?? 0) + 1;
    }
    return res;
  });

  const cosmeticCounts = $derived.by(() => {
    const res: Record<CategoryTab, number> = { all: cosmetics.length, avatar: 0, space: 0, nickname: 0, other: 0 };
    for (const c of cosmetics) {
      const cat = getCosmeticCategory(c);
      res[cat] = (res[cat] ?? 0) + 1;
    }
    return res;
  });

  const filteredProducts = $derived.by(() => {
    if (products.state !== 'ok') return [];
    return products.items.filter((p) => {
      if (activeCategory !== 'all' && getProductCategory(p) !== activeCategory) return false;
      if (searchQuery.trim()) {
        const q = searchQuery.trim().toLowerCase();
        return p.title.toLowerCase().includes(q) || p.slug.toLowerCase().includes(q);
      }
      return true;
    });
  });

  const filteredCosmetics = $derived.by(() => {
    return cosmetics.filter((c) => {
      if (activeCategory !== 'all' && getCosmeticCategory(c) !== activeCategory) return false;
      if (searchQuery.trim()) {
        const q = searchQuery.trim().toLowerCase();
        return c.name.toLowerCase().includes(q) || c.id.toLowerCase().includes(q);
      }
      return true;
    });
  });

  const categoryTabs = $derived.by(() => {
    const tabs: Array<{ id: CategoryTab; label: string; icon: string; count: number }> = [
      { id: 'all', label: '全部', icon: 'shopping-bag', count: productCounts.all },
      { id: 'avatar', label: 'Steam 动效头像框', icon: 'award', count: productCounts.avatar },
      { id: 'space', label: '个人资料背景', icon: 'sparkles', count: productCounts.space },
      { id: 'nickname', label: '彩色昵称', icon: 'wand-2', count: productCounts.nickname }
    ];
    if (productCounts.other > 0 || cosmeticCounts.other > 0) {
      tabs.push({ id: 'other', label: '其它装扮', icon: 'tag', count: productCounts.other });
    }
    return tabs;
  });


  // JS 启用：动作结果走全局 Toast 浮窗（成功绿/失败红）；顶部内联横幅仅保留为
  // 无 JS 回退（SSR HTML 仍渲染，见 hasJs）。
  let hasJs = $state(false);
  $effect(() => {
    hasJs = true;
  });


  /** 弹层表单共用结果处理：toast → update → 成功才关弹层（失败留在弹层改）。 */
  const dialogEnhance = (onSuccess: () => void): SubmitFunction =>
    () => async ({ result, update }) => {
      toastActionResult(result);
      await update();
      if (result.type === 'success') onSuccess();
    };

  // ── 批量选择（约定 B：商品行复选框 + BatchBar）──
  let selectedProductIds = $state<string[]>([]);
  const selectableProducts: ShopProduct[] = $derived(filteredProducts);
  let allProductsSelected = $derived(
    selectableProducts.length > 0 && selectableProducts.every((p) => selectedProductIds.includes(p.id))
  );
  function toggleAllProducts(): void {
    if (allProductsSelected) {
      const currentIds = new Set(selectableProducts.map((p) => p.id));
      selectedProductIds = selectedProductIds.filter((id) => !currentIds.has(id));
    } else {
      const merged = new Set([...selectedProductIds, ...selectableProducts.map((p) => p.id)]);
      selectedProductIds = Array.from(merged);
    }
  }
  function toggleProduct(id: string): void {
    if (selectedProductIds.includes(id)) selectedProductIds = selectedProductIds.filter((x) => x !== id);
    else selectedProductIds = [...selectedProductIds, id];
  }
  function productById(id: string): ShopProduct | undefined {
    return selectableProducts.find((p) => p.id === id);
  }

  // ── 弹层 target 状态（一个 Dialog 服务一类操作，target 区分行）──

  /**
   * 行操作弹层（M18-ADMIN-OPS 约定 D：每行一个「⋮」三点菜单，菜单项决定动作）。
   * 商品行动作 = 编辑（?/update，If-Match）/ 上架（?/publish）/ 下架（?/disable，
   * reason 必填）——不同端点 → Dialog 按 tab 渲染对应表单节。
   */
  let opsTarget = $state<ShopProduct | null>(null);
  let opsTab = $state<'edit' | 'publish' | 'disable'>('edit');

  /** 编辑分节表单草稿（?/update：If-Match version + reason）。 */
  let editTarget = $state<ShopProduct | null>(null);
  let editTitle = $state('');
  let editUnitPrice = $state('');
  let editStock = $state('');
  let editLevel = $state('');
  let editLimit = $state('');
  let editValidity = $state('');
  let editReason = $state('');

  /** 下架分节表单（?/disable：reason 必填写审计）。 */
  let disableReason = $state('');

  function openOps(p: ShopProduct, tab: 'edit' | 'publish' | 'disable'): void {
    opsTarget = p;
    opsTab = tab;
    // 预填编辑草稿（沿用既有 openEdit 预填逻辑）
    editTarget = p;
    editTitle = p.title;
    editUnitPrice = String(p.unit_price);
    editStock = p.stock_remaining == null ? '' : String(p.stock_remaining);
    editLevel = String(p.required_level ?? 1);
    editLimit = String(p.quantity_limit ?? 1);
    editValidity = p.validity_seconds == null ? '' : String(p.validity_seconds);
    editReason = '';
  }

  function closeOps(): void {
    opsTarget = null;
  }

  /** 行「⋮」菜单项（约定 D）：编辑 + 上架|下架（按商品状态二选一）。 */
  function rowActions(p: ShopProduct) {
    const actions: { label: string; danger?: boolean; run: () => void }[] = [
      { label: '编辑', run: () => openOps(p, 'edit') }
    ];
    if (p.status !== 'published') actions.push({ label: '上架', run: () => openOps(p, 'publish') });
    else actions.push({ label: '下架', danger: true, run: () => openOps(p, 'disable') });
    return actions;
  }

  /** Dialog 描述随菜单选定的动作变化（单动作确认，约定 D）。 */
  const opsDescription = $derived.by(() => {
    if (!opsTarget) return '';
    if (opsTab === 'publish') return `将「${opsTarget.title}」上架发布；发布后用户可见可购买。`;
    if (opsTab === 'disable') return `将「${opsTarget.title}」下架停售；停售原因写入审计日志。`;
    return `编辑「${opsTarget.title}」（${opsTarget.slug}，v${opsTarget.version}）；保存走 If-Match 乐观锁，操作原因写审计。`;
  });

  /** 订单退款（?/refund：amount 可空=全额，reason 必填）。 */
  let refundTarget = $state<ShopOrder | null>(null);
  let refundReason = $state('');
  function openRefund(o: ShopOrder): void {
    refundTarget = o;
    refundReason = '';
  }
  function closeRefund(): void {
    refundTarget = null;
  }

  /** 批量上架（?/batchPublish）/ 批量下架（?/batchDisable）。 */
  let batchPublishOpen = $state(false);
  let batchDisableOpen = $state(false);
  let batchDisableReason = $state('');
  function openBatchPublish(): void {
    batchPublishOpen = true;
  }
  function closeBatchPublish(): void {
    batchPublishOpen = false;
  }
  function openBatchDisable(): void {
    batchDisableReason = '';
    batchDisableOpen = true;
  }
  function closeBatchDisable(): void {
    batchDisableOpen = false;
  }

  function formatTs(ts: number | undefined): string {
    if (!ts) return '—';
    return new Date(ts).toLocaleString('zh-CN', { hour12: false });
  }

  // ── 装扮样式库（M07-SHOP-UI-10）：自己起名 + 可视化配置颜色/渐变/动效 ──


  /** 归档/恢复弹层（写审计原因）。 */
  let archiveTarget = $state<CosmeticDef | null>(null);
  let archiveStatus = $state<'active' | 'archived'>('archived');
  let archiveReason = $state('');
  function openStyleArchive(def: CosmeticDef, status: 'active' | 'archived'): void {
    archiveTarget = def;
    archiveStatus = status;
    archiveReason = '';
  }
  function closeStyleArchive(): void {
    archiveTarget = null;
  }



  /** 快捷上架弹窗状态：彩色昵称、Steam 头像框、Steam 资料背景。 */
  let nicknameModalOpen = $state(false);
  let steamFrameModalOpen = $state(false);
  let steamBgModalOpen = $state(false);

  function cosmeticKindLabel(kind: string): string {
    const labels: Record<string, string> = {
      nickname_color: '彩色昵称',
      avatar_frame: 'Steam 动效头像框',
      profile_effect: '个人资料背景'
    };
    return labels[kind] ?? kind;
  }

  /** 获取商品关联的装扮预览参数（优先匹配装扮样式库，回退至 Steam 预设） */
  function getProductPreview(p: ShopProduct): { kind: string; id: string; name: string; style: any } | null {
    for (const token of p.presentation_tokens ?? []) {
      if (token.startsWith('avatar.frame.') || token.startsWith('profile.effect.') || token.startsWith('nickname.color.')) {
        const id = token.split('.').pop();
        const matched = cosmetics.find((c) => c.id === id);
        if (matched) return matched;
      }
    }
    const byName = cosmetics.find((c) => c.name === p.title || p.title.includes(c.name) || c.name.includes(p.title));
    if (byName) return byName;

    if (p.kind === 'profile_effect' || p.slot === 'profile_effect') {
      const steamBg = (steamBackgrounds as any[]).find(
        (s) => s.name.toLowerCase() === p.title.toLowerCase() || p.title.toLowerCase().includes(s.name.toLowerCase())
      );
      if (steamBg) {
        return {
          id: steamBg.id,
          kind: 'profile_effect',
          name: steamBg.name,
          style: {
            mode: 'profile',
            appid: steamBg.appid,
            image: steamBg.image,
            webm: steamBg.webm ? `https://shared.fastly.steamstatic.com/community_assets/images/items/${steamBg.appid}/${steamBg.webm}` : undefined,
            mp4: steamBg.mp4 ? `https://shared.fastly.steamstatic.com/community_assets/images/items/${steamBg.appid}/${steamBg.mp4}` : undefined
          }
        };
      }
    } else if (p.kind === 'cosmetic_avatar' || p.slot === 'avatar_frame') {
      const steamFrame = (steamFrames as any[]).find(
        (s) => s.name.toLowerCase() === p.title.toLowerCase() || p.title.toLowerCase().includes(s.name.toLowerCase())
      );
      if (steamFrame) {
        return {
          id: steamFrame.id,
          kind: 'avatar_frame',
          name: steamFrame.name,
          style: {
            mode: 'steam_frame',
            appid: steamFrame.appid,
            image: steamFrame.image,
            url: `/api/v1/steam-assets/frames/${steamFrame.image}`
          }
        };
      }
    }
    return null;
  }
</script>

<svelte:head>
  <title>商城管理 — BBLBB</title>
</svelte:head>

<PageHeader title="商城管理" />

  {#if message && !hasJs}
    <p class="input-hint is-error" role="alert">{message}</p>
  {/if}

  {#if config.state === 'ok'}
    <div class="app-card" style="margin-bottom:var(--space-4);">
      <div class="card-body" style="display:flex;flex-wrap:wrap;gap:var(--space-4);align-items:center;">
        <span class="badge {config.data.enabled === false ? 'badge-warning' : 'badge-success'}">
          {config.data.enabled === false ? '商城停用' : '商城启用'}
        </span>
        <span class="text-secondary" style="font-size:var(--text-sm);">
          结算货币 {currencyLabel(config.data.currency_id ?? 'coin')} · 默认退款策略 {refundPolicyLabel(config.data.default_refund_policy ?? 'non_refundable')}
        </span>
      </div>
    </div>
  {/if}

  <!-- 商城类型 Tag 筛选与展示视图切换栏 -->
  <div class="app-card shop-filter-card" style="margin-bottom:var(--space-4);">
    <div class="shop-filter-bar">
      <!-- 分类筛选 Tags -->
      <div class="shop-filter-tags" role="tablist" aria-label="商品类型筛选">
        {#each categoryTabs as tab (tab.id)}
          <button
            type="button"
            role="tab"
            class="shop-filter-tag"
            class:is-active={activeCategory === tab.id}
            aria-selected={activeCategory === tab.id}
            onclick={() => (activeCategory = tab.id)}
          >
            <Icon name={tab.icon} size={14} />
            <span>{tab.label}</span>
            <span class="shop-filter-tag__count">{tab.count}</span>
          </button>
        {/each}
      </div>

      <!-- 右侧：搜索与视图模式切换 -->
      <div class="shop-filter-controls">
        <div class="shop-search-wrapper">
          <Icon name="search" size={14} class="shop-search-icon" />
          <input
            type="text"
            class="input-field shop-search-input"
            placeholder="搜索商品或样式..."
            bind:value={searchQuery}
            aria-label="搜索商品或样式"
          />
          {#if searchQuery}
            <button
              type="button"
              class="shop-search-clear"
              onclick={() => (searchQuery = '')}
              aria-label="清空搜索"
            >
              <Icon name="x" size={13} />
            </button>
          {/if}
        </div>

        <div class="view-mode-toggle" role="radiogroup" aria-label="展示方式">
          <button
            type="button"
            class="view-mode-btn"
            class:is-active={viewMode === 'grid'}
            aria-checked={viewMode === 'grid'}
            role="radio"
            onclick={() => setViewMode('grid')}
            title="卡片展示"
          >
            <Icon name="grid" size={14} />
            <span>卡片展示</span>
          </button>
          <button
            type="button"
            class="view-mode-btn"
            class:is-active={viewMode === 'list'}
            aria-checked={viewMode === 'list'}
            role="radio"
            onclick={() => setViewMode('list')}
            title="列表展示"
          >
            <Icon name="list" size={14} />
            <span>列表展示</span>
          </button>
        </div>
      </div>
    </div>
  </div>

  <!-- 装扮样式库（M07-SHOP-UI-10）：自己起名的自定义颜色/渐变/动效，商品可直接选用 -->
  <div class="app-card" style="margin-bottom:var(--space-4);">
    <div class="app-card__head" style="display:flex;align-items:center;justify-content:space-between;gap:8px;flex-wrap:wrap;">
      <h2 style="margin:0;">已生效装扮样式库（{filteredCosmetics.length}{#if activeCategory !== 'all' || searchQuery} / 全部 {cosmeticCounts.all}{/if}）</h2>
    </div>
    <div class="card-body" style="padding:0;">
      {#if filteredCosmetics.length === 0}
        <div style="padding:var(--space-4);">
          <EmptyState icon="palette" title={activeCategory !== 'all' || searchQuery ? '没有符合条件的装扮样式' : '暂无自定义样式：新建后即可在商品里选用自己的颜色、渐变与动效'} />
        </div>
      {:else if viewMode === 'grid'}
        <!-- 样式库卡片展示 -->
        <div class="shop-admin-card-grid">
          {#each filteredCosmetics as def (def.id)}
            <div class="shop-admin-card">
              <div class="shop-admin-card__head">
                <span class="badge badge-neutral">{cosmeticKindLabel(def.kind)}</span>
                <div style="display:flex;align-items:center;gap:6px;">
                  <span class="badge {def.status === 'active' ? 'badge-success' : 'badge-warning'}">
                    {def.status === 'active' ? '可用' : '已归档'}
                  </span>
                  <RowActionsMenu
                    label="更多操作：样式 {def.name}"
                    actions={[
                      def.status === 'active'
                        ? { label: '归档', danger: true, run: () => openStyleArchive(def, 'archived') }
                        : { label: '恢复', run: () => openStyleArchive(def, 'active') }
                    ]}
                  />
                </div>
              </div>

              <div class="shop-admin-card__preview">
                <CosmeticPreviewThumbnail
                  kind={def.kind}
                  id={def.id}
                  name={def.name}
                  style={def.style}
                  size="card"
                />
              </div>

              <div class="shop-admin-card__body">
                <strong class="shop-admin-card__title" title={def.name}>{def.name}</strong>
                <p class="text-secondary shop-admin-card__meta">
                  {def.id} · 更新于 {formatTs(def.updatedAt)}
                </p>
              </div>
            </div>
          {/each}
        </div>
      {:else}
        <!-- 样式库列表展示 -->
        <div style="display:flex;flex-direction:column;">
          {#each filteredCosmetics as def (def.id)}
            <div class="post-row" style="padding:var(--space-3);border-bottom:var(--border-default);">
              <div style="display:flex;gap:var(--space-3);align-items:center;flex-wrap:wrap;">
                <div style="flex:0 0 auto;display:flex;align-items:center;">
                  <CosmeticPreviewThumbnail
                    kind={def.kind}
                    id={def.id}
                    name={def.name}
                    style={def.style}
                    size="sm"
                  />
                </div>
                <div style="min-width:0;flex:1;">
                  <strong>{def.name}</strong>
                  <span class="badge badge-neutral" style="margin-left:var(--space-2);">{cosmeticKindLabel(def.kind)}</span>
                  <span class="badge {def.status === 'active' ? 'badge-success' : 'badge-warning'}">
                    {def.status === 'active' ? '可用' : '已归档'}
                  </span>
                  <p class="text-secondary" style="font-size:var(--text-xs);margin:2px 0 0;">
                    {def.id} · 更新于 {formatTs(def.updatedAt)}
                  </p>
                </div>
                <div style="display:flex;gap:var(--space-2);flex-wrap:wrap;align-items:center;">
                  <RowActionsMenu
                    label="更多操作：样式 {def.name}"
                    actions={[
                      def.status === 'active'
                        ? { label: '归档', danger: true, run: () => openStyleArchive(def, 'archived') }
                        : { label: '恢复', run: () => openStyleArchive(def, 'active') }
                    ]}
                  />
                </div>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>

  <div class="app-card" style="margin-bottom:var(--space-4);">
    <div class="app-card__head" style="display:flex;align-items:center;justify-content:space-between;gap:8px;flex-wrap:wrap;">
      <div>
        <h2 style="margin:0;">商品列表（{products.state === 'ok' ? filteredProducts.length : '—'}{#if activeCategory !== 'all' || searchQuery} / 全部 {productCounts.all}{/if}）</h2>
        <span class="text-secondary" style="font-size:var(--text-xs);">直接选品定价即可上架，无需复杂样式配置。成就徽章为社区荣誉专属，不可购买。</span>
      </div>
      <div style="display:flex;gap:var(--space-2);flex-wrap:wrap;">
        <Button text="新建商品：彩色昵称" variant="secondary" size="sm" onclick={() => (nicknameModalOpen = true)} />
        <Button text="上架 Steam 头像框 (2071款)" variant="primary" size="sm" onclick={() => (steamFrameModalOpen = true)} />
        <Button text="上架 Steam 资料背景 (1000款)" variant="secondary" size="sm" onclick={() => (steamBgModalOpen = true)} />
      </div>
    </div>
    <div class="card-body" style="padding:0;">
      {#if products.state !== 'ok'}
        <p class="input-hint is-error" role="alert" style="padding:var(--space-4);">
          {products.state === 'forbidden' || products.state === 'not_implemented' || products.state === 'error'
            ? adminStateLabel(products.state)
            : '加载失败'}
          {#if products.state === 'forbidden' || products.state === 'error' || products.state === 'not_implemented'}
            ：{products.message}
          {/if}
        </p>
      {:else if filteredProducts.length === 0}
        <div style="padding:var(--space-4);"><EmptyState icon="package" title={activeCategory !== 'all' || searchQuery ? '没有符合条件的商品' : '暂无商品'} /></div>
      {:else}
        <!-- 批量工具条（约定 B：选中后渲染） -->
        <div style="padding:10px 14px 0;">
          <BatchBar count={selectedProductIds.length} noun="件商品" onclear={() => (selectedProductIds = [])}>
            <Button text="批量上架" variant="secondary" size="sm" onclick={openBatchPublish} />
            <Button text="批量下架" variant="danger" size="sm" onclick={openBatchDisable} />
          </BatchBar>
        </div>
        <div class="shop-admin-select-bar">
          <label class="shop-admin-select-all">
            <input
              type="checkbox"
              checked={allProductsSelected}
              onchange={toggleAllProducts}
              aria-label="全选当前显示商品"
            />
            <span>全选当前（已选 {selectedProductIds.length} / 共 {filteredProducts.length} 件）</span>
          </label>
        </div>
        {#if viewMode === 'grid'}
          <!-- 商品卡片展示 -->
          <div class="shop-admin-card-grid">
            {#each filteredProducts as p (p.id)}
              {@const pPreview = getProductPreview(p)}
              <div class="shop-admin-card" class:is-selected={selectedProductIds.includes(p.id)}>
                <div class="shop-admin-card__head">
                  <span class="shop-admin-card__checkbox">
                    <input
                      type="checkbox"
                      checked={selectedProductIds.includes(p.id)}
                      onchange={() => toggleProduct(p.id)}
                      aria-label="选择商品 {p.title}"
                    />
                  </span>
                  <div style="display:flex;align-items:center;gap:6px;">
                    <span class="badge {p.status === 'published' ? 'badge-success' : p.status === 'disabled' ? 'badge-warning' : 'badge-neutral'}">
                      {productStatusLabel(p.status)}
                    </span>
                    <RowActionsMenu label="更多操作：商品 {p.title}" actions={rowActions(p)} />
                  </div>
                </div>

                <div class="shop-admin-card__preview">
                  {#if pPreview}
                    <CosmeticPreviewThumbnail
                      kind={pPreview.kind}
                      id={pPreview.id}
                      name={pPreview.name}
                      style={pPreview.style}
                      size="card"
                    />
                  {:else}
                    <div class="shop-admin-card__preview-empty">
                      <Icon name="package" size={32} />
                    </div>
                  {/if}
                </div>

                <div class="shop-admin-card__body">
                  <div class="shop-admin-card__title-row">
                    <strong class="shop-admin-card__title" title={p.title}>{p.title}</strong>
                    <span class="badge badge-neutral" style="flex:0 0 auto;">{productKindLabel(p.kind)}</span>
                  </div>
                  <div class="shop-admin-card__price-row">
                    <span class="shop-admin-card__price">
                      {formatMoney(p.unit_price, { id: p.currency_id, code: p.currency_code, name: p.currency_name }, { free: true })}
                    </span>
                    <span class="text-secondary" style="font-size:var(--text-xs);font-family:var(--aui-font-mono, monospace);">
                      v{p.version}
                    </span>
                  </div>
                  <p class="text-secondary shop-admin-card__meta">
                    {p.slug} · 更新于 {formatTs(p.updated_at)}
                  </p>
                </div>
              </div>
            {/each}
          </div>
        {:else}
          <!-- 商品列表展示 -->
          <div style="display:flex;flex-direction:column;">
            {#each filteredProducts as p (p.id)}
              {@const pPreview = getProductPreview(p)}
              <div class="post-row" style="padding:var(--space-3);border-bottom:var(--border-default);">
                <div style="display:flex;gap:var(--space-3);align-items:center;flex-wrap:wrap;">
                  <div style="flex:0 0 auto;">
                    <input
                      type="checkbox"
                      checked={selectedProductIds.includes(p.id)}
                      onchange={() => toggleProduct(p.id)}
                      aria-label="选择商品 {p.title}"
                    />
                  </div>
                  {#if pPreview}
                    <div style="flex:0 0 auto;display:flex;align-items:center;">
                      <CosmeticPreviewThumbnail
                        kind={pPreview.kind}
                        id={pPreview.id}
                        name={pPreview.name}
                        style={pPreview.style}
                        size="sm"
                      />
                    </div>
                  {/if}
                  <div style="min-width:0;flex:1;">
                    <strong>{p.title}</strong>
                    <span class="badge badge-neutral" style="margin-left:var(--space-2);">{productKindLabel(p.kind)}</span>
                    <span class="badge {p.status === 'published' ? 'badge-success' : p.status === 'disabled' ? 'badge-warning' : 'badge-neutral'}">
                      {productStatusLabel(p.status)}
                    </span>
                    <p class="text-secondary" style="font-size:var(--text-xs);margin:2px 0 0;">
                      {p.slug} · {formatMoney(p.unit_price, { id: p.currency_id, code: p.currency_code, name: p.currency_name }, { free: true })} · v{p.version} · 更新于 {formatTs(p.updated_at)}
                    </p>
                  </div>
                  <div style="display:flex;gap:var(--space-2);flex-wrap:wrap;">
                    <!-- 每行一个「⋮」三点菜单：编辑/上架|下架由菜单项决定动作（约定 D） -->
                    <RowActionsMenu label="更多操作：商品 {p.title}" actions={rowActions(p)} />
                  </div>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      {/if}
    </div>
  </div>

  <div class="app-card">
    <div class="app-card__head"><h2>订单（{orders.state === 'ok' ? orders.items.length : '—'}）</h2></div>
    <div class="card-body" style="padding:0;">
      {#if orders.state !== 'ok'}
        <p class="input-hint is-error" role="alert" style="padding:var(--space-4);">
          {orders.state === 'forbidden' || orders.state === 'not_implemented' || orders.state === 'error'
            ? adminStateLabel(orders.state)
            : '加载失败'}
        </p>
      {:else if orders.items.length === 0}
        <div style="padding:var(--space-4);"><EmptyState icon="inbox" title="暂无订单" /></div>
      {:else}
        <div style="display:flex;flex-direction:column;">
          {#each orders.items as o (o.id)}
            <div class="post-row" style="padding:var(--space-3);border-bottom:var(--border-default);">
              <div style="display:flex;gap:var(--space-3);align-items:center;flex-wrap:wrap;">
                <div style="min-width:0;flex:1;">
                  <strong>{o.product_title ?? o.product_id}</strong>
                  <span class="badge {o.status === 'succeeded' ? 'badge-success' : 'badge-neutral'}">{o.status}</span>
                  {#if o.entitlement_status === 'pending'}
                    <span class="badge badge-warning">补偿待处理</span>
                  {/if}
                  <p class="text-secondary" style="font-size:var(--text-xs);margin:2px 0 0;">
                    {o.id} · ×{o.quantity} · {formatMoney(o.total_amount, { id: o.currency_id, code: o.currency_code, name: o.currency_name }, { free: true })} · v{o.product_version} · {formatTs(o.created_at)}
                  </p>
                </div>
                {#if o.status === 'succeeded'}
                  <!-- 行内操作有且只有「⋯」菜单（约定 D）：退款收进菜单 -->
                  <RowActionsMenu
                    label="更多操作：订单 {o.id}"
                    actions={[{ label: '退款', run: () => openRefund(o) }]}
                  />
                {/if}
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>


  <!-- 行操作 Dialog（约定 D）：动作由「⋮」菜单项决定，弹层内单动作确认——
       编辑（?/update）/ 上架（?/publish）/ 下架（?/disable）按 opsTab 渲染对应表单节 -->
  <Dialog
    open={opsTarget !== null}
    title={opsTarget ? `商品操作：${opsTarget.title}` : '商品操作'}
    description={opsDescription}
    onclose={closeOps}
  >
    {#if opsTarget}
      {#if opsTab === 'edit'}
        <!-- 编辑商品（?/update：If-Match version + reason 审计） -->
        <form method="POST" action="?/update" use:enhance={dialogEnhance(closeOps)}>
          <input type="hidden" name="id" value={opsTarget?.id ?? ''} />
          <input type="hidden" name="version" value={opsTarget?.version ?? ''} />
          <div class="admin-form-grid" style="display:grid;grid-template-columns:repeat(auto-fit,minmax(160px,1fr));gap:var(--space-2);">
            <div class="input-wrapper">
              <label class="input-label" for="up-title">标题</label>
              <input id="up-title" name="title" class="input-field" bind:value={editTitle} />
            </div>
            <div class="input-wrapper">
              <label class="input-label" for="up-price">价格</label>
              <input id="up-price" name="unit_price" type="number" min="0" class="input-field" bind:value={editUnitPrice} />
            </div>
            <div class="input-wrapper">
              <label class="input-label" for="up-stock">库存（空=不限）</label>
              <input id="up-stock" name="stock_remaining" type="number" min="0" class="input-field" bind:value={editStock} />
            </div>
            <div class="input-wrapper">
              <label class="input-label" for="up-level">等级门槛</label>
              <input id="up-level" name="required_level" type="number" min="1" class="input-field" bind:value={editLevel} />
            </div>
            <div class="input-wrapper">
              <label class="input-label" for="up-limit">限购</label>
              <input id="up-limit" name="quantity_limit" type="number" min="1" class="input-field" bind:value={editLimit} />
            </div>
            <div class="input-wrapper">
              <label class="input-label" for="up-validity">有效期（秒）</label>
              <input id="up-validity" name="validity_seconds" type="number" min="0" class="input-field" bind:value={editValidity} />
            </div>
          </div>
           <!-- 装扮内容只读展示（M07-SHOP-STUDIO）：样式修改统一走装扮工作台 -->
           <div class="studio-readonly">
             <span class="input-label">装扮内容</span>
             {#if opsTarget.presentation_tokens && opsTarget.presentation_tokens.length > 0}
               <div class="studio-readonly__tokens">
                 {#each opsTarget.presentation_tokens as token (token)}
                   <code class="studio-readonly__token">{token}</code>
                 {/each}
               </div>
             {:else if opsTarget.asset_attachment_id}
               <p class="input-hint">自定义图片头像框（附件 {opsTarget.asset_attachment_id}）</p>
             {:else}
               <p class="input-hint">无装扮 Token（道具/消耗品）。</p>
             {/if}
           </div>
           <div class="input-wrapper" style="margin-top:var(--space-2);">
             <label class="input-label" for="up-reason">操作原因</label>
            <input id="up-reason" name="reason" class="input-field" required bind:value={editReason} placeholder="必填（写审计）" />
          </div>
          <Button text={opsTarget ? `保存（v${opsTarget.version + 1}）` : '保存'} variant="primary" size="sm" type="submit" />
        </form>
      {:else if opsTab === 'publish'}
        <!-- 上架（?/publish：server 契约无 reason/If-Match） -->
        <form method="POST" action="?/publish" use:enhance={dialogEnhance(closeOps)}>
          <input type="hidden" name="id" value={opsTarget?.id ?? ''} />
          <Button text="确认上架" variant="primary" size="sm" type="submit" />
        </form>
      {:else}
        <!-- 下架（?/disable：reason 必填写审计） -->
        <form method="POST" action="?/disable" use:enhance={dialogEnhance(closeOps)}>
          <input type="hidden" name="id" value={opsTarget?.id ?? ''} />
          <div class="input-wrapper" style="margin-bottom:var(--space-3);">
            <label class="input-label" for="sp-disable-reason">停售原因（写审计）</label>
            <input id="sp-disable-reason" name="reason" class="input-field" required bind:value={disableReason} placeholder="必填" />
          </div>
          <Button text="确认下架" variant="danger" size="sm" type="submit" />
        </form>
      {/if}
    {/if}
  </Dialog>

  <!-- 订单退款 Dialog：当前服务端只支持全额补偿，界面不暴露未实现的部分退款。 -->
  <Dialog
    open={refundTarget !== null}
    title="全额退款补偿"
    description={refundTarget ? `将对订单 ${refundTarget.id} 执行全额补偿，并撤销对应权益。` : ''}
    onclose={closeRefund}
  >
    <form method="POST" action="?/refund" use:enhance={dialogEnhance(closeRefund)}>
      <input type="hidden" name="id" value={refundTarget?.id ?? ''} />
      <div class="input-wrapper" style="margin-bottom:var(--space-3);">
        <label class="input-label" for="sp-refund-reason">退款原因（写审计）</label>
        <input id="sp-refund-reason" name="reason" class="input-field" required bind:value={refundReason} placeholder="必填" />
      </div>
      <Button text="确认退款" variant="primary" size="sm" type="submit" />
    </form>
  </Dialog>

  <!-- 批量上架 Dialog（?/batchPublish：循环 publish 单条端点） -->
  <Dialog
    open={batchPublishOpen}
    title="批量上架"
    description={`将发布 ${selectedProductIds.length} 件商品；发布后用户可见可购买。`}
    onclose={closeBatchPublish}
  >
    <form
      method="POST"
      action="?/batchPublish"
      use:enhance={() => async ({ result, update }) => {
        toastActionResult(result);
        await update();
        if (result.type === 'success') {
          selectedProductIds = [];
          closeBatchPublish();
        }
      }}
    >
      <input type="hidden" name="ids" value={selectedProductIds.join(',')} />
      <Button text={`批量上架 ${selectedProductIds.length} 件`} variant="primary" size="sm" type="submit" />
    </form>
  </Dialog>

  <!-- 批量下架 Dialog（?/batchDisable：同一原因逐条写审计） -->
  <Dialog
    open={batchDisableOpen}
    title="批量下架"
    description={`将停售 ${selectedProductIds.length} 件商品；原因将逐条写入审计。`}
    onclose={closeBatchDisable}
  >
    <form
      method="POST"
      action="?/batchDisable"
      use:enhance={() => async ({ result, update }) => {
        toastActionResult(result);
        await update();
        if (result.type === 'success') {
          selectedProductIds = [];
          closeBatchDisable();
        }
      }}
    >
      <input type="hidden" name="ids" value={selectedProductIds.join(',')} />
      <div class="input-wrapper" style="margin-bottom:var(--space-3);">
        <label class="input-label" for="sp-batch-disable-reason">停售原因（写审计）</label>
        <input id="sp-batch-disable-reason" name="reason" class="input-field" required bind:value={batchDisableReason} placeholder="必填" />
      </div>
      <Button text={`批量下架 ${selectedProductIds.length} 件`} variant="danger" size="sm" type="submit" />
    </form>
  </Dialog>


  <!-- 样式归档/恢复（写审计原因） -->
  <Dialog
    open={archiveTarget !== null}
    title={archiveStatus === 'archived' ? '归档样式' : '恢复样式'}
    description={
      archiveTarget
        ? archiveStatus === 'archived'
          ? `将「${archiveTarget.name}」归档：商城不可再选用；已购用户的权益会降级为默认样式。`
          : `将「${archiveTarget.name}」恢复为可用。`
        : ''
    }
    onclose={closeStyleArchive}
  >
    <form method="POST" action="?/updateCosmetic" use:enhance={dialogEnhance(closeStyleArchive)}>
      <input type="hidden" name="id" value={archiveTarget?.id ?? ''} />
      <input type="hidden" name="status" value={archiveStatus} />
      <input type="hidden" name="name" value={archiveTarget?.name ?? ''} />
      <div class="input-wrapper" style="margin-bottom:var(--space-3);">
        <label class="input-label" for="cs-archive-reason">操作原因（写审计）</label>
        <input id="cs-archive-reason" name="reason" class="input-field" required bind:value={archiveReason} placeholder="必填" />
      </div>
      <Button text={archiveStatus === 'archived' ? '确认归档' : '确认恢复'} variant={archiveStatus === 'archived' ? 'danger' : 'primary'} size="sm" type="submit" />
    </form>
  </Dialog>

  <QuickNicknameModal bind:open={nicknameModalOpen} enhanceHandler={dialogEnhance(() => (nicknameModalOpen = false))} />
  <SteamFramePricingModal bind:open={steamFrameModalOpen} enhanceHandler={dialogEnhance(() => (steamFrameModalOpen = false))} />
  <SteamBackgroundPricingModal bind:open={steamBgModalOpen} enhanceHandler={dialogEnhance(() => (steamBgModalOpen = false))} />

<style>
  .studio-readonly {
    margin-top: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border: 1px dashed var(--color-border, #ccc);
    border-radius: 8px;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    align-items: flex-start;
  }
  .studio-readonly__tokens { display: flex; flex-wrap: wrap; gap: 4px; }
  .studio-readonly__token {
    font-size: var(--text-xs, 11px);
    padding: 2px 6px;
    border-radius: 4px;
    background: var(--color-bg-subtle, #f5f5f5);
  }

  /* ── 商城分类 Tag 与视图切换工具条 ── */
  .shop-filter-card {
    padding: 0;
    overflow: hidden;
  }
  .shop-filter-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3, 12px);
    flex-wrap: wrap;
    padding: var(--space-3, 12px) var(--space-4, 16px);
  }
  .shop-filter-tags {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .shop-filter-tag {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 13px;
    border-radius: var(--radius-full, 9999px);
    border: 1px solid var(--border-default, #2e323b);
    background: var(--color-bg-subtle, rgba(255, 255, 255, 0.04));
    color: var(--color-text-secondary, #94a3b8);
    font-size: var(--text-xs, 12px);
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .shop-filter-tag:hover {
    color: var(--color-text-primary, #ffffff);
    background: var(--color-bg-subtle-hover, rgba(255, 255, 255, 0.08));
    border-color: var(--color-brand, #8b5cf6);
  }
  .shop-filter-tag.is-active {
    color: #ffffff;
    background: var(--color-brand, #8b5cf6);
    border-color: var(--color-brand, #8b5cf6);
    font-weight: 600;
  }
  .shop-filter-tag__count {
    display: inline-block;
    padding: 1px 6px;
    font-size: 11px;
    line-height: 1.2;
    border-radius: var(--radius-full, 9999px);
    background: rgba(0, 0, 0, 0.25);
    color: inherit;
  }
  .shop-filter-tag.is-active .shop-filter-tag__count {
    background: rgba(255, 255, 255, 0.25);
    color: #ffffff;
  }
  .shop-filter-controls {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }
  .shop-search-wrapper {
    position: relative;
    display: flex;
    align-items: center;
    min-width: 180px;
  }
  :global(.shop-search-icon) {
    position: absolute;
    left: 10px;
    color: var(--color-text-secondary, #64748b);
    pointer-events: none;
  }
  .shop-search-input {
    padding-left: 30px !important;
    padding-right: 28px !important;
    height: 32px !important;
    font-size: var(--text-xs, 12px) !important;
  }
  .shop-search-clear {
    position: absolute;
    right: 8px;
    background: none;
    border: none;
    color: var(--color-text-secondary, #64748b);
    cursor: pointer;
    display: flex;
    align-items: center;
    padding: 2px;
  }
  .view-mode-toggle {
    display: inline-flex;
    align-items: center;
    border: 1px solid var(--border-default, #2e323b);
    border-radius: var(--radius-sm, 6px);
    overflow: hidden;
    background: var(--color-bg-subtle, rgba(0, 0, 0, 0.2));
  }
  .view-mode-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border: none;
    background: transparent;
    color: var(--color-text-secondary, #94a3b8);
    font-size: var(--text-xs, 12px);
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .view-mode-btn:hover {
    color: var(--color-text-primary, #ffffff);
  }
  .view-mode-btn.is-active {
    background: var(--color-brand, #8b5cf6);
    color: #ffffff;
    font-weight: 600;
  }

  /* ── 卡片视图与选择条 ── */
  .shop-admin-select-bar {
    padding: 8px 14px;
    border-bottom: 1px solid var(--border-default, #2e323b);
    background: var(--color-bg-subtle, rgba(255, 255, 255, 0.02));
  }
  .shop-admin-select-all {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: var(--text-xs, 12px);
    color: var(--color-text-secondary, #94a3b8);
    cursor: pointer;
  }
  .shop-admin-card-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: var(--space-3, 12px);
    padding: var(--space-3, 14px);
  }
  .shop-admin-card {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border-default, #2e323b);
    border-radius: var(--radius-md, 8px);
    background: var(--color-bg-card, #181a20);
    padding: var(--space-3, 12px);
    gap: var(--space-2, 8px);
    transition: transform 0.15s ease, border-color 0.15s ease, box-shadow 0.15s ease;
  }
  .shop-admin-card:hover {
    border-color: var(--color-brand, #8b5cf6);
    box-shadow: 0 4px 14px rgba(0, 0, 0, 0.25);
  }
  .shop-admin-card.is-selected {
    border-color: var(--color-brand, #8b5cf6);
    background: color-mix(in srgb, var(--color-brand) 8%, var(--color-bg-card));
  }
  .shop-admin-card__head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
  }
  .shop-admin-card__checkbox {
    display: flex;
    align-items: center;
    cursor: pointer;
  }
  .shop-admin-card__preview {
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .shop-admin-card__preview-empty {
    width: 100%;
    height: 100px;
    border-radius: var(--radius-md, 8px);
    background: var(--color-bg-subtle, #1a1d24);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-text-secondary, #64748b);
  }
  .shop-admin-card__body {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  .shop-admin-card__title-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
  }
  .shop-admin-card__title {
    font-size: var(--text-sm, 13px);
    font-weight: 600;
    color: var(--color-text-primary, #ffffff);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .shop-admin-card__price-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    margin-top: 2px;
  }
  .shop-admin-card__price {
    font-size: var(--text-sm, 14px);
    font-weight: 700;
    color: var(--color-warning, #f59e0b);
    font-family: var(--aui-font-mono, monospace);
  }
  .shop-admin-card__meta {
    font-size: 11px;
    margin: 2px 0 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
