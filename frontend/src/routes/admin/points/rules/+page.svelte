<script lang="ts">
  // M18-ADMIN-POINTS-03：积分规则配置页——「什么操作获得多少」矩阵。
  // 约定 A（按钮→弹层）：既有规则 = 行「编辑」按钮 + Dialog（?/update，
  // If-Match version 隐藏域）；未配置动作/追加规则 = 「新增」按钮 + Dialog
  // （?/create，按动作类型预填 kind）。签到全局开关/时区在「签到与活跃」页。
  // 约定 D（M18-ADMIN-OPS）：规则行「编辑」按钮 → 「⋮」三点菜单（单项「编辑规则」，
  // RowActionsMenu；点菜单项打开既有编辑 Dialog ?/update，契约不变）；「配置「X」奖励 /
  // 追加规则」位于动作卡头部（每卡一个 create 入口），保持按钮 + 弹层不改菜单。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { enhance } from '$app/forms';
  import { adminStateLabel } from '$lib/admin';
  import { toastActionResult } from '$lib/ui/action-toast';
  import Icon from '$lib/components/ui/Icon.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import Dialog from '$lib/components/ui/Dialog.svelte';
  import RowActionsMenu from '$lib/components/admin/RowActionsMenu.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import type { ActivityTask } from '$lib/api/types';
  import type { AdminPointsRulesPageData } from './+page.server';

  let { data, form }: {
    data: AdminPointsRulesPageData;
    form?: { message?: string } | null;
  } = $props();

  const rules = $derived(data.rules);

  // JS 启用：动作结果走全局 Toast 浮窗；顶部内联横幅仅保留为无 JS 回退。
  let hasJs = $state(false);
  $effect(() => {
    hasJs = true;
  });

  /** 动作种类展示元数据（kind 值与后端 RULE_KINDS / 0050 CHECK 一致）。 */
  const KINDS: Array<{
    kind: ActivityTask['kind'];
    label: string;
    icon: string;
    desc: string;
    wired: string;
  }> = [
    {
      kind: 'check_in',
      label: '每日签到',
      icon: 'activity',
      desc: '用户每日签到 / 登录访问自动打卡时发放',
      wired: '已接入（签到引擎）'
    },
    {
      kind: 'post',
      label: '发布主题',
      icon: 'book-open',
      desc: '成功发布一篇主题后发放（同帖去重，删帖重发不重复奖励）',
      wired: '已接入（发帖事件）'
    },
    {
      kind: 'comment',
      label: '发表回复',
      icon: 'at-sign',
      desc: '成功发表一条回复后发放（同回复去重）',
      wired: '已接入（回复事件）'
    },
    {
      kind: 'reaction',
      label: '内容表态',
      icon: 'heart',
      desc: '对他人内容点赞/表态后发放（自赞不算，同目标同反应只奖一次）',
      wired: '已接入（表态事件）'
    },
    {
      kind: 'task',
      label: '自定义任务',
      icon: 'award',
      desc: '运营活动的自定义任务奖励规则（事件接入后按规则领取）',
      wired: '规则可配置'
    },
    {
      kind: 'leaderboard',
      label: '榜单奖励',
      icon: 'trophy',
      desc: '榜单结算奖励规则（事件接入后按规则领取）',
      wired: '规则可配置'
    }
  ];

  function kindLabel(kind: string): string {
    return KINDS.find((k) => k.kind === kind)?.label ?? kind;
  }

  function currencyLabel(_c: string): string {
    return 'B币';
  }

  // ── 编辑 Dialog（一个 Dialog 服务全部规则行，target 区分行） ──
  let updateTarget: ActivityTask | null = $state(null);

  function openUpdate(rule: ActivityTask): void {
    updateTarget = rule;
  }

  // ── 新建规则 Dialog（预填动作类型） ──
  let createOpen = $state(false);
  let createKind = $state<ActivityTask['kind']>('task');

  function openCreate(kind: ActivityTask['kind']): void {
    createKind = kind;
    createOpen = true;
  }

  /** 按 kind 分组（keep 原顺序）。 */
  const byKind = $derived.by(() => {
    const map = new Map<string, ActivityTask[]>();
    if (rules.state === 'ok') {
      for (const r of rules.items) {
        const list = map.get(r.kind) ?? [];
        list.push(r);
        map.set(r.kind, list);
      }
    }
    return map;
  });

  const stats = $derived.by(() => {
    if (rules.state !== 'ok') return { total: 0, enabled: 0 };
    return {
      total: rules.items.length,
      enabled: rules.items.filter((r) => r.is_enabled).length
    };
  });

  const masterOn = $derived(data.masterSwitch && !('unavailable' in data.masterSwitch) ? data.masterSwitch.enabled : null);
