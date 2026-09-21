<script lang="ts">
  // GAP-FIX（社交域·成就）：/achievements——成就墙 SSR。
  //
  // - 总览卡：已解锁 X/Y + 装备 n/maxSlots + 总进度条；
  // - 成就卡片网格：已解锁（含解锁时间/装备按钮/已装备徽章）、进行中
  //   （进度条）、隐藏未解锁（???）；
  // - equip/unequip：form action + use:enhance → toast + invalidateAll；
  // - 无 JS 基线：原生 form POST 整页刷新，form.message 状态行可见。
  import { enhance } from '$app/forms';
  import { invalidateAll } from '$app/navigation';
  import type { SubmitFunction } from '@sveltejs/kit';
  import Button from '$lib/components/ui/Button.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import LoadFailureState from '$lib/components/LoadFailureState.svelte';
  import ProblemState from '$lib/components/ProblemState.svelte';
  import StatCard from '$lib/components/ui/StatCard.svelte';
  import Badge from '$lib/components/ui/Badge.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { isTransientProblem } from '$lib/errors';
  import { announceTransientProblem } from '$lib/ui/problem-toast';
  import { show } from '$lib/ui/toast';
  import { formatTime } from '$lib/utils';
  import type {
    AchievementsActionData,
    AchievementsPageData,
    AchievementCard
  } from './+page.server';
  import PageTitle from '$lib/components/PageTitle.svelte';

  let { data, form }: {
    data: AchievementsPageData;
    form?: AchievementsActionData | null;
  } = $props();

  // 瞬态服务端错误（5xx/429）→ 全局 Toast 提示 + 页面只留「加载失败·重试」
  // 占位（产品约定：不整页展示错误态）；持续性错误仍走 ProblemState。
  $effect(() => {
    void data.problem;
    announceTransientProblem(data.problem);
  });

  const actionMessage = $derived(form?.message ?? null);
  const overallPct = $derived(
    data.stats.total > 0 ? Math.round((data.stats.unlocked / data.stats.total) * 100) : 0
  );

  /** M18-ACH-02：筛选 tab（全部 / 进行中 / 已解锁 / 隐藏，对齐原型）。 */
  type AchFilter = 'all' | 'in_progress' | 'unlocked' | 'hidden';
  let activeFilter = $state<AchFilter>('all');

  /** 视图展示方式：卡片展示（grid）或 列表展示（list），默认卡片展示。 */
  type AchViewMode = 'grid' | 'list';
  let viewMode = $state<AchViewMode>('grid');

  $effect(() => {
    try {
      const urlParam = new URLSearchParams(window.location.search).get('view');
      if (urlParam === 'list') {
        viewMode = 'list';
        return;
      } else if (urlParam === 'grid' || urlParam === 'card') {
        viewMode = 'grid';
        return;
      }
      const saved = localStorage.getItem('bblbb_achievements_view_mode');
      if (saved === 'grid' || saved === 'list') {
        viewMode = saved;
      }
    } catch {}
  });

  function setViewMode(mode: AchViewMode) {
    viewMode = mode;
    try {
      localStorage.setItem('bblbb_achievements_view_mode', mode);
    } catch {}
  }

  const equippedCards = $derived(data.cards.filter((c) => c.equipped));
  const unlockedCards = $derived(data.cards.filter((c) => c.unlocked));
  const inProgressCards = $derived(data.cards.filter((c) => !c.unlocked && (!c.isHidden || c.progress > 0)));
  const hiddenCards = $derived(data.cards.filter((c) => c.isHidden));

  const filterTabs = $derived<Array<{ key: AchFilter; label: string; count: number }>>([
    { key: 'all', label: '全部', count: data.cards.length },
    { key: 'in_progress', label: '进行中', count: inProgressCards.length },
    { key: 'unlocked', label: '已解锁', count: unlockedCards.length },
    { key: 'hidden', label: '隐藏', count: hiddenCards.length }
  ]);

  const displayedCards = $derived.by(() => {
    switch (activeFilter) {
      case 'in_progress':
        return inProgressCards;
      case 'unlocked':
        return unlockedCards;
      case 'hidden':
        return hiddenCards;
      default:
        return data.cards;
    }
  });

  /** 后端时间戳为毫秒（M01-DB-08），formatTime 口径为秒。 */
  function toSeconds(ts: number | null | undefined): number | null {
    if (typeof ts !== 'number' || !Number.isFinite(ts) || ts <= 0) return null;
    return ts > 1e11 ? Math.floor(ts / 1000) : ts;
  }

  /** 单卡进度百分比（target 0 = 无进度行，不渲染）。 */
  function pct(card: AchievementCard): number {
    if (card.target <= 0) return 0;
    return Math.min(100, Math.round((card.progress / card.target) * 100));
  }

  /** equip/unequip 共用 enhance 回调（按成功文案区分）。 */
  function enhanceHandler(successText: string): SubmitFunction {
    return () => {
      return async ({ result, update }) => {
        if (result.type === 'success') {
          show(successText, 'success');
          await update();
          await invalidateAll();
        } else {
          if (result.type === 'failure') {
            show(
              String((result.data as AchievementsActionData | undefined)?.message ?? '操作失败'),
              'danger'
            );
          }
          await update();
        }
      };
    };
  }
