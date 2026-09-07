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

<svelte:head>
  <title>成就墙 — BBLBB</title>
</svelte:head>

<div class="container page-content">
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

    <!-- M18-ACH-02：4 个筛选 tabs（全部 / 进行中 / 已解锁 / 隐藏，带计数） -->
    <div class="tabs" role="tablist" style="margin-top:var(--space-4);">
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

    <!-- 成就卡片网格 -->
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
      <div class="achievement-grid" style="margin-top:var(--space-4);">
        {#each displayedCards as card (card.code)}
          <div class="card achievement-card" class:is-locked={!card.unlocked}>
            <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-3);">
              <div style="display:flex;align-items:center;gap:var(--space-2);flex-wrap:wrap;">
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
              <p class="text-secondary" style="margin:0;font-size:var(--text-sm);">{card.description}</p>
              {#if card.unlocked}
                <p class="input-hint" style="margin:0;">
                  解锁于 {formatTime(toSeconds(card.unlockedAt))}
                </p>
              {:else if card.target > 0}
                <!-- 进行中：进度条 progress/target -->
                <div class="points-progress" role="progressbar" aria-valuenow={pct(card)} aria-valuemin={0} aria-valuemax={100} aria-label="成就进度">
                  <span style="width:{pct(card)}%;"></span>
                </div>
                <p class="input-hint" style="margin:0;">{card.progress}/{card.target}</p>
              {/if}
              <div style="display:flex;align-items:center;justify-content:space-between;gap:var(--space-3);">
                <span class="text-secondary" style="font-size:var(--text-sm);">
                  +{card.rewardExp} 经验 · +{card.rewardCoin} 积分
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

  .achievement-grid {
    margin-top: var(--space-5);
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: var(--space-4);
    align-items: start;
  }

  .achievement-card.is-locked {
    opacity: 0.85;
  }

  .achievement-name {
    font-weight: var(--weight-medium);
    font-size: var(--text-md);
  }

  .achievement-name.is-unknown {
    letter-spacing: 2px;
  }
</style>
