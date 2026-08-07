<!-- M07-UI-01：余额/等级/经验/签到状态安全投影。
  签到为每日首次有效页面访问自动领取；本页同时提供显式领取按钮
  （原生 form + 隐藏幂等键，429 时展示冷却提示）。
-->
<script lang="ts">
  import { enhance } from '$app/forms';
  import { newClientRequestId } from '$lib/api/client';
  import Button from '$lib/components/ui/Button.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import type { BalanceActionData, BalancePageData } from './+page.server';

  let { data, form }: { data: BalancePageData; form?: BalanceActionData | null } = $props();

  const summary = $derived(data.summary);
  const error = $derived(data.error);
  const message = $derived(form?.message ?? null);
  const retryAfter = $derived(form?.retryAfterSecs ?? null);

  const idempotencyKey = $state(newClientRequestId());

  const coinBalance = $derived((summary?.balances ?? []).find((b) => b.currency === 'coin'));
  const expBalance = $derived((summary?.balances ?? []).find((b) => b.currency === 'exp'));
  const xp = $derived(summary?.xp ?? 0);
  const xpToNext = $derived(summary?.xp_to_next ?? null);
  const todayEarned = $derived(summary?.today_earned ?? form?.todayEarned ?? []);
  const streak = $derived(summary?.streak_days ?? form?.streakDays ?? 0);
  const checkedIn = $derived(summary?.checked_in_today ?? false);

  function pct(current: number, total: number | null | undefined): number {
    if (!total || total <= 0) return 0;
    return Math.min(100, Math.round((current / (current + total)) * 100));
  }

  function coinLabel(m: { currency: string; amount: number } | undefined): string {
    return m ? `${m.amount} ${m.currency.toUpperCase()}` : '—';
  }
</script>

<svelte:head>
  <title>我的积分 — BBLBB</title>
</svelte:head>

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <a href="/me" class="breadcrumb-link">我的主页</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">我的积分</span>
  </nav>

  {#if error}
    <p class="input-hint is-error" role="alert">{error}</p>
  {/if}
  {#if message}
    <p class="input-hint is-error" role="alert">{message}</p>
  {/if}

  {#if summary}
    <div class="content-grid">
      <div class="main-col">
        <div class="card" style="margin-bottom:var(--space-4);">
          <div class="card-header">
            <span class="card-title">等级与经验</span>
            <span class="badge badge-level">LV.{summary.level}</span>
          </div>
          <div class="card-body">
            {#if summary.level_name}
              <p class="text-secondary">{summary.level_name}</p>
            {/if}
            <div style="display:flex;align-items:center;gap:var(--space-3);margin-top:var(--space-2);">
              <Icon name="trending-up" size={20} />
              <div style="flex:1;">
                <div
                  class="xp-bar"
                  role="progressbar"
                  aria-label="经验进度"
                  aria-valuenow={pct(xp, xpToNext)}
                  aria-valuemin={0}
                  aria-valuemax={100}
                >
                  <div class="xp-bar-fill" style="width:{pct(xp, xpToNext)}%;"></div>
                </div>
              </div>
              <span class="text-secondary" style="font-size:var(--text-sm);">
                {xpToNext ? `${xp} / ${xp + xpToNext} 经验` : '已满级'}
              </span>
            </div>
          </div>
        </div>

        <div class="card" style="margin-bottom:var(--space-4);">
          <div class="card-header"><span class="card-title">余额</span></div>
          <div class="card-body">
            <div class="balance-grid" style="display:grid;grid-template-columns:repeat(auto-fit,minmax(160px,1fr));gap:var(--space-3);">
              <div class="balance-card">
                <div class="balance-label">B 币（可消费）</div>
                <div class="balance-value">{coinLabel(coinBalance)}</div>
              </div>
              <div class="balance-card">
                <div class="balance-label">经验（等级来源）</div>
                <div class="balance-value">{expBalance ? coinLabel(expBalance) : `${xp}`}</div>
              </div>
            </div>
            <div style="margin-top:var(--space-3);display:flex;gap:var(--space-2);">
              <Button text="去商城" variant="primary" size="sm" icon="shopping-bag" href="/shop" />
            </div>
          </div>
        </div>

        <div class="card">
          <div class="card-header"><span class="card-title">今日奖励</span></div>
          <div class="card-body">
            {#if todayEarned.length === 0}
              <p class="auth-hint">今日暂无已入账奖励。</p>
            {:else}
              <ul style="list-style:none;margin:0;padding:0;display:flex;flex-direction:column;gap:var(--space-2);">
                {#each todayEarned as earned (earned.currency + earned.amount)}
                  <li style="display:flex;align-items:center;gap:var(--space-2);">
                    <Icon name="check-circle" size={16} />
                    <span>+{earned.amount} {earned.currency.toUpperCase()}</span>
                  </li>
                {/each}
              </ul>
            {/if}
          </div>
        </div>
      </div>

      <div class="side-col">
        <div class="card">
          <div class="card-header"><span class="card-title">签到</span></div>
          <div class="card-body">
            <p class="auth-hint">
              每日首次有效页面访问会自动签到；这里也可以手动领取。
            </p>
            <p>
              <span class="badge {checkedIn ? 'badge-success' : 'badge-warning'}">
                {checkedIn ? '今日已签到' : '今日未签到'}
              </span>
              <span class="text-secondary" style="margin-left:var(--space-2);font-size:var(--text-sm);">
                连续签到 {streak} 天
              </span>
            </p>
            {#if retryAfter}
              <p class="input-hint is-error" role="alert">操作过于频繁，请约 {retryAfter} 秒后再试。</p>
            {/if}
            <form method="POST" action="?/visit" use:enhance style="margin-top:var(--space-3);">
              <input type="hidden" name="client_request_id" value={idempotencyKey} />
              <Button
                text={checkedIn ? '今日已签到' : '立即签到'}
                variant={checkedIn ? 'secondary' : 'primary'}
                size="md"
                type="submit"
                disabled={checkedIn}
              />
            </form>
            <p class="input-hint" style="margin-top:var(--space-2);">
              签到奖励按你的时区自然日计算，重复打开页面不会重复发放。
            </p>
          </div>
        </div>
      </div>
    </div>
  {:else if !error}
    <p class="input-hint" role="status">加载中…</p>
  {/if}
</div>

<style>
  .xp-bar {
    height: 10px;
    border-radius: 5px;
    background: var(--color-border, #d0d7de);
    overflow: hidden;
  }
  .xp-bar-fill {
    height: 100%;
    background: var(--color-primary, #0969da);
    transition: width 0.3s ease;
  }
  .balance-card {
    border: 1px solid var(--color-border, #d0d7de);
    border-radius: var(--radius-md, 8px);
    padding: var(--space-3);
  }
  .balance-label {
    font-size: var(--text-xs, 12px);
    color: var(--color-text-secondary, #666);
  }
  .balance-value {
    font-size: var(--text-lg, 18px);
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    margin-top: 2px;
  }
</style>