</script>

  <PageTitle title="成就墙" />

<div class="container page-content" id="page-achievements">
  <!-- 原型对齐（prototype/pages/achievements.html）：仅 sr-only h1，无可见页头、无面包屑。 -->
  <h1 class="sr-only" tabindex="-1">成就墙</h1>

  {#if data.problem && isTransientProblem(data.problem)}
    <LoadFailureState onretry={() => void invalidateAll()} />
  {:else if data.problem}
    <ProblemState problem={data.problem} />
  {:else}
    <!-- 总览卡：已解锁 / 装备槽 + 总进度 -->
    <div class="card">
      <div class="card-header"><span class="card-title">成就总览</span></div>
      <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-4);">
        {#if actionMessage}
          <p class="input-hint" role="status" style="margin:0;">{actionMessage}</p>
        {/if}
        <div class="stats-row">
          <StatCard label="已解锁" value="{data.stats.unlocked}/{data.stats.total}" />
          <StatCard label="已装备徽章" value="{data.stats.equipped}/{data.stats.maxSlots}" />
          <StatCard label="总进度" value="{overallPct}%" />
        </div>
        <div class="points-progress" role="progressbar" aria-valuenow={overallPct} aria-valuemin={0} aria-valuemax={100} aria-label="成就总进度">
          <span style="width:{overallPct}%;"></span>
        </div>
        <p class="input-hint" style="margin:0;">
          在个人资料页展示已装备的成就徽章；最多可同时装备 {data.stats.maxSlots} 枚。
        </p>
      </div>
    </div>

    <!-- M18-ACH-02：正在装备区块（原型同款 3 槽位：已装备卡片 + 虚线空槽） -->
    <div class="card" style="margin-top:var(--space-4);">
      <div class="card-header" style="display:flex;align-items:center;justify-content:space-between;">
        <span class="card-title">正在装备</span>
        <span class="app-muted" style="font-size:var(--text-xs);">{equippedCards.length}/{data.stats.maxSlots} 槽位</span>
      </div>
      <div class="card-body">
        <div style="display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:var(--space-2);">
          {#each equippedCards as eq (eq.code)}
            <div style="border:var(--border-default);border-radius:var(--radius-md);padding:var(--space-2);text-align:center;background:var(--color-bg-subtle);">
              <Icon name="award" size={24} />
              <div style="font-size:var(--text-xs);font-weight:600;margin-top:4px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">{eq.name}</div>
            </div>
          {/each}
          {#each Array(Math.max(0, data.stats.maxSlots - equippedCards.length)) as _}
            <div style="border:1px dashed var(--color-border-strong);border-radius:var(--radius-md);padding:var(--space-3);text-align:center;color:var(--color-text-tertiary);display:flex;flex-direction:column;align-items:center;justify-content:center;min-height:70px;">
              <Icon name="plus" size={18} />
              <span style="font-size:var(--text-xs);margin-top:2px;">空槽位</span>
            </div>
          {/each}
        </div>
      </div>
    </div>

    <!-- M18-ACH-02：4 个筛选 tabs（全部 / 进行中 / 已解锁 / 隐藏，带计数）与卡片/列表展示切换 -->
    <div class="achievement-toolbar">
      <div class="tabs" role="tablist" aria-label="成就状态筛选">
        {#each filterTabs as t (t.key)}
          <button
            type="button"
            role="tab"
            aria-selected={activeFilter === t.key ? 'true' : 'false'}
            class="tab {activeFilter === t.key ? 'is-active' : ''}"
            onclick={() => (activeFilter = t.key)}
          >
            {t.label} ({t.count})
          </button>
        {/each}
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

    <!-- 成就卡片网格 / 列表 -->
    {#if data.cards.length === 0}
      <div class="card" style="margin-top:var(--space-4);">
        <div class="card-body">
          <EmptyState icon="award" title="暂无成就" desc="成就系统还没有配置成就，敬请期待" />
        </div>
      </div>
    {:else if displayedCards.length === 0}
      <div class="card" style="margin-top:var(--space-4);">
        <div class="card-body">
          <EmptyState icon="award" title="没有匹配的成就" desc="换个筛选 tab 看看其他成就吧" />
        </div>
      </div>
    {:else}
      <div
        class="achievement-grid"
        class:is-grid={viewMode === 'grid'}
        class:is-list={viewMode === 'list'}
        data-view={viewMode}
        style="margin-top:var(--space-4);"
      >
        {#each displayedCards as card (card.code)}
          <div
            class="card achievement-card"
            class:is-locked={!card.unlocked}
            data-code={card.code}
          >
            <div class="card-body achievement-card__body">
              <div class="achievement-card__head">
                {#if card.iconUrl && !(card.isHidden && !card.unlocked)}
                  <!-- 后台上传的成就图标（本地磁盘存储，不走 S3）；隐藏且未解锁
                       不展示，避免提前泄露成就配置。 -->
                  <img
                    src={card.iconUrl}
                    alt="{card.name} 图标"
                    width="44"
                    height="44"
                    loading="lazy"
                    class="achievement-icon-img"
                  />
                {:else if card.isHidden && !card.unlocked}
                  <div class="achievement-icon-box is-locked" aria-hidden="true">
                    <Icon name="lock" size={20} />
                  </div>
                {:else}
                  <div
                    class="achievement-icon-box"
                    class:is-unlocked={card.unlocked}
                    class:is-equipped={card.equipped}
                    aria-hidden="true"
                  >
                    {#if card.equipped}
                      <Icon name="award" size={20} />
                    {:else}
                      <Icon name="trophy" size={20} />
                    {/if}
                  </div>
                {/if}
                <div class="achievement-card__title-wrap">
                  <div class="achievement-card__title-row">
                    <span class="achievement-name" class:is-unknown={card.isHidden && !card.unlocked}>
                      {card.name}
                    </span>
                    {#if card.equipped}
                      <Badge text="已装备" type="success" />
                    {:else if card.unlocked}
                      <Badge text="已解锁" type="pinned" />
                    {:else if card.isHidden}
                      <Badge text="隐藏" type="neutral" />
                    {/if}
                  </div>
                  {#if card.unlocked}
                    <p class="input-hint achievement-unlock-time" style="margin:2px 0 0;">
                      解锁于 {formatTime(toSeconds(card.unlockedAt))}
                    </p>
                  {/if}
                </div>
              </div>

              <p class="text-secondary achievement-desc">{card.description}</p>

              {#if !card.unlocked && card.target > 0}
                <!-- 进行中：进度条 progress/target -->
                <div class="achievement-progress-wrap">
                  <div
                    class="points-progress"
                    role="progressbar"
                    aria-valuenow={pct(card)}
                    aria-valuemin={0}
                    aria-valuemax={100}
                    aria-label="成就进度"
                  >
                    <span style="width:{pct(card)}%;"></span>
                  </div>
                  <p class="input-hint achievement-progress-text" style="margin:2px 0 0;">{card.progress}/{card.target}</p>
                </div>
              {/if}

              <div class="achievement-card__foot">
                <span class="text-secondary achievement-reward">
                  +{card.rewardCoin} 积分
                </span>
                {#if card.unlocked}
                  {#if card.equipped}
                    <form
                      method="POST"
                      action="?/unequip"
                      use:enhance={enhanceHandler('徽章已卸下')}
                    >
                      <input type="hidden" name="code" value={card.code} />
                      <Button type="submit" text="卸下" variant="ghost" size="sm" />
                    </form>
                  {:else}
                    <form
                      method="POST"
                      action="?/equip"
                      use:enhance={enhanceHandler('徽章已装备')}
                    >
                      <input type="hidden" name="code" value={card.code} />
                      <Button type="submit" text="装备" variant="secondary" size="sm" />
                    </form>
                  {/if}
                {/if}
              </div>
            </div>
          </div>
        {/each}
      </div>
    {/if}

    <!-- M18-ACH-02：解锁说明信息卡（对齐原型同款说明） -->
    <div class="card" style="margin-top:var(--space-4);">
      <div class="card-header"><span class="card-title">解锁说明</span></div>
      <div class="card-body">
        <p class="text-secondary" style="margin:0;font-size:var(--text-sm);line-height:1.6;">
          成就进度由服务端事件流自动判定；隐藏成就不会提前泄露解锁条件；撤销后历史进度仍会安全保留。
        </p>
      </div>
    </div>
  {/if}
</div>

<style>
  .stats-row {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
    gap: var(--space-3);
  }

  /* ── 视图工具栏 ── */
  .achievement-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    margin-top: var(--space-4);
    flex-wrap: wrap;
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
    color: var(--on-brand, #ffffff);
    font-weight: 600;
  }

  /* ── 卡片网格展示 ── */
  .achievement-grid.is-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: var(--space-4);
    align-items: stretch;
    background: transparent;
    border: none;
    padding: 0;
  }

  .achievement-grid.is-grid .achievement-card {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--color-bg-card);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    box-shadow: 0 2px 8px color-mix(in srgb, var(--color-bg-page) 40%, transparent);
    transition: transform var(--duration-fast, 0.15s) ease, border-color var(--duration-fast, 0.15s) ease, box-shadow var(--duration-fast, 0.15s) ease;
  }

  .achievement-grid.is-grid .achievement-card:hover {
    transform: translateY(-2px);
    border-color: var(--color-brand);
    box-shadow: 0 10px 24px color-mix(in srgb, var(--color-brand) 12%, transparent);
  }

  .achievement-grid.is-grid .achievement-card__body {
    display: flex;
    flex-direction: column;
    height: 100%;
    padding: var(--space-4);
    gap: var(--space-3);
  }

  .achievement-grid.is-grid .achievement-desc {
    margin: 0;
    font-size: var(--text-sm);
    min-height: 38px;
    line-height: 1.5;
  }

  .achievement-grid.is-grid .achievement-card__foot {
    margin-top: auto;
    padding-top: var(--space-3);
    border-top: 1px dashed var(--color-border-muted);
  }

  /* ── 列表展示 ── */
  .achievement-grid.is-list {
    display: grid;
    grid-template-columns: 1fr;
    gap: 0;
    padding: 0 var(--space-4);
    border: var(--border-default);
    background: var(--color-bg-card);
    border-radius: var(--radius-lg);
  }

  .achievement-grid.is-list .achievement-card {
    background: transparent;
    border: 0;
    border-bottom: 1px solid var(--color-border-muted);
    border-radius: 0;
    box-shadow: none;
  }

  .achievement-grid.is-list .achievement-card:last-child {
    border-bottom: 0;
  }

  .achievement-grid.is-list .achievement-card__body {
    padding: var(--space-4) 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .achievement-grid.is-list .achievement-desc {
    margin: 0;
    font-size: var(--text-sm);
  }

  /* ── 通用卡片内结构 ── */
  .achievement-card__head {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
  }

  .achievement-icon-img {
    width: 44px;
    height: 44px;
    border-radius: var(--radius-md, 10px);
    object-fit: cover;
    border: 1px solid var(--color-border);
    background: var(--color-bg-subtle);
    flex-shrink: 0;
  }

  .achievement-icon-box {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    flex-shrink: 0;
    border-radius: var(--radius-md, 10px);
    border: 1px solid var(--color-border);
    background: var(--color-bg-subtle);
    color: var(--color-text-secondary);
    transition: all 0.2s ease;
  }

  .achievement-icon-box.is-unlocked {
    background: color-mix(in srgb, var(--color-brand) 12%, var(--color-bg-card));
    border-color: color-mix(in srgb, var(--color-brand) 30%, var(--color-border));
    color: var(--color-brand);
  }

  .achievement-icon-box.is-equipped {
    background: var(--color-brand);
    color: var(--on-brand, #ffffff);
    border-color: var(--color-brand);
  }

  .achievement-icon-box.is-locked {
    opacity: 0.6;
    color: var(--color-text-tertiary);
  }

  .achievement-card__title-wrap {
    flex: 1;
    min-width: 0;
  }

  .achievement-card__title-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }

  .achievement-name {
    font-weight: var(--weight-medium, 600);
    font-size: var(--text-md, 15px);
    color: var(--color-text-primary);
  }

  .achievement-name.is-unknown {
    letter-spacing: 2px;
  }

  .achievement-unlock-time {
    font-size: var(--text-xs);
  }

  .achievement-card__foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
  }

  .achievement-reward {
    font-size: var(--text-sm);
    font-weight: 500;
  }

  .achievement-card.is-locked {
    opacity: 0.85;
  }

  .achievement-progress-wrap {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .achievement-progress-text {
    font-size: var(--text-xs);
  }

  @media (max-width: 767px) {
    .achievement-toolbar {
      flex-direction: column;
      align-items: stretch;
    }

    .achievement-toolbar .tabs {
      overflow-x: auto;
      flex-wrap: nowrap;
    }

    .view-mode-toggle {
      align-self: flex-start;
    }

    .achievement-grid.is-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
