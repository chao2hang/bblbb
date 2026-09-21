<!-- M07-UI-05/06：衣柜——游戏化装备槽位与装扮背包系统：
  1. 角色装备台（纸娃娃装扮系统）：核心三大槽位（头像框、昵称特效、专属徽章三插槽）+ 场景气场槽；
  2. 徽章槽位全面关联成就系统（/achievements）：支持佩戴最多 3 枚已解锁成就徽章，支持一键佩戴/卸下；
  3. 点击装备槽位智能联动高亮并过滤右侧背包；
  4. 背包网格支持一键换装穿戴与卸下，自动替换槽位并支持版本乐观并发；
  5. 白名单 Token 投影与完整渐进增强（use:enhance + 无 JS 可用）。
-->
<script lang="ts">
  import { enhance } from '$app/forms';
  import {
    NICKNAME_COLORS,
    AVATAR_FRAMES,
    BADGES,
    PROFILE_EFFECTS,
    normalizeSlot,
    projectEntitlementTokens,
    slotLabel
  } from '$lib/components/wardrobe/tokens';
  import Button from '$lib/components/ui/Button.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import CosmeticAvatar from '$lib/components/wardrobe/CosmeticAvatar.svelte';
  import CosmeticName from '$lib/components/wardrobe/CosmeticName.svelte';
  import EntitlementPreview from '$lib/components/wardrobe/EntitlementPreview.svelte';
  import type { Entitlement } from '$lib/api/types';
  import type { WardrobeActionData, WardrobeAchievementBadge, WardrobePageData } from './+page.server';
  import PageTitle from '$lib/components/PageTitle.svelte';

  let { data, form }: { data: WardrobePageData; form?: WardrobeActionData | null } = $props();

  const presentation = $derived(data.presentation);
  const user = $derived(data.user);
  const entitlements = $derived(data.entitlements);
  const achievementBadges = $derived<WardrobeAchievementBadge[]>(data.achievementBadges ?? []);
  const achievementStats = $derived(data.achievementStats ?? { unlocked: 0, total: 0, equipped: 0, maxSlots: 3 });
  const error = $derived(data.error);
  const actionMessage = $derived(form?.message ?? null);
  const actionOk = $derived(form?.ok === true);
  const presentationVersion = $derived(presentation?.version ?? 1);

  // ── 当前包裹分类过滤 ──
  type SlotTab = 'all' | 'avatar_frame' | 'nickname_color' | 'profile_effect' | 'profile_badges';
  let activeTab = $state<SlotTab>('all');

  // ── 白名单 Token 提取（未知 Token 一律不渲染） ──
  const tokens = $derived<Record<string, string | string[] | null>>(
    presentation?.presentation_tokens ?? {}
  );

  const nicknameColor = $derived.by(() => {
    const v = tokens['nickname_color'];
    return typeof v === 'string' && v in NICKNAME_COLORS ? v : null;
  });
  const avatarFrame = $derived.by(() => {
    const v = tokens['avatar_frame'];
    return typeof v === 'string' && v in AVATAR_FRAMES ? AVATAR_FRAMES[v] : null;
  });
  const profileEffect = $derived.by(() => {
    const v = tokens['profile_effect'];
    return typeof v === 'string' && v in PROFILE_EFFECTS ? v : null;
  });

  // ── 已装备成就徽章（来自成就系统） ──
  const equippedAchievementBadges = $derived(
    achievementBadges.filter((b) => b.equipped)
  );

  /** 徽章最多 3 个（已佩戴达上限时禁用多余佩戴） */
  const equippedBadgeCount = $derived(
    Math.max(
      equippedAchievementBadges.length,
      presentation?.profile_badge_ids?.length ?? 0,
      entitlements.filter((e) => e.status === 'equipped' && normalizeSlot(e.slot) === 'profile_badges').length
    )
  );
  const badgesAtLimit = $derived(equippedBadgeCount >= 3);

  // ── 各装备槽位的当前已装备物品 ──
  const equippedFrame = $derived(
    entitlements.find((e) => e.status === 'equipped' && normalizeSlot(e.slot) === 'avatar_frame')
  );
  const equippedNickname = $derived(
    entitlements.find((e) => e.status === 'equipped' && normalizeSlot(e.slot) === 'nickname_color')
  );
  const equippedProfileEffect = $derived(
    entitlements.find((e) => e.status === 'equipped' && normalizeSlot(e.slot) === 'profile_effect')
  );

  const totalEquippedSlots = $derived(
    (equippedFrame ? 1 : 0) +
    (equippedNickname ? 1 : 0) +
    (equippedProfileEffect ? 1 : 0) +
    equippedAchievementBadges.length
  );

  // ── 包裹列表划分：只展示已拥有/已购买且未过期的有效装扮 ──
  const validActiveEntitlements = $derived(
    entitlements.filter(
      (e) =>
        (e.status === 'equipped' || (e.status === 'owned' && e.remaining_quantity > 0)) &&
        (!e.expires_at || e.expires_at > Date.now())
    )
  );

  /** 已解锁成就徽章（未达成的成就徽章不放入我的装扮） */
  const unlockedAchievementBadges = $derived(
    achievementBadges.filter((b) => b.unlocked)
  );

  /** 当前包裹过滤后的商城装扮项（头像框、昵称、背景） */
  const filteredCosmeticItems = $derived.by(() => {
    let list = validActiveEntitlements;
    if (activeTab === 'avatar_frame') {
      list = validActiveEntitlements.filter((e) => normalizeSlot(e.slot) === 'avatar_frame');
    } else if (activeTab === 'nickname_color') {
      list = validActiveEntitlements.filter((e) => normalizeSlot(e.slot) === 'nickname_color');
    } else if (activeTab === 'profile_effect') {
      list = validActiveEntitlements.filter((e) => normalizeSlot(e.slot) === 'profile_effect');
    } else if (activeTab === 'profile_badges') {
      list = validActiveEntitlements.filter((e) => normalizeSlot(e.slot) === 'profile_badges');
    }

    return list.slice().sort((a, b) => {
      if (a.status === 'equipped' && b.status !== 'equipped') return -1;
      if (b.status === 'equipped' && a.status !== 'equipped') return 1;
      return (b.created_at || 0) - (a.created_at || 0);
    });
  });

  /** 成就徽章列表（在「全部」或「专属徽章」分类下渲染：仅展示已获得徽章） */
  const filteredBadgeItems = $derived.by(() => {
    if (activeTab !== 'all' && activeTab !== 'profile_badges') {
      return [];
    }
    return unlockedAchievementBadges.slice().sort((a, b) => {
      if (a.equipped && !b.equipped) return -1;
      if (!a.equipped && b.equipped) return 1;
      return a.name.localeCompare(b.name, 'zh-CN');
    });
  });

  function selectTab(tab: SlotTab) {
    activeTab = tab;
  }

  function canEquip(e: Entitlement): boolean {
    if (e.status !== 'owned' && e.status !== 'equipped') return false;
    return true;
  }

  /** 权益 Token 投影（白名单可视化 + 中文标签；未知 Token 不渲染）。 */
  function previewOf(e: Entitlement) {
    const projection = projectEntitlementTokens(e.presentation_tokens, e.asset_attachment_id, e.slot, data.cosmetics);
    if ((normalizeSlot(e.slot) === 'profile_effect' || e.kind === 'profile_effect') && !projection.visual.profile_effect) {
      const title = e.product_title?.trim() ?? '';
      if (title) {
        const normalizedTitle = title.toLowerCase();
        const match = data.cosmetics
          .filter((c) => c.kind === 'profile_effect')
          .filter((c) => {
            const name = c.name.trim().toLowerCase();
            return name.length > 0 && (
              name === normalizedTitle ||
              (name.length >= 3 && normalizedTitle.length >= 3 && (normalizedTitle.includes(name) || name.includes(normalizedTitle)))
            );
          })
          .sort((a, b) => b.name.length - a.name.length)[0];
        if (match) {
          projection.visual.profile_effect = match.id;
          projection.visual.profile_effect_name = match.name;
          projection.visual.profile_effect_style = match.style;
          projection.labels.unshift(match.name);
        }
      }
    }
    return projection;
  }

  /** 行标题：商品名优先，缺失时回退 Token 中文标签，最后才是商品 id。 */
  function entitlementTitle(e: Entitlement, labels: string[]): string {
    const title = e.product_title?.trim();
    if (title) return title;
    return labels[0] ?? e.product_id;
  }

  function statusLabel(status: string): string {
    switch (status) {
      case 'owned':
        return '在背包中';
      case 'equipped':
        return '已穿戴';
      case 'expired':
        return '已过期';
      case 'revoked':
        return '已撤销';
      case 'consumed':
        return '已使用';
      default:
        return status;
    }
  }

  function expiring(e: Entitlement): string | null {
    if (!e.expires_at) return null;
    const left = e.expires_at - Date.now();
    if (left <= 0) return '已过期';
    const days = Math.ceil(left / 86400000);
    return days > 1 ? `剩余 ${days} 天` : `剩余 ${Math.max(1, Math.round(left / 3600000))} 小时`;
  }
