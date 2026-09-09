<!-- M07-UI-08：管理端活跃——签到/任务配置（版本冲突提示、审计 reason 必填）。
-->
<script lang="ts">
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { enhance } from '$app/forms';
  import { adminStateLabel } from '$lib/admin';
  import { withActionToast } from '$lib/ui/action-toast';
  import Button from '$lib/components/ui/Button.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import type { ActivityTask } from '$lib/api/types';
  import type { AdminActivityPageData } from './+page.server';

  let { data, form }: { data: AdminActivityPageData; form?: { message?: string } | null } = $props();

  const config = $derived(data.config);
  const tasks = $derived(data.tasks);
  const message = $derived(form?.message ?? null);

  // JS 启用：动作结果走全局 Toast 浮窗（成功绿/失败红）；顶部内联横幅仅保留为
  // 无 JS 回退（SSR HTML 仍渲染，见 hasJs）。
  let hasJs = $state(false);
  $effect(() => {
    hasJs = true;
  });

  const TASK_KINDS = ['check_in', 'task', 'reaction', 'post', 'comment', 'leaderboard'] as const;

  const RESET_HOURS = [
    { hour: 0, label: '00:00（每日零点刷新，默认）' },
    { hour: 1, label: '01:00（凌晨 1 点）' },
    { hour: 2, label: '02:00（凌晨 2 点）' },
    { hour: 3, label: '03:00（凌晨 3 点）' },
    { hour: 4, label: '04:00（凌晨 4 点刷新，适合夜猫子社区）' },
    { hour: 5, label: '05:00（凌晨 5 点刷新）' },
    { hour: 6, label: '06:00（早晨 6 点刷新）' },
    { hour: 7, label: '07:00（早晨 7 点）' },
    { hour: 8, label: '08:00（早晨 8 点）' },
    { hour: 9, label: '09:00（上午 9 点）' },
    { hour: 12, label: '12:00（中午 12 点）' }
  ];

  const TIMEZONES = [
    { value: 'Asia/Shanghai', label: 'Asia/Shanghai (中国标准时间 UTC+8)' },
    { value: 'UTC', label: 'UTC (世界协调时间)' },
    { value: 'Asia/Tokyo', label: 'Asia/Tokyo (日本标准时间 UTC+9)' },
    { value: 'Asia/Hong_Kong', label: 'Asia/Hong_Kong (香港时间 UTC+8)' },
    { value: 'America/New_York', label: 'America/New_York (美东时间 UTC-5)' },
    { value: 'America/Los_Angeles', label: 'America/Los_Angeles (美西时间 UTC-8)' },
    { value: 'Europe/London', label: 'Europe/London (伦敦时间 UTC+0)' }
  ];

  function kindLabel(kind: string): string {
    const map: Record<string, string> = {
      check_in: '签到',
      task: '任务',
      reaction: '反应',
      post: '发帖',
      comment: '评论',
      leaderboard: '榜单'
    };
    return map[kind] ?? kind;
  }
</script>

<svelte:head>
  <title>签到与活跃管理 — BBLBB</title>
</svelte:head>

<PageHeader title="签到与活跃管理" />

