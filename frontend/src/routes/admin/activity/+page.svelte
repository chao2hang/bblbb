<!-- M07-UI-08：管理端活跃——签到/任务配置（版本冲突提示、审计 reason 必填）。
-->
<script lang="ts">
  import { enhance } from '$app/forms';
  import { adminStateLabel } from '$lib/admin';
  import Button from '$lib/components/ui/Button.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import type { ActivityTask } from '$lib/api/types';
  import type { AdminActivityPageData } from './+page.server';

  let { data, form }: { data: AdminActivityPageData; form?: { message?: string } | null } = $props();

  const config = $derived(data.config);
  const tasks = $derived(data.tasks);
  const message = $derived(form?.message ?? null);

  const TASK_KINDS = ['check_in', 'task', 'reaction', 'post', 'comment', 'leaderboard'] as const;

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

<div class="container page-content">
  <nav class="breadcrumb" aria-label="面包屑">
    <a href="/" class="breadcrumb-link">首页</a>
    <span class="breadcrumb-sep">/</span>
    <a href="/admin" class="breadcrumb-link">管理后台</a>
    <span class="breadcrumb-sep">/</span>
    <span class="breadcrumb-current">活跃管理</span>
  </nav>

  {#if message}
    <p class="input-hint is-error" role="alert">{message}</p>
  {/if}

  {#if config.state === 'ok'}
    <div class="card" style="margin-bottom:var(--space-4);">
      <div class="card-header"><span class="card-title">签到与活跃配置（v{config.data.version}）</span></div>
      <div class="card-body">
        <form method="POST" action="?/save-config" use:enhance>
          <input type="hidden" name="expected_version" value={config.data.version} />
          <div style="display:flex;flex-wrap:wrap;gap:var(--space-4);align-items:center;">
            <label class="input-label" style="display:flex;align-items:center;gap:var(--space-1);">
              <input type="checkbox" name="check_in_enabled" checked={config.data.check_in_enabled !== false} />
              启用自动签到（每日首次有效页面访问）
            </label>
            <div class="input-wrapper">
              <label class="input-label" for="ac-amount">签到奖励（coin）</label>
              <input id="ac-amount" name="check_in_amount" type="number" min="0" class="input-field" value={config.data.check_in_reward?.amount ?? ''} style="width:110px;" />
            </div>
            <div class="input-wrapper">
              <label class="input-label" for="ac-reason">操作原因</label>
              <input id="ac-reason" name="reason" class="input-field" required placeholder="必填（写审计）" style="width:200px;" />
            </div>
            <Button text="保存配置" variant="primary" size="sm" type="submit" />
          </div>
        </form>
      </div>
    </div>
  {:else}
    <div class="card" style="margin-bottom:var(--space-4);">
      <div class="card-body">
        <p class="input-hint is-error" role="alert">{adminStateLabel(config.state)}：{config.message}</p>
      </div>
    </div>
  {/if}

  <div class="card" style="margin-bottom:var(--space-4);">
    <div class="card-header"><span class="card-title">新建任务</span></div>
    <div class="card-body">
      <form method="POST" action="?/create-task" use:enhance>
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
            <input id="nt-reason" name="reason" class="input-field" required placeholder="必填（写审计）" />
          </div>
          <Button text="创建任务" variant="primary" size="sm" type="submit" />
        </div>
      </form>
    </div>
  </div>

  <div class="card">
    <div class="card-header"><span class="card-title">任务列表（{tasks.state === 'ok' ? tasks.items.length : '—'}）</span></div>
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
                  <span class="badge badge-neutral" style="margin-left:var(--space-2);">{t.title ?? ''}</span>
                  <span class="badge {t.is_enabled ? 'badge-success' : 'badge-neutral'}">{t.is_enabled ? '启用' : '停用'}</span>
                  <p class="text-secondary" style="font-size:var(--text-xs);margin:2px 0 0;">
                    +{t.amount} {t.currency.toUpperCase()} · v{t.version} · 更新于 {new Date(t.updated_at).toLocaleString('zh-CN')}
                  </p>
                </div>
                <form method="POST" action="?/update-task" use:enhance style="display:flex;gap:var(--space-2);align-items:center;flex-wrap:wrap;">
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
