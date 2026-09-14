<!-- M07-UI-08 + M18-ADMIN-POINTS-03 优化：管理端签到页。
     角色收敛：本页 = 签到运行概览 + 签到全局配置（总闸/自动打卡/跨天/时区/奖励数额）。
     其他活跃规则（发帖/回复/表态/任务/榜单）的编辑统一在「积分规则配置」页，
     本页只读展示摘要计数，避免两处编辑同一 activity_rules 行。 -->
<script lang="ts">
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { enhance } from '$app/forms';
  import { adminStateLabel } from '$lib/admin';
  import { toastActionResult } from '$lib/ui/action-toast';
  import Button from '$lib/components/ui/Button.svelte';
  import Dialog from '$lib/components/ui/Dialog.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import type { ActivityTask } from '$lib/api/types';
  import type { AdminActivityPageData } from './+page.server';

  let { data, form }: { data: AdminActivityPageData; form?: { message?: string } | null } = $props();

  const config = $derived(data.config);
  const tasks = $derived(data.tasks);
  const message = $derived(form?.message ?? null);

  // 约定 A（按钮→弹层）：签到全局配置 = 「全局配置」按钮 + Dialog（?/save-config）；
  // 页面主体保留只读概览卡。
  let configOpen = $state(false);

  // JS 启用：动作结果走全局 Toast 浮窗；顶部内联横幅仅保留为无 JS 回退。
  let hasJs = $state(false);
  $effect(() => {
    hasJs = true;
  });

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

  /** 其他活跃规则种类（与后端 RULE_KINDS 一致；签到单独配置，不在摘要卡）。 */
  const SUMMARY_KINDS: Array<{ kind: ActivityTask['kind']; label: string; icon: string }> = [
    { kind: 'post', label: '发帖', icon: 'book-open' },
    { kind: 'comment', label: '回复', icon: 'at-sign' },
    { kind: 'reaction', label: '表态', icon: 'heart' },
    { kind: 'task', label: '任务', icon: 'award' },
    { kind: 'leaderboard', label: '榜单', icon: 'trophy' }
  ];

  const summary = $derived.by(() => {
    if (tasks.state !== 'ok') return null;
    return SUMMARY_KINDS.map((meta) => {
      const rules = tasks.items.filter((t) => t.kind === meta.kind);
      return { ...meta, total: rules.length, enabled: rules.filter((r) => r.is_enabled).length };
    });
  });
</script>

<svelte:head>
  <title>签到与活跃管理 — BBLBB</title>
</svelte:head>