</script>

<svelte:head>
  <title>积分规则配置 — BBLBB Admin</title>
</svelte:head>

<PageHeader title="积分规则配置" />

{#if form?.message && !hasJs}
  <div class="alert alert-info" role="status" style="margin-bottom:12px;padding:10px 14px;background:var(--color-bg-subtle);border-radius:var(--radius-sm);font-size:13px;">
    {form.message}
  </div>
{/if}

<!-- 总览：统计 + 奖励总闸状态 -->
<div class="app-card" style="margin-bottom:14px;">
  <div class="app-card__body" style="display:flex;flex-wrap:wrap;gap:var(--space-4);align-items:center;">
    <div style="display:flex;gap:var(--space-5);">
      <div>
        <div style="font-size:22px;font-weight:700;">{stats.total}</div>
        <div class="text-secondary" style="font-size:12px;">规则总数</div>
      </div>
      <div>
        <div style="font-size:22px;font-weight:700;color:var(--color-success);">{stats.enabled}</div>
        <div class="text-secondary" style="font-size:12px;">启用中</div>
      </div>
    </div>
    <div style="flex:1;min-width:220px;display:flex;align-items:center;gap:10px;padding:10px 14px;border:1px solid var(--color-border);border-radius:var(--radius-md);background:var(--color-bg-subtle);">
      <Icon name={masterOn === false ? 'alert-triangle' : 'check-circle'} size={18} />
      <div style="font-size:12px;line-height:1.5;">
        {#if data.masterSwitch && 'unavailable' in data.masterSwitch}
          奖励总闸状态未知：{data.masterSwitch.message}
        {:else if masterOn === false}
          <b>全站奖励发放已关闭</b>——所有规则暂停入账。请在
          <a href="/admin/activity">签到与活跃</a> 中开启「启用全站奖励发放」。
        {:else}
          全站奖励发放运行中（<a href="/admin/activity">签到与活跃</a> 管理总闸与时区）。
        {/if}
      </div>
    </div>
  </div>
</div>

{#if rules.state !== 'ok'}
  <div class="app-card">
    <div class="app-card__body">
      <p class="input-hint is-error" role="alert">{adminStateLabel(rules.state)}：{'message' in rules ? rules.message : ''}</p>
    </div>
  </div>
{:else}
  {#each KINDS as meta (meta.kind)}
    {@const kindRules = byKind.get(meta.kind) ?? []}
    <section class="app-card" style="margin-bottom:14px;">
      <header class="app-card__head" style="display:flex;align-items:center;justify-content:space-between;gap:10px;flex-wrap:wrap;">
        <div style="display:flex;align-items:center;gap:10px;">
          <Icon name={meta.icon} size={18} />
          <div>
            <h2 style="margin:0;font-size:15px;">{meta.label}</h2>
            <p class="text-secondary" style="font-size:12px;margin:2px 0 0;">{meta.desc}</p>
          </div>
        </div>
        <div style="display:flex;align-items:center;gap:8px;">
          <span class="badge {kindRules.some((r) => r.is_enabled) ? 'badge-success' : 'badge-neutral'}">
            {kindRules.length === 0 ? '未配置' : `${kindRules.filter((r) => r.is_enabled).length}/${kindRules.length} 启用`}
          </span>
          <Button
            text={kindRules.length === 0 ? `配置「${meta.label}」奖励` : '追加规则'}
            variant={kindRules.length === 0 ? 'primary' : 'secondary'}
            size="sm"
            onclick={() => openCreate(meta.kind)}
          />
        </div>
      </header>
      <div class="app-card__body" style="display:flex;flex-direction:column;gap:10px;">
        <p class="text-secondary" style="font-size:11px;margin:0;">生效状态：{meta.wired}</p>

        {#each kindRules as rule (rule.id)}
          <div
            style="display:flex;flex-wrap:wrap;gap:8px;align-items:center;padding:10px;border:1px solid var(--color-border);border-radius:var(--radius-md);"
          >
            <span class="badge {rule.is_enabled ? 'badge-success' : 'badge-neutral'}">{rule.is_enabled ? '启用' : '停用'}</span>
            <b style="font-size:13px;">+{rule.amount} {currencyLabel(rule.currency)}</b>
            <span class="text-secondary" style="font-size:12px;">每日上限：{rule.daily_limit ?? '不限'}</span>
            <span class="text-secondary" style="font-size:12px;">冷却：{rule.cooldown_seconds ?? 0} 秒</span>
            <span class="text-secondary" style="font-size:11px;">
              v{rule.version} · 更新于 {new Date(rule.updated_at).toLocaleString('zh-CN')}
            </span>
            <span style="flex:1;"></span>
            <!-- 行写操作 = 「⋮」三点菜单（约定 D）：单项「编辑规则」→ 既有编辑 Dialog -->
            <RowActionsMenu
              label="更多操作：积分规则 {kindLabel(rule.kind)}"
              actions={[{ label: '编辑规则', run: () => openUpdate(rule) }]}
            />
          </div>
        {/each}

        {#if kindRules.length === 0}
          <div style="padding:6px 0 2px;"><EmptyState icon={meta.icon} title="尚未配置该动作的奖励" /></div>
        {/if}
      </div>
    </section>
  {/each}

  <div class="app-card">
    <div class="app-card__body">
      <p class="text-secondary" style="font-size:12px;margin:0;line-height:1.6;">
        规则保存于 <code>activity_rules</code>（不可变账本 + 幂等去重 + 每日上限 + 冷却由服务端裁决）；
        签到的全局开关 / 自动打卡 / 跨天时区在 <a href="/admin/activity">签到与活跃</a> 页配置。
        修改历史与操作原因可在 <a href="/admin/audit">审计日志</a> 中追溯。
      </p>
    </div>
  </div>
{/if}

<!-- 编辑规则：Dialog 内表单（id/version/kind 隐藏域 + If-Match 乐观锁，原因必填写审计）。 -->
<Dialog
  open={updateTarget !== null}
  title="编辑积分规则"
  description={updateTarget
    ? `${kindLabel(updateTarget.kind)}：修改数额/币种/每日上限/冷却/启停；原因写入审计日志。`
    : ''}
  onclose={() => (updateTarget = null)}
>
  {#if updateTarget}
    <form
      method="POST"
      action="?/update"
      use:enhance={() => {
        return async ({ result, update }) => {
          toastActionResult(result);
          await update({ reset: false });
          if (result.type === 'success') updateTarget = null;
        };
      }}
      style="display:flex;flex-direction:column;gap:10px;"
    >
      <input type="hidden" name="id" value={updateTarget.id} />
      <input type="hidden" name="version" value={updateTarget.version} />
      <input type="hidden" name="kind" value={updateTarget.kind} />

      <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(160px,1fr));gap:10px;">
        <label style="display:flex;flex-direction:column;gap:3px;font-size:12px;font-weight:600;">
          奖励数额
          <input name="amount" type="number" min="0" class="input-field" value={updateTarget.amount} aria-label="奖励数额" />
        </label>
        <label style="display:flex;flex-direction:column;gap:3px;font-size:12px;font-weight:600;">
          币种
          <input type="hidden" name="currency" value="coin" />
          <span class="app-select" style="width:100%;display:flex;align-items:center;">B币</span>
        </label>
        <label style="display:flex;flex-direction:column;gap:3px;font-size:12px;font-weight:600;">
          每日上限（空=不变）
          <input name="daily_limit" type="number" min="1" class="input-field" value={updateTarget.daily_limit ?? ''} aria-label="每日上限" />
        </label>
        <label style="display:flex;flex-direction:column;gap:3px;font-size:12px;font-weight:600;">
          冷却秒数（空=不变）
          <input name="cooldown_seconds" type="number" min="0" class="input-field" value={updateTarget.cooldown_seconds ?? ''} aria-label="冷却秒数" />
        </label>
      </div>
      <label style="display:flex;align-items:center;gap:6px;font-size:13px;font-weight:600;">
        <input type="checkbox" name="is_enabled" checked={updateTarget.is_enabled} style="width:15px;height:15px;" /> 启用该规则
      </label>
      <label style="display:flex;flex-direction:column;gap:3px;font-size:12px;font-weight:600;">
        修改原因（审计必填）
        <input name="reason" class="input-field" required placeholder="例如：调整发帖奖励至 10 B币" aria-label="修改原因" />
      </label>
      <div style="display:flex;gap:8px;justify-content:flex-end;">
        <button type="button" class="btn ghost sm" onclick={() => (updateTarget = null)}>取消</button>
        <Button text="保存规则" variant="primary" size="sm" type="submit" />
      </div>
    </form>
  {/if}
</Dialog>

<!-- 新建规则：Dialog 内表单（kind 预填，服务端校验 RULE_KINDS）。 -->
<Dialog
  open={createOpen}
  title={`配置「${kindLabel(createKind)}」奖励`}
  description="新增一条该动作的奖励规则；原因写入审计日志。"
  onclose={() => (createOpen = false)}
>
  <form
    method="POST"
    action="?/create"
    use:enhance={() => {
      return async ({ result, update }) => {
        toastActionResult(result);
        await update();
        if (result.type === 'success') createOpen = false;
      };
    }}
    style="display:flex;flex-direction:column;gap:10px;"
  >
    <input type="hidden" name="kind" value={createKind} />
    <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(160px,1fr));gap:10px;">
      <label style="display:flex;flex-direction:column;gap:3px;font-size:12px;font-weight:600;">
        奖励数额
        <input name="amount" type="number" min="0" class="input-field" required aria-label="奖励数额" />
      </label>
      <label style="display:flex;flex-direction:column;gap:3px;font-size:12px;font-weight:600;">
        币种
        <input type="hidden" name="currency" value="coin" />
        <span class="app-select" style="width:100%;display:flex;align-items:center;">B币</span>
      </label>
      <label style="display:flex;flex-direction:column;gap:3px;font-size:12px;font-weight:600;">
        每日上限（空=不限）
        <input name="daily_limit" type="number" min="1" class="input-field" aria-label="每日上限" />
      </label>
      <label style="display:flex;flex-direction:column;gap:3px;font-size:12px;font-weight:600;">
        冷却秒数（空=不限）
        <input name="cooldown_seconds" type="number" min="0" class="input-field" aria-label="冷却秒数" />
      </label>
    </div>
    <label style="display:flex;align-items:center;gap:6px;font-size:13px;font-weight:600;">
      <input type="checkbox" name="is_enabled" checked style="width:15px;height:15px;" /> 立即启用
    </label>
    <label style="display:flex;flex-direction:column;gap:3px;font-size:12px;font-weight:600;">
      操作原因（审计必填）
      <input name="reason" class="input-field" required placeholder="例如：上线发帖积分激励" aria-label="操作原因" />
    </label>
    <div style="display:flex;gap:8px;justify-content:flex-end;">
      <button type="button" class="btn ghost sm" onclick={() => (createOpen = false)}>取消</button>
      <Button text="创建规则" variant="primary" size="sm" type="submit" />
    </div>
  </form>
</Dialog>