</script>

<PageTitle title="我的衣橱" />

<div class="container page-content wardrobe-container" id="page-wardrobe">

  {#if error}
    <div class="alert alert-danger" role="alert" style="margin-bottom:var(--space-3);">
      {error}
    </div>
  {/if}
  {#if actionMessage}
    <div class="alert {actionOk ? 'alert-success' : 'alert-danger'}" role="alert" style="margin-bottom:var(--space-3);">
      {actionMessage}
    </div>
  {/if}

  <!-- 页面顶部概览横幅 -->
  <div class="wardrobe-header card">
    <div class="wardrobe-header-content">
      <div>
        <h1 class="wardrobe-page-title">
          <Icon name="palette" size={24} />
          <span>装扮工坊 · 衣橱</span>
        </h1>
        <p class="wardrobe-page-desc">
          类似游戏角色的穿戴配置：徽章关联成就系统，点击装备槽可快速筛选，亦可直接从右侧装扮背包中一键更换。
        </p>
      </div>
      <div class="wardrobe-header-actions">
        <a class="btn btn-secondary" href="/achievements">
          <Icon name="trophy" size={16} />
          <span>成就墙</span>
        </a>
        <a class="btn btn-primary" href="/shop">
          <Icon name="shopping-bag" size={16} />
          <span>装扮商城</span>
        </a>
      </div>
    </div>
  </div>

  <!-- 主布局：左侧角色装备台 + 右侧装扮包裹 -->
  <div class="wardrobe-layout">

    <!-- ──────────────── 左侧：角色装备面板（Equipment Station） ──────────────── -->
    <aside class="wardrobe-equipment-deck">

      <!-- 角色核心舞台卡片 -->
      <div class="card stage-card {profileEffect ? 'effect-' + profileEffect : ''}">
        <div class="stage-card-header">
          <span class="stage-tag">当前形象预览</span>
          <span class="badge badge-neutral">已装配 {totalEquippedSlots} 件</span>
        </div>

        <div class="stage-avatar-display">
          <div class="avatar-pedestal">
            <CosmeticAvatar
              name={user?.display_name || user?.username || "我"}
              size="2xl"
              presentation={presentation}
              avatarAttachmentId={user?.avatar_attachment_id}
              seed={user?.username ?? user?.id}
            />
          </div>
          <div class="stage-name-box">
            <CosmeticName
              name={user?.display_name || user?.username || "我的昵称"}
              presentation={presentation}
            />
            {#if user?.level}
              <span class="badge badge-neutral stage-level">LV.{user.level}</span>
            {/if}
          </div>

          <!-- 佩戴中的成就徽章陈列条 -->
          {#if equippedAchievementBadges.length > 0}
            <div class="stage-badges-row">
              {#each equippedAchievementBadges as badge}
                <span class="stage-badge-chip" title="{badge.name}：{badge.description}">
                  {#if badge.iconUrl}
                    <img src={badge.iconUrl} alt="" class="badge-mini-icon" />
                  {:else}
                    <span class="badge-fallback-icon">🎖</span>
                  {/if}
                  <span>{badge.name}</span>
                </span>
              {/each}
            </div>
          {:else}
            <span class="stage-empty-badges">未佩戴成就徽章</span>
          {/if}
        </div>
      </div>

      <!-- 核心装备插槽列表 -->
      <div class="card slots-card">
        <div class="slots-header">
          <span class="card-title">装备槽位（点击槽位换装）</span>
        </div>

        <div class="slots-list">
          <!-- 槽位 1：头像框 -->
          <div
            class="gear-slot-item {activeTab === 'avatar_frame' ? 'is-active' : ''} {equippedFrame ? 'is-filled' : 'is-empty'}"
            role="button"
            tabindex="0"
            onclick={() => selectTab('avatar_frame')}
            onkeydown={(e) => { if (e.key === 'Enter') selectTab('avatar_frame'); }}
          >
            <div class="gear-slot-left">
              {#if equippedFrame}
                {@const preview = previewOf(equippedFrame)}
                <EntitlementPreview projection={preview} iconToken={equippedFrame.icon_token} name="头" entitlementSlot={normalizeSlot(equippedFrame.slot)} compact={true} />
              {:else}
                <div class="empty-socket-icon"><Icon name="image" size={18} /></div>
              {/if}
            </div>
            <div class="gear-slot-body">
              <div class="gear-slot-type">
                <span>头像框</span>
                {#if activeTab === 'avatar_frame'}
                  <span class="slot-focus-tag">正在挑选</span>
                {/if}
              </div>
              <div class="gear-slot-name">
                {#if equippedFrame}
                  {@const preview = previewOf(equippedFrame)}
                  <strong>{entitlementTitle(equippedFrame, preview.labels)}</strong>
                {:else}
                  <span class="empty-hint">点击为头像挑选边框</span>
                {/if}
              </div>
            </div>
            <div class="gear-slot-action">
              {#if equippedFrame}
                <form method="POST" action="?/unequip" use:enhance onclick={(e) => e.stopPropagation()}>
                  <input type="hidden" name="entitlement_id" value={equippedFrame.id} />
                  <input type="hidden" name="expected_presentation_version" value={presentationVersion} />
                  <Button text="卸下" variant="ghost" size="sm" type="submit" />
                </form>
              {:else}
                <Icon name="chevron-right" size={14} class="slot-arrow" />
              {/if}
            </div>
          </div>

          <!-- 槽位 2：昵称特效 / 颜色 -->
          <div
            class="gear-slot-item {activeTab === 'nickname_color' ? 'is-active' : ''} {equippedNickname ? 'is-filled' : 'is-empty'}"
            role="button"
            tabindex="0"
            onclick={() => selectTab('nickname_color')}
            onkeydown={(e) => { if (e.key === 'Enter') selectTab('nickname_color'); }}
          >
            <div class="gear-slot-left">
              {#if equippedNickname}
                {@const preview = previewOf(equippedNickname)}
                <EntitlementPreview projection={preview} iconToken={equippedNickname.icon_token} name="字" entitlementSlot={normalizeSlot(equippedNickname.slot)} compact={true} />
              {:else}
                <div class="empty-socket-icon"><Icon name="palette" size={18} /></div>
              {/if}
            </div>
            <div class="gear-slot-body">
              <div class="gear-slot-type">
                <span>昵称特效</span>
                {#if activeTab === 'nickname_color'}
                  <span class="slot-focus-tag">正在挑选</span>
                {/if}
              </div>
              <div class="gear-slot-name">
                {#if equippedNickname}
                  {@const preview = previewOf(equippedNickname)}
                  <strong>{entitlementTitle(equippedNickname, preview.labels)}</strong>
                {:else}
                  <span class="empty-hint">点击挑选昵称流光 / 颜色</span>
                {/if}
              </div>
            </div>
            <div class="gear-slot-action">
              {#if equippedNickname}
                <form method="POST" action="?/unequip" use:enhance onclick={(e) => e.stopPropagation()}>
                  <input type="hidden" name="entitlement_id" value={equippedNickname.id} />
                  <input type="hidden" name="expected_presentation_version" value={presentationVersion} />
                  <Button text="卸下" variant="ghost" size="sm" type="submit" />
                </form>
              {:else}
                <Icon name="chevron-right" size={14} class="slot-arrow" />
              {/if}
            </div>
          </div>

          <!-- 槽位 3：主页装饰 -->
          <div
            class="gear-slot-item {activeTab === 'profile_effect' ? 'is-active' : ''} {equippedProfileEffect ? 'is-filled' : 'is-empty'}"
            role="button"
            tabindex="0"
            onclick={() => selectTab('profile_effect')}
            onkeydown={(e) => { if (e.key === 'Enter') selectTab('profile_effect'); }}
          >
            <div class="gear-slot-left">
              {#if equippedProfileEffect}
                {@const preview = previewOf(equippedProfileEffect)}
                <EntitlementPreview projection={preview} iconToken={equippedProfileEffect.icon_token} name="景" entitlementSlot={normalizeSlot(equippedProfileEffect.slot)} compact={true} />
              {:else}
                <div class="empty-socket-icon"><Icon name="sparkles" size={18} /></div>
              {/if}
            </div>
            <div class="gear-slot-body">
              <div class="gear-slot-type">
                <span>主页装饰</span>
                {#if activeTab === 'profile_effect'}
                  <span class="slot-focus-tag">正在挑选</span>
                {/if}
              </div>
              <div class="gear-slot-name">
                {#if equippedProfileEffect}
                  {@const preview = previewOf(equippedProfileEffect)}
                  <strong>{entitlementTitle(equippedProfileEffect, preview.labels)}</strong>
                {:else}
                  <span class="empty-hint">点击挑选个人主页全景装饰 / 背景</span>
                {/if}
              </div>
            </div>
            <div class="gear-slot-action">
              {#if equippedProfileEffect}
                <form method="POST" action="?/unequip" use:enhance onclick={(e) => e.stopPropagation()}>
                  <input type="hidden" name="entitlement_id" value={equippedProfileEffect.id} />
                  <input type="hidden" name="expected_presentation_version" value={presentationVersion} />
                  <Button text="卸下" variant="ghost" size="sm" type="submit" />
                </form>
              {:else}
                <Icon name="chevron-right" size={14} class="slot-arrow" />
              {/if}
            </div>
          </div>

          <!-- 槽位 4：专属勋章（关联成就页，内含 3 个镶嵌孔） -->
          <div
            class="gear-slot-item {activeTab === 'profile_badges' ? 'is-active' : ''} {equippedAchievementBadges.length > 0 ? 'is-filled' : 'is-empty'}"
            role="button"
            tabindex="0"
            onclick={() => selectTab('profile_badges')}
            onkeydown={(e) => { if (e.key === 'Enter') selectTab('profile_badges'); }}
          >
            <div class="gear-slot-left">
              <div class="empty-socket-icon"><Icon name="award" size={18} /></div>
            </div>
            <div class="gear-slot-body">
              <div class="gear-slot-type">
                <span>成就徽章（最多 3 枚）</span>
                {#if badgesAtLimit}
                  <span class="slot-limit-hint text-xs text-muted">徽章最多 3 个</span>
                {/if}
                <span class="badge badge-neutral" style="font-size:10px;">{equippedAchievementBadges.length}/3</span>
                {#if activeTab === 'profile_badges'}
                  <span class="slot-focus-tag">正在挑选</span>
                {/if}
              </div>
              <!-- 3 个微型孔位 -->
              <div class="badge-sockets-row">
                {#each [0, 1, 2] as idx}
                  {@const badge = equippedAchievementBadges[idx]}
                  {#if badge}
                    <div class="badge-socket is-filled" title="{badge.name}：{badge.description}">
                      {#if badge.iconUrl}
                        <img src={badge.iconUrl} alt="" class="badge-socket-img" />
                      {:else}
                        <span class="badge-socket-emoji">🎖</span>
                      {/if}
                      <form method="POST" action="?/unequip" use:enhance onclick={(e) => e.stopPropagation()}>
                        <input type="hidden" name="achievement_code" value={badge.code} />
                        <button type="submit" class="badge-socket-remove" title="卸下 {badge.name}" aria-label="卸下 {badge.name}">
                          <Icon name="x" size={10} />
                        </button>
                      </form>
                    </div>
                  {:else}
                    <div class="badge-socket is-empty" title="空徽章槽，点击右侧包裹佩戴">
                      <span>+{idx + 1}</span>
                    </div>
                  {/if}
                {/each}
              </div>
            </div>
            <div class="gear-slot-action">
              <Icon name="chevron-right" size={14} class="slot-arrow" />
            </div>
          </div>
        </div>
      </div>

    </aside>

    <!-- ──────────────── 右侧：装扮背包 / 储物包裹（Armory Inventory） ──────────────── -->
    <main class="wardrobe-inventory-deck">
      <div class="card inventory-card-wrap">

        <!-- 背包过滤标签栏（Tabs） -->
        <div class="inventory-toolbar">
          <div class="inventory-tabs" role="tablist" aria-label="装扮分类">
            <button
              type="button"
              role="tab"
              aria-selected={activeTab === 'all'}
              class="tab-btn {activeTab === 'all' ? 'is-active' : ''}"
              onclick={() => selectTab('all')}
            >
              全部装扮
              <span class="tab-count">{validActiveEntitlements.length + unlockedAchievementBadges.length}</span>
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={activeTab === 'profile_badges'}
              class="tab-btn {activeTab === 'profile_badges' ? 'is-active' : ''}"
              onclick={() => selectTab('profile_badges')}
            >
              <Icon name="award" size={14} />
              成就徽章
              {#if unlockedAchievementBadges.length > 0}
                <span class="tab-count">{unlockedAchievementBadges.length}</span>
              {/if}
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={activeTab === 'avatar_frame'}
              class="tab-btn {activeTab === 'avatar_frame' ? 'is-active' : ''}"
              onclick={() => selectTab('avatar_frame')}
            >
              头像框
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={activeTab === 'nickname_color'}
              class="tab-btn {activeTab === 'nickname_color' ? 'is-active' : ''}"
              onclick={() => selectTab('nickname_color')}
            >
              昵称特效
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={activeTab === 'profile_effect'}
              class="tab-btn {activeTab === 'profile_effect' ? 'is-active' : ''}"
              onclick={() => selectTab('profile_effect')}
            >
              主页装饰
            </button>
          </div>
        </div>

        <!-- 针对当前挑选槽位的状态小条 -->
        {#if activeTab !== 'all'}
          <div class="slot-filter-banner">
            <div class="banner-left">
              <Icon name="compass" size={14} />
              {#if activeTab === 'profile_badges'}
                <span>成就徽章库：已解锁 <strong>{unlockedAchievementBadges.length}/{achievementStats.total}</strong> · 正在佩戴 <strong>{equippedAchievementBadges.length}/3</strong></span>
              {:else}
                <span>正在浏览 <strong>{slotLabel(activeTab)}</strong> 槽位备选装扮</span>
              {/if}
            </div>
            <div class="banner-right">
              {#if activeTab === 'profile_badges'}
                <a class="banner-link" href="/achievements">
                  查看成就墙全部达成条件 <Icon name="chevron-right" size={12} />
                </a>
              {/if}
              <button type="button" class="btn-clear-filter" onclick={() => selectTab('all')}>
                全部包裹
              </button>
            </div>
          </div>
        {/if}

        <!-- 物品网格列表（RPG 道具卡片风格） -->
        <div class="inventory-body">
          {#if filteredCosmeticItems.length === 0 && filteredBadgeItems.length === 0}
            <div class="inventory-empty">
              <EmptyState
                icon="sparkles"
                title="背包里暂无此类装扮"
                desc="去商城选购装扮，或完成社区成就解锁荣誉徽章"
              />
              <div style="margin-top:var(--space-3);text-align:center;display:flex;justify-content:center;gap:var(--space-2);">
                <a class="btn btn-secondary btn-sm" href="/achievements">去成就墙解锁徽章</a>
                <a class="btn btn-primary btn-sm" href="/shop">逛逛装扮商城</a>
              </div>
            </div>
          {:else}
            <div class="inventory-grid">

              <!-- 1. 成就徽章道具卡片（在「全部」或「成就徽章」分类下优先展示） -->
              {#each filteredBadgeItems as badge (badge.code)}
                <div class="item-card item-card-badge {badge.equipped ? 'is-equipped' : ''}">
                  <div class="item-card-header">
                    <span class="item-slot-badge">成就 · {badge.category}</span>
                    {#if badge.equipped}
                      <span class="badge badge-success item-status-badge">
                        <Icon name="check" size={12} />
                        已佩戴
                      </span>
                    {:else}
                      <span class="badge badge-neutral item-status-badge">已解锁</span>
                    {/if}
                  </div>

                  <!-- 徽章居中预览 -->
                  <div class="item-card-preview">
                    {#if badge.iconUrl}
                      <img src={badge.iconUrl} alt={badge.name} class="badge-card-icon" />
                    {:else}
                      <div class="badge-card-placeholder is-active">
                        <Icon name="award" size={32} />
                      </div>
                    {/if}
                  </div>

                  <!-- 徽章信息 -->
                  <div class="item-card-info">
                    <h3 class="item-card-title" title={badge.name}>{badge.name}</h3>
                    <p class="badge-desc" title={badge.description}>{badge.description}</p>
                    <div class="item-card-meta">
                      <span class="meta-tag is-perm">永久成就</span>
                    </div>
                  </div>

                  <!-- 操作按钮：佩戴 / 卸下 -->
                  <div class="item-card-actions">
                    {#if badge.equipped}
                      <form method="POST" action="?/unequip" use:enhance style="width:100%;">
                        <input type="hidden" name="achievement_code" value={badge.code} />
                        <Button text="卸下徽章" variant="ghost" size="sm" type="submit" block />
                      </form>
                    {:else}
                      <form method="POST" action="?/equip" use:enhance style="width:100%;">
                        <input type="hidden" name="achievement_code" value={badge.code} />
                        <Button
                          text={badgesAtLimit ? '徽章已满 (3/3)' : '立即佩戴'}
                          variant="secondary"
                          size="sm"
                          type="submit"
                          disabled={badgesAtLimit}
                          block
                        />
                      </form>
                    {/if}
                  </div>
                </div>
              {/each}

              <!-- 2. 普通装扮道具卡片（头像框、昵称特效、背景装饰） -->
              {#each filteredCosmeticItems as e (e.id)}
                {@const preview = previewOf(e)}
                {@const isEquipped = e.status === 'equipped'}
                {@const itemTitle = entitlementTitle(e, preview.labels)}

                <div class="item-card {isEquipped ? 'is-equipped' : ''}">
                  <!-- 卡片顶部：角标状态 -->
                  <div class="item-card-header">
                    <span class="item-slot-badge">{slotLabel(normalizeSlot(e.slot))}</span>
                    {#if isEquipped}
                      <span class="badge badge-success item-status-badge">
                        <Icon name="check" size={12} />
                        已穿戴
                      </span>
                    {:else if expiring(e)}
                      <span class="badge badge-warning item-status-badge">{expiring(e)}</span>
                    {/if}
                  </div>

                  <!-- 卡片中央：道具预览舞台 -->
                  <div class="item-card-preview">
                    <EntitlementPreview
                      projection={preview}
                      iconToken={e.icon_token}
                      name={user?.display_name || user?.username || '我'}
                      title={e.product_title || ''}
                      entitlementSlot={
                         normalizeSlot(e.slot) || (e.kind === 'profile_effect' ? 'profile_effect' : null)
                       }
                    />
                  </div>

                  <!-- 卡片信息：名称与详情 -->
                  <div class="item-card-info">
                    <h3 class="item-card-title" title={itemTitle}>{itemTitle}</h3>
                    <div class="item-card-meta">
                      {#if expiring(e)}
                        <span class="meta-tag is-time">{expiring(e)}</span>
                      {:else}
                        <span class="meta-tag is-perm">永久持有</span>
                      {/if}
                      {#if normalizeSlot(e.slot) === 'profile_badges' && e.remaining_quantity > 1}
                        <span class="badge badge-neutral">×{e.remaining_quantity}</span>
                      {/if}
                    </div>
                  </div>

                  <!-- 卡片底部：操作按钮（直接装备或卸下） -->
                  <div class="item-card-actions">
                    {#if isEquipped}
                      <form method="POST" action="?/unequip" use:enhance style="width:100%;">
                        <input type="hidden" name="entitlement_id" value={e.id} />
                        <input type="hidden" name="expected_presentation_version" value={presentationVersion} />
                        <Button text="卸下" variant="ghost" size="sm" type="submit" block />
                      </form>
                    {:else}
                      <form method="POST" action="?/equip" use:enhance style="width:100%;">
                        <input type="hidden" name="entitlement_id" value={e.id} />
                        <input type="hidden" name="expected_presentation_version" value={presentationVersion} />
                        <Button
                          text="立即穿戴"
                          variant="secondary"
                          size="sm"
                          type="submit"
                          block
                        />
                      </form>
                    {/if}
                  </div>
                </div>
              {/each}

            </div>
          {/if}
        </div>
      </div>
    </main>

  </div>
</div>

<style>
  .wardrobe-container {
    padding-top: var(--space-4);
    padding-bottom: var(--space-6);
  }

  /* 顶部横幅 */
  .wardrobe-header {
    margin-bottom: var(--space-4);
    padding: var(--space-4) var(--space-5);
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
  }
  .wardrobe-header-content {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: var(--space-4);
    flex-wrap: wrap;
  }
  .wardrobe-page-title {
    margin: 0;
    font-size: var(--text-lg);
    font-weight: 700;
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .wardrobe-page-desc {
    margin: var(--space-1) 0 0;
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
  }
  .wardrobe-header-actions {
    display: flex;
    gap: var(--space-2);
  }

  /* 双栏游戏化布局：左侧装备台，右侧储物包裹 */
  .wardrobe-layout {
    display: grid;
    grid-template-columns: 360px 1fr;
    gap: var(--space-4);
    align-items: start;
  }
  @media (max-width: 900px) {
    .wardrobe-layout {
      grid-template-columns: 1fr;
    }
  }

  /* ──────────────── 左侧装备面板 ──────────────── */
  .wardrobe-equipment-deck {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  /* 角色舞台预览卡片 */
  .stage-card {
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    padding: var(--space-4);
    background: var(--color-bg-card);
    position: relative;
    overflow: hidden;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.03);
  }
  .stage-card-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--space-3);
  }
  .stage-tag {
    font-size: var(--text-xs);
    font-weight: 600;
    text-transform: uppercase;
    color: var(--color-text-secondary);
    letter-spacing: 0.5px;
  }
  .stage-avatar-display {
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: var(--space-3) 0 var(--space-2);
    text-align: center;
  }
  .avatar-pedestal {
    position: relative;
    display: inline-flex;
    margin-bottom: var(--space-3);
  }
  .stage-name-box {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    font-size: var(--text-lg);
    font-weight: 700;
  }
  .stage-level {
    font-size: 11px;
    padding: 1px 6px;
    border-radius: 999px;
  }
  .stage-badges-row {
    display: flex;
    justify-content: center;
    gap: var(--space-2);
    margin-top: var(--space-2);
    flex-wrap: wrap;
  }
  .stage-badge-chip {
    font-size: var(--text-xs);
    background: var(--color-bg-subtle, rgba(0,0,0,0.05));
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    padding: 2px 8px;
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .badge-mini-icon {
    width: 14px;
    height: 14px;
    border-radius: 2px;
    object-fit: cover;
  }
  .badge-fallback-icon {
    font-size: 12px;
  }
  .stage-empty-badges {
    font-size: var(--text-xs);
    color: var(--color-text-secondary);
    margin-top: var(--space-2);
  }

  /* 装备槽位卡片 */
  .slots-card {
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-bg-card);
    padding: var(--space-3) var(--space-4);
  }
  .slots-header {
    margin-bottom: var(--space-3);
    padding-bottom: var(--space-2);
    border-bottom: 1px solid var(--color-border);
  }
  .slots-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  /* 装备槽位 Item */
  .gear-slot-item {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-bg-card);
    cursor: pointer;
    transition: all 0.15s ease;
    user-select: none;
  }
  .gear-slot-item:hover {
    border-color: var(--color-border-strong);
    background: var(--color-bg-subtle);
  }
  .gear-slot-item.is-active {
    border-color: var(--color-brand);
    box-shadow: 0 0 0 1.5px var(--color-brand);
    background: rgba(var(--color-brand-rgb, 178,62,42), 0.03);
  }
  .gear-slot-item.is-empty {
    border-style: dashed;
  }
  .gear-slot-left {
    flex-shrink: 0;
    width: 36px;
    height: 36px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--radius-sm);
    overflow: hidden;
  }
  .gear-slot-left :global(.profile-preview-plate) {
    min-width: 0 !important;
    width: 36px !important;
    height: 36px !important;
    border-radius: var(--radius-sm) !important;
  }
  .gear-slot-left :global(.profile-preview-label) {
    display: none !important;
  }
  .gear-slot-left :global(.swatch) {
    width: 36px !important;
    height: 36px !important;
    border-radius: var(--radius-sm) !important;
  }
  .gear-slot-left :global(.name-plate) {
    max-width: 36px !important;
    overflow: hidden;
  }
  .empty-socket-icon {
    width: 34px;
    height: 34px;
    border-radius: var(--radius-sm);
    border: 1px dashed var(--color-border-strong);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-text-secondary);
    background: var(--color-bg-subtle);
  }
  .gear-slot-body {
    flex: 1;
    min-width: 0;
  }
  .gear-slot-type {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-xs);
    color: var(--color-text-secondary);
  }
  .slot-focus-tag {
    font-size: 10px;
    color: var(--color-brand);
    font-weight: 600;
  }
  .gear-slot-name {
    margin-top: 1px;
    font-size: var(--text-sm);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .empty-hint {
    color: var(--color-text-secondary);
    font-size: var(--text-xs);
  }
  .gear-slot-action {
    flex-shrink: 0;
  }
  .slot-arrow {
    color: var(--color-text-secondary);
    opacity: 0.6;
  }

  /* 徽章三孔位 */
  .badge-sockets-row {
    display: flex;
    gap: 6px;
    margin-top: 4px;
  }
  .badge-socket {
    width: 30px;
    height: 30px;
    border-radius: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    position: relative;
  }
  .badge-socket.is-filled {
    border: 1px solid var(--color-border-strong);
    background: var(--color-bg-subtle);
  }
  .badge-socket.is-empty {
    border: 1px dashed var(--color-border);
    font-size: 10px;
    color: var(--color-text-secondary);
  }
  .badge-socket-img {
    width: 24px;
    height: 24px;
    object-fit: contain;
    border-radius: 2px;
  }
  .badge-socket-emoji {
    font-size: 14px;
  }
  .badge-socket-remove {
    position: absolute;
    top: -5px;
    right: -5px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border-strong);
    color: var(--color-text-secondary);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    cursor: pointer;
  }
  .badge-socket-remove:hover {
    color: var(--color-danger, #cf222e);
    border-color: var(--color-danger, #cf222e);
  }

  /* ──────────────── 右侧装扮包裹（Inventory Grid） ──────────────── */
  .inventory-card-wrap {
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-bg-card);
    min-height: 500px;
  }
  .inventory-toolbar {
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--color-border);
  }
  .inventory-tabs {
    display: flex;
    gap: var(--space-2);
    overflow-x: auto;
    scrollbar-width: none;
  }
  .inventory-tabs::-webkit-scrollbar {
    display: none;
  }
  .tab-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: 999px;
    border: 1px solid var(--color-border);
    background: var(--color-bg-card);
    font-size: var(--text-sm);
    cursor: pointer;
    white-space: nowrap;
    color: var(--color-text-secondary);
    transition: all 0.15s ease;
  }
  .tab-btn:hover {
    background: var(--color-bg-subtle);
    color: var(--color-text-primary);
  }
  .tab-btn.is-active {
    background: var(--color-brand);
    color: #fff;
    border-color: var(--color-brand);
  }
  .tab-count {
    font-size: 11px;
    padding: 0 5px;
    border-radius: 10px;
    background: rgba(0, 0, 0, 0.1);
  }
  .tab-btn.is-active .tab-count {
    background: rgba(255, 255, 255, 0.25);
  }

  /* 槽位筛选提示横幅 */
  .slot-filter-banner {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 8px var(--space-4);
    background: var(--color-bg-subtle);
    border-bottom: 1px solid var(--color-border);
    font-size: var(--text-xs);
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .banner-left {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .banner-right {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  .banner-link {
    color: var(--color-brand);
    text-decoration: none;
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: var(--text-xs);
  }
  .banner-link:hover {
    text-decoration: underline;
  }
  .btn-clear-filter {
    background: none;
    border: none;
    color: var(--color-text-secondary);
    cursor: pointer;
    font-size: var(--text-xs);
    text-decoration: underline;
    padding: 0;
  }

  /* 背包物品网格 */
  .inventory-body {
    padding: var(--space-4);
  }
  .inventory-empty {
    padding: var(--space-6) 0;
  }
  .inventory-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: var(--space-3);
  }

  /* 单个装扮道具卡片（类似游戏物品框） */
  .item-card {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-bg-card);
    padding: var(--space-3);
    position: relative;
    transition: transform 0.15s ease, box-shadow 0.15s ease, border-color 0.15s ease;
  }
  .item-card:hover {
    transform: translateY(-2px);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.05);
    border-color: var(--color-border-strong);
  }
  .item-card.is-equipped {
    border-color: var(--color-success, #1a7f37);
    box-shadow: 0 0 0 1px var(--color-success, #1a7f37);
    background: rgba(26, 127, 55, 0.02);
  }
  .item-card.is-locked {
    opacity: 0.6;
    background: var(--color-bg-subtle);
  }
  .item-card.is-history {
    opacity: 0.65;
    background: var(--color-bg-subtle);
  }

  .item-card-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--space-2);
    min-height: 20px;
  }
  .item-slot-badge {
    font-size: 11px;
    color: var(--color-text-secondary);
    background: var(--color-bg-subtle);
    padding: 1px 6px;
    border-radius: 4px;
    border: 1px solid var(--color-border);
  }
  .item-status-badge {
    font-size: 11px;
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }

  .item-card-preview {
    height: 72px;
    display: flex;
    align-items: center;
    justify-content: center;
    margin: var(--space-1) 0;
  }
  .badge-card-icon {
    width: 48px;
    height: 48px;
    object-fit: contain;
    border-radius: 4px;
  }
  .badge-card-placeholder {
    width: 48px;
    height: 48px;
    border-radius: 50%;
    background: var(--color-bg-subtle);
    border: 1px dashed var(--color-border-strong);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-text-secondary);
  }
  .badge-card-placeholder.is-active {
    border-color: var(--color-brand);
    color: var(--color-brand);
    background: rgba(var(--color-brand-rgb, 178,62,42), 0.05);
  }

  .item-card-info {
    flex: 1;
    margin-bottom: var(--space-2);
    display: flex;
    flex-direction: column;
    align-items: center;
  }
  .item-card-title {
    margin: 0;
    font-size: var(--text-sm);
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: center;
    max-width: 100%;
  }
  .badge-desc {
    margin: 4px 0 0;
    font-size: 11px;
    color: var(--color-text-secondary);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    text-align: center;
    min-height: 28px;
    line-height: 1.3;
  }
  .item-card-meta {
    display: flex;
    justify-content: center;
    align-items: center;
    gap: 6px;
    margin-top: 4px;
    font-size: var(--text-xs);
  }
  .meta-tag.is-perm {
    color: var(--color-text-secondary);
  }
  .meta-tag.is-time {
    color: var(--color-warning, #bf8700);
  }
  .meta-tag.is-locked-hint {
    color: var(--color-text-secondary);
  }
  .meta-tag.is-history {
    color: var(--color-text-secondary);
  }

  .item-card-actions {
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .item-history-hint {
    font-size: var(--text-xs);
    color: var(--color-text-secondary);
  }
</style>