<PageHeader title="签到与活跃管理" />

  {#if message && !hasJs}
    <p class="input-hint is-error" role="alert" style="margin-bottom:var(--space-3);">{message}</p>
  {/if}

  {#if config.state === 'ok'}
    {@const currentTz = config.data.site_timezone || 'Asia/Shanghai'}
    {@const currentResetHour = config.data.day_reset_hour ?? config.data.check_in?.day_reset_hour ?? 0}
    {@const currentAmount = config.data.check_in?.amount ?? config.data.check_in_reward?.amount ?? 10}
    {@const currentDailyLimit = config.data.check_in?.daily_limit ?? config.data.check_in_daily_limit ?? 1}
    {@const isAutoEnabled = config.data.auto_check_in_enabled !== false && config.data.check_in?.auto_enabled !== false}
    {@const isCheckInEnabled = config.data.check_in_enabled !== false && config.data.check_in?.enabled !== false}
    {@const isRewardsEnabled = config.data.rewards_enabled !== false}
    {@const currencyLabel = 'B币'}

    <!-- ── 运行概览：状态磁贴 + 当前规则摘要（只读，随保存刷新） ── -->
    <div class="app-card" style="margin-bottom:var(--space-4);">
      <div class="app-card__head" style="display:flex;align-items:center;justify-content:space-between;gap:10px;flex-wrap:wrap;">
        <div>
          <h2 style="margin:0;">运行概览</h2>
          <p class="text-secondary" style="font-size:var(--text-xs);margin:2px 0 0;">当前生效的签到策略（配置 v{config.data.version}）</p>
        </div>
        <span class="badge {isCheckInEnabled ? 'badge-success' : 'badge-neutral'}">
          {isCheckInEnabled ? '签到功能运行中' : '签到已关闭'}
        </span>
      </div>
      <div class="app-card__body">
        <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(200px,1fr));gap:var(--space-3);">
          <div style="padding:12px 14px;border:1px solid var(--color-border);border-radius:var(--radius-md);display:flex;gap:10px;align-items:flex-start;background:var(--color-bg-subtle);">
            <span style="margin-top:2px;flex:none;display:inline-flex;"><Icon name="activity" size={16} /></span>
            <div>
              <div style="font-size:12px;font-weight:600;">每日签到</div>
              <div style="font-size:13px;font-weight:700;color:{isCheckInEnabled ? 'var(--color-success)' : 'var(--color-text-secondary)'};">
                {isCheckInEnabled ? '运行中' : '已关闭'}
              </div>
            </div>
          </div>
          <div style="padding:12px 14px;border:1px solid var(--color-border);border-radius:var(--radius-md);display:flex;gap:10px;align-items:flex-start;background:var(--color-bg-subtle);">
            <span style="margin-top:2px;flex:none;display:inline-flex;"><Icon name="log-in" size={16} /></span>
            <div>
              <div style="font-size:12px;font-weight:600;">登录自动打卡</div>
              <div style="font-size:13px;font-weight:700;color:{isAutoEnabled ? 'var(--color-success)' : 'var(--color-text-secondary)'};">
                {isAutoEnabled ? '已开启' : '需手动签到'}
              </div>
            </div>
          </div>
          <div style="padding:12px 14px;border:1px solid var(--color-border);border-radius:var(--radius-md);display:flex;gap:10px;align-items:flex-start;background:var(--color-bg-subtle);">
            <span style="margin-top:2px;flex:none;display:inline-flex;"><Icon name={isRewardsEnabled ? 'check-circle' : 'alert-triangle'} size={16} /></span>
            <div>
              <div style="font-size:12px;font-weight:600;">全站奖励</div>
              <div style="font-size:13px;font-weight:700;color:{isRewardsEnabled ? 'var(--color-success)' : 'var(--color-text-secondary)'};">
                {isRewardsEnabled ? '发放中' : '已暂停'}
              </div>
            </div>
          </div>
          <div style="padding:12px 14px;border:1px solid var(--color-border);border-radius:var(--radius-md);display:flex;gap:10px;align-items:flex-start;background:var(--color-bg-subtle);">
            <span style="margin-top:2px;flex:none;display:inline-flex;"><Icon name="coins" size={16} /></span>
            <div>
              <div style="font-size:12px;font-weight:600;">签到奖励</div>
              <div style="font-size:13px;font-weight:700;">+{currentAmount} {currencyLabel}<span class="text-secondary" style="font-weight:400;font-size:11px;"> / 次 · 每日≤{currentDailyLimit} 次</span></div>
            </div>
          </div>
        </div>
        <p class="text-secondary" style="font-size:var(--text-xs);margin:10px 0 0;display:flex;gap:14px;flex-wrap:wrap;">
          <span><Icon name="clock" size={12} /> 跨天刷新 {String(currentResetHour).padStart(2, '0')}:00（{currentTz}）</span>
          <span><a href="/admin/points/rules">积分规则配置 →</a></span>
          <span><a href="/admin/audit">审计日志 →</a></span>
        </p>
      </div>
    </div>

    <!-- ── 签到配置（写操作 = 按钮 + Dialog；概览磁贴只读随保存刷新） ── -->
    <div class="app-card" style="margin-bottom:var(--space-4);">
      <div class="app-card__head" style="display:flex;align-items:center;justify-content:space-between;gap:10px;flex-wrap:wrap;">
        <div>
          <h2>签到配置</h2>
          <p class="text-secondary" style="font-size:var(--text-xs);margin:2px 0 0;">
            总闸 / 自动打卡 / 跨天 / 时区 / 奖励数额；保存需填写修改原因（写审计），并要求近期登录验证（step-up）
          </p>
        </div>
        <Button text="全局配置" variant="primary" size="sm" onclick={() => (configOpen = true)} />
      </div>
    </div>

    <!-- ── 签到全局配置：Dialog 内表单（?/save-config，If-Match 版本 + 原因必填） ── -->
    <Dialog
      open={configOpen}
      title="签到全局配置"
      description="总闸 / 自动打卡 / 跨天刷新点 / 时区 / 奖励数额与每日上限；保存需填写修改原因（写审计）。"
      onclose={() => (configOpen = false)}
    >
      <form
        method="POST"
        action="?/save-config"
        use:enhance={() => {
          return async ({ result, update }) => {
            toastActionResult(result);
            await update({ reset: false });
            if (result.type === 'success') configOpen = false;
          };
        }}
        style="display:flex;flex-direction:column;gap:var(--space-4);"
      >
        <input type="hidden" name="expected_version" value={config.data.version} />

        <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(240px,1fr));gap:var(--space-3);">
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
              开启后，用户每日首次登录或浏览社区时自动签到并入账；关闭后，用户须前往个人中心手动点击「立即签到」。
            </p>
          </div>

          <!-- 开关 3：全站奖励开关 -->
          <div style="padding:14px;border:1px solid var(--color-border);border-radius:var(--radius-md);background:var(--color-bg-subtle);">
            <label class="input-label" style="display:flex;align-items:center;gap:8px;font-weight:600;cursor:pointer;">
              <input type="checkbox" name="rewards_enabled" checked={isRewardsEnabled} style="width:16px;height:16px;" />
              启用全站奖励发放
            </label>
            <p class="text-secondary" style="font-size:var(--text-xs);margin:6px 0 0;line-height:1.5;">
              所有活跃奖励实时入账的总闸（含签到与积分规则页配置的各类 B币奖励）。
            </p>
          </div>
        </div>

        <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(220px,1fr));gap:var(--space-3);">
          <!-- 从什么时间算新的一天 -->
          <div class="input-wrapper">
            <label class="input-label" for="ac-reset-hour">跨天刷新点（每天几点算新的一天）</label>
            <select id="ac-reset-hour" name="day_reset_hour" class="input-field">
              {#each RESET_HOURS as opt}
                <option value={opt.hour} selected={currentResetHour === opt.hour}>{opt.label}</option>
              {/each}
            </select>
            <small class="text-secondary" style="display:block;margin-top:4px;font-size:11px;">
              例如设为 04:00 时，凌晨 3:59 签到仍算前一天。
            </small>
          </div>

          <!-- 站点基准时区 -->
          <div class="input-wrapper">
            <label class="input-label" for="ac-timezone">站点基准时区</label>
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
            <label class="input-label" for="ac-amount">签到奖励数额</label>
            <div style="display:flex;gap:6px;">
              <input id="ac-amount" name="check_in_amount" type="number" min="0" class="input-field" value={currentAmount} style="flex:1;" />
              <input type="hidden" name="check_in_currency" value="coin" />
              <span class="input-field" style="width:120px;display:flex;align-items:center;justify-content:center;">B币</span>
            </div>
            <small class="text-secondary" style="display:block;margin-top:4px;font-size:11px;">
              每次成功签到发放的资产数量（当前 +{currentAmount} {currencyLabel}）。
            </small>
          </div>

          <!-- 每日签到次数上限 -->
          <div class="input-wrapper">
            <label class="input-label" for="ac-daily-limit">每日签到上限次数</label>
            <input id="ac-daily-limit" name="check_in_daily_limit" type="number" min="1" max="10" class="input-field" value={currentDailyLimit} />
            <small class="text-secondary" style="display:block;margin-top:4px;font-size:11px;">
              同一自然周期内可领取的最大签到次数（默认 1 次）。
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
    </Dialog>
  {:else}
    <div class="app-card" style="margin-bottom:var(--space-4);">
      <div class="app-card__body">
        <p class="input-hint is-error" role="alert">{adminStateLabel(config.state)}：{config.message}</p>
      </div>
    </div>
  {/if}

  <!-- ── 其他活跃规则（只读摘要；编辑统一在积分规则配置页） ── -->
  <div class="app-card">
    <div class="app-card__head" style="display:flex;align-items:center;justify-content:space-between;gap:10px;flex-wrap:wrap;">
      <div>
        <h2>其他活跃规则</h2>
        <p class="text-secondary" style="font-size:var(--text-xs);margin:2px 0 0;">
          发帖 / 回复 / 表态 / 任务 / 榜单奖励的数额、每日上限、冷却与启停，统一在积分规则配置页管理
        </p>
      </div>
      <a href="/admin/points/rules" class="btn secondary sm">
        <Icon name="coins" size={13} /> 前往积分规则配置
      </a>
    </div>
    <div class="app-card__body">
      {#if tasks.state !== 'ok'}
        <p class="input-hint is-error" role="alert">{adminStateLabel(tasks.state)}：{'message' in tasks ? tasks.message : ''}</p>
      {:else if summary}
        <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:var(--space-3);">
          {#each summary as s (s.kind)}
            <div style="padding:12px 14px;border:1px solid var(--color-border);border-radius:var(--radius-md);display:flex;align-items:center;gap:10px;">
              <span style="flex:none;display:inline-flex;color:{s.enabled > 0 ? 'var(--color-success)' : 'var(--color-text-secondary)'};"><Icon name={s.icon} size={16} /></span>
              <div>
                <div style="font-size:13px;font-weight:700;">
                  {s.enabled}/{s.total} <span class="text-secondary" style="font-weight:400;font-size:11px;">启用</span>
                </div>
                <div class="text-secondary" style="font-size:11px;">{s.label}奖励</div>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>