<div class="container page-content">

  {#if message && !hasJs}
    <p class="input-hint is-error" role="alert" style="margin-bottom:var(--space-3);">{message}</p>
  {/if}

  {#if config.state === 'ok'}
    {@const currentTz = config.data.site_timezone || 'Asia/Shanghai'}
    {@const currentResetHour = config.data.day_reset_hour ?? config.data.check_in?.day_reset_hour ?? 0}
    {@const currentCurrency = config.data.check_in?.currency || (config.data.check_in_reward?.currency === 'exp' ? 'exp' : 'coin')}
    {@const currentAmount = config.data.check_in?.amount ?? config.data.check_in_reward?.amount ?? 10}
    {@const currentDailyLimit = config.data.check_in?.daily_limit ?? config.data.check_in_daily_limit ?? 1}
    {@const isAutoEnabled = config.data.auto_check_in_enabled !== false && config.data.check_in?.auto_enabled !== false}
    {@const isCheckInEnabled = config.data.check_in_enabled !== false && config.data.check_in?.enabled !== false}

    <div class="app-card" style="margin-bottom:var(--space-5);">
      <div class="app-card__head" style="display:flex;align-items:center;justify-content:space-between;gap:12px;flex-wrap:wrap;">
        <div>
          <h2>签到功能与规则配置（v{config.data.version}）</h2>
          <p class="text-secondary" style="font-size:var(--text-xs);margin:2px 0 0;">配置全站每日签到开关、自动打卡模式、跨天结算周期与奖励发放</p>
        </div>
        <span class="badge {isCheckInEnabled ? 'badge-success' : 'badge-neutral'}">
          {isCheckInEnabled ? '签到功能运行中' : '签到已关闭'}
        </span>
      </div>
      <div class="app-card__body">
        <form method="POST" action="?/save-config" use:enhance={withActionToast()}>
          <input type="hidden" name="expected_version" value={config.data.version} />

          <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(280px,1fr));gap:var(--space-4);margin-bottom:var(--space-4);">
            <!-- 开关 1：是否可以签到 -->
            <div style="padding:14px;border:1px solid var(--color-border);border-radius:var(--radius-md);background:var(--color-bg-subtle);">
              <label class="input-label" style="display:flex;align-items:center;gap:8px;font-weight:600;cursor:pointer;">
                <input type="checkbox" name="check_in_enabled" checked={isCheckInEnabled} style="width:16px;height:16px;" />
                开启用户每日签到
              </label>
              <p class="text-secondary" style="font-size:var(--text-xs);margin:6px 0 0;line-height:1.5;">
                全站签到总开关。关闭后，用户将无法进行签到打卡，签到奖励暂停发放。
              </p>
            </div>

            <!-- 开关 2：是否登录后自动签到 -->
            <div style="padding:14px;border:1px solid var(--color-border);border-radius:var(--radius-md);background:var(--color-bg-subtle);">
              <label class="input-label" style="display:flex;align-items:center;gap:8px;font-weight:600;cursor:pointer;">
                <input type="checkbox" name="auto_check_in_enabled" checked={isAutoEnabled} style="width:16px;height:16px;" />
                登录/访问后自动签到
              </label>
              <p class="text-secondary" style="font-size:var(--text-xs);margin:6px 0 0;line-height:1.5;">
                开启后，用户每日首次登录或浏览社区时自动签到并入账；关闭后，用户须前往个人中心手动点击“立即签到”。
              </p>
            </div>

            <!-- 开关 3：全站奖励开关 -->
            <div style="padding:14px;border:1px solid var(--color-border);border-radius:var(--radius-md);background:var(--color-bg-subtle);">
              <label class="input-label" style="display:flex;align-items:center;gap:8px;font-weight:600;cursor:pointer;">
                <input type="checkbox" name="rewards_enabled" checked={config.data.rewards_enabled !== false} style="width:16px;height:16px;" />
                启用全站奖励发放
              </label>
              <p class="text-secondary" style="font-size:var(--text-xs);margin:6px 0 0;line-height:1.5;">
                控制签到、发帖、回复等所有活跃积分/经验的实时入账总闸。
              </p>
            </div>
          </div>

          <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(240px,1fr));gap:var(--space-3);margin-bottom:var(--space-4);">
            <!-- 从什么时间算新的一天 -->
            <div class="input-wrapper">
              <label class="input-label" for="ac-reset-hour">
                从什么时间算新的一天（跨天刷新点）
              </label>
              <select id="ac-reset-hour" name="day_reset_hour" class="input-field">
                {#each RESET_HOURS as opt}
                  <option value={opt.hour} selected={currentResetHour === opt.hour}>{opt.label}</option>
                {/each}
              </select>
              <small class="text-secondary" style="display:block;margin-top:4px;font-size:11px;">
                例如设为 04:00 时，凌晨 3:59 签到仍算前一天，早晨 4:00 起方进入新一天的签到周期。
              </small>
            </div>

            <!-- 站点基准时区 -->
            <div class="input-wrapper">
              <label class="input-label" for="ac-timezone">
                站点基准时区
              </label>
              <select id="ac-timezone" name="site_timezone" class="input-field">
                {#each TIMEZONES as tz}
                  <option value={tz.value} selected={currentTz === tz.value}>{tz.label}</option>
                {/each}
              </select>
              <small class="text-secondary" style="display:block;margin-top:4px;font-size:11px;">
                跨天时间按此基准时区计算（默认 Asia/Shanghai，UTC+8）。
              </small>
            </div>

            <!-- 签到奖励币种与数额 -->
            <div class="input-wrapper">
              <label class="input-label" for="ac-amount">
                签到奖励数额
              </label>
              <div style="display:flex;gap:6px;">
                <input id="ac-amount" name="check_in_amount" type="number" min="0" class="input-field" value={currentAmount} style="flex:1;" />
                <select name="check_in_currency" class="input-field" style="width:110px;">
                  <option value="coin" selected={currentCurrency === 'coin'}>B币 (金币)</option>
                  <option value="exp" selected={currentCurrency === 'exp'}>经验值 (EXP)</option>
                </select>
              </div>
              <small class="text-secondary" style="display:block;margin-top:4px;font-size:11px;">
                每次成功签到时发放给用户的资产数量。
              </small>
            </div>

            <!-- 每日签到次数上限 -->
            <div class="input-wrapper">
              <label class="input-label" for="ac-daily-limit">
                每日签到上限次数
              </label>
              <input id="ac-daily-limit" name="check_in_daily_limit" type="number" min="1" max="10" class="input-field" value={currentDailyLimit} />
              <small class="text-secondary" style="display:block;margin-top:4px;font-size:11px;">
                每个用户在同一个自然周期内可领取的最大签到次数（默认 1 次）。
              </small>
            </div>
          </div>

          <div style="display:flex;gap:var(--space-3);align-items:flex-end;padding-top:var(--space-3);border-top:1px solid var(--color-border);flex-wrap:wrap;">
            <div class="input-wrapper" style="flex:1;min-width:240px;margin-bottom:0;">
              <label class="input-label" for="ac-reason">修改原因（审计必填） <span class="app-required">*</span></label>
              <input id="ac-reason" name="reason" class="input-field" required placeholder="例如：更新签到规则与跨天刷新时间点" />
            </div>
            <Button text="保存签到配置" variant="primary" size="md" type="submit" />
          </div>
        </form>
      </div>
    </div>
  {:else}
    <div class="app-card" style="margin-bottom:var(--space-4);">
      <div class="app-card__body">
        <p class="input-hint is-error" role="alert">{adminStateLabel(config.state)}：{config.message}</p>
      </div>
    </div>
  {/if}

  <div class="app-card" style="margin-bottom:var(--space-4);">
    <div class="app-card__head"><h2>新建活跃任务</h2></div>
    <div class="app-card__body">
      <form method="POST" action="?/create-task" use:enhance={withActionToast()}>
        <div style="display:flex;flex-wrap:wrap;gap:var(--space-2);align-items:flex-end;">
          <div class="input-wrapper">
            <label class="input-label" for="nt-kind">类型</label>
            <select id="nt-kind" name="kind" class="input-field">
              {#each TASK_KINDS as kind}
                <option value={kind}>{kindLabel(kind)}</option>
              {/each}
            </select>
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="nt-amount">奖励</label>
            <input id="nt-amount" name="amount" type="number" min="0" class="input-field" required />
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="nt-daily">每日上限（空=不限）</label>
            <input id="nt-daily" name="daily_limit" type="number" min="0" class="input-field" />
          </div>
          <div class="input-wrapper">
            <label class="input-label" for="nt-reason">操作原因</label>
            <input id="nt-reason" name="reason" class="input-field" required placeholder="必填（写审计）" style="width:200px;" />
          </div>
          <Button text="创建任务" variant="primary" size="sm" type="submit" />
        </div>
      </form>
    </div>
  </div>

  <div class="app-card">
    <div class="app-card__head"><h2>活跃任务列表（{tasks.state === 'ok' ? tasks.items.length : '—'}）</h2></div>
    <div class="card-body" style="padding:0;">
      {#if tasks.state !== 'ok'}
        <p class="input-hint is-error" role="alert" style="padding:var(--space-4);">{adminStateLabel(tasks.state)}</p>
      {:else if tasks.items.length === 0}
        <div style="padding:var(--space-4);"><EmptyState icon="activity" title="暂无任务" /></div>
      {:else}
        <div style="display:flex;flex-direction:column;">
          {#each tasks.items as t (t.id)}
            <div class="post-row" style="padding:var(--space-3);border-bottom:var(--border-default);">
              <div style="display:flex;gap:var(--space-3);align-items:center;flex-wrap:wrap;">
                <div style="min-width:0;flex:1;">
                  <strong>{kindLabel(t.kind)}</strong>
                  {#if t.title}<span class="badge badge-neutral" style="margin-left:var(--space-2);">{t.title}</span>{/if}
                  <span class="badge {t.is_enabled ? 'badge-success' : 'badge-neutral'}">{t.is_enabled ? '启用' : '停用'}</span>
                  <p class="text-secondary" style="font-size:var(--text-xs);margin:2px 0 0;">
                    +{t.amount} {t.currency.toUpperCase()} · v{t.version} · 更新于 {new Date(t.updated_at).toLocaleString('zh-CN')}
                  </p>
                </div>
                <form method="POST" action="?/update-task" use:enhance={withActionToast()} style="display:flex;gap:var(--space-2);align-items:center;flex-wrap:wrap;">
                  <input type="hidden" name="id" value={t.id} />
                  <input type="hidden" name="version" value={t.version} />
                  <label class="input-label" style="display:flex;align-items:center;gap:4px;font-size:var(--text-sm);">
                    <input type="checkbox" name="is_enabled" checked={t.is_enabled} /> 启用
                  </label>
                  <input name="amount" type="number" min="0" class="input-field" value={t.amount} aria-label="奖励" style="width:90px;" />
                  <input name="reason" class="input-field" required placeholder="原因（必填）" aria-label="操作原因" style="width:150px;" />
                  <Button text="保存" variant="secondary" size="sm" type="submit" />
                </form>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>
</div>
