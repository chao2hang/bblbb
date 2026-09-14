<!-- P0 整改：/admin/feature-flags —— 可选能力运行时开关管理。
     每行一个启停 Dialog（If-Match version + reason 审计）；页脚 kill switch
     危险操作（全部可选能力立即禁用，reason 必填）。 -->
<script lang="ts">
  import { enhance } from '$app/forms';
  import type { SubmitFunction } from '@sveltejs/kit';
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import Dialog from '$lib/components/ui/Dialog.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { toastActionResult } from '$lib/ui/action-toast';
  import { flagLabel } from './flags';
  import type {
    AdminFlagsPageData,
    AdminFlagsActionData,
    AdminFlagItem
  } from './+page.server';

  let { data, form }: { data: AdminFlagsPageData; form?: AdminFlagsActionData | null } = $props();

  let hasJs = $state(false);
  $effect(() => {
    hasJs = true;
  });

  // 行启停弹层（一个 Dialog 服务一类操作）。
  let toggleTarget: AdminFlagItem | null = $state(null);
  let toggleNext = $state(false);
  let toggleReason = $state('');

  function openToggle(f: AdminFlagItem): void {
    toggleTarget = f;
    toggleNext = !f.enabled;
    toggleReason = '';
  }
  function closeToggle(): void {
    toggleTarget = null;
  }

  // kill switch 弹层。
  let killOpen = $state(false);
  let killReason = $state('');

  const message = $derived(form?.message ?? null);
  const conflict = $derived(form?.conflict === true);

  function badge(enabled: boolean, kill: boolean): { text: string; cls: string } {
    if (kill) return { text: '已紧急关闭', cls: 'badge-danger' };
    return enabled ? { text: '启用中', cls: 'badge-success' } : { text: '已停用', cls: 'badge-neutral' };
  }

  const dialogEnhance =
    (onSuccess: () => void): SubmitFunction =>
    () =>
    async ({ result, update }) => {
      toastActionResult(result);
      await update();
      if (result.type === 'success') onSuccess();
    };
</script>

<svelte:head>
  <title>功能开关 — BBLBB Admin</title>
</svelte:head>

<PageHeader title="功能开关" />

{#if form?.message && !hasJs}
  <div class="alert alert-info" role="status" style="margin-bottom:12px;padding:10px 14px;background:var(--color-bg-subtle);border-radius:var(--radius-sm);font-size:13px;">
    {form.message}
  </div>
{/if}
{#if conflict}
  <div class="app-error" role="alert" style="margin-bottom:12px;padding:10px 14px;">
    {form?.message}
  </div>
{/if}

{#if data.state === 'ok' && data.flags}
  {#if data.kill_switch}
    <section class="app-card" style="margin-bottom:14px;border-color:var(--color-danger);">
      <div class="app-card__body" style="padding:14px 16px;">
        <b style="color:var(--color-danger);">紧急关闭生效中</b>
        <span class="app-muted" style="font-size:12px;display:block;">
          全部可选能力被强制禁用（优先于一切开关）。恢复需重启实例并解除环境变量 kill switch 后重新启用。
        </span>
      </div>
    </section>
  {/if}

  <section class="app-card" style="margin-bottom:14px;">
    <header class="app-card__head">
      <h2>可选能力</h2>
    </header>
    <div class="app-card__body">
      <div class="app-table-wrap">
        <table class="app-table" aria-label="功能开关">
          <thead>
            <tr>
              <th>能力</th>
              <th style="width:110px;">状态</th>
              <th style="width:90px;">版本</th>
              <th>最近变更</th>
              <th style="width:140px;">操作</th>
            </tr>
          </thead>
          <tbody>
            {#each data.flags as f (f.name)}
              {@const b = badge(f.enabled, data.kill_switch)}
              <tr>
                <td>
                  <b>{flagLabel(f.name)}</b>
                  <code style="font-size:11px;color:var(--color-text-secondary);display:block;">{f.name}</code>
                </td>
                <td><span class="badge {b.cls}" style="font-size:11px;">{b.text}</span></td>
                <td><code style="font-size:12px;">v{f.version}</code></td>
                <td>
                  <span class="text-secondary" style="font-size:12px;">
                    {f.updated_by ?? 'system（默认值）'}
                  </span>
                </td>
                <td>
                  <Button
                    text={f.enabled ? '停用' : '启用'}
                    variant={f.enabled ? 'secondary' : 'primary'}
                    size="sm"
                    disabled={data.kill_switch}
                    onclick={() => openToggle(f)}
                  />
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </div>
    <footer class="app-card__foot" style="display:flex;align-items:center;justify-content:space-between;gap:10px;padding:12px 16px;">
      <span class="app-muted" style="font-size:11px;">
        变更即时生效（同进程原地重载）；reason 写入审计日志。Flag 不绕过权限、CSRF 与账务。
      </span>
      <Button text="紧急关闭全部" variant="secondary" size="sm" disabled={data.kill_switch} onclick={() => (killOpen = true)} />
    </footer>
  </section>
{:else if data.state === 'forbidden'}
  <section class="app-card">
    <div class="app-card__body">
      <EmptyState icon="shield" title="无权限" desc="需要 admin.manage 权限管理功能开关" />
    </div>
  </section>
{:else}
  <section class="app-card">
    <div class="app-card__body">
      <p class="input-hint" role="alert">功能开关接口不可用（{data.state}）：{data.error}</p>
    </div>
  </section>
{/if}

<!-- 启停弹层 -->
<Dialog
  open={toggleTarget !== null}
  title={toggleTarget ? `${toggleNext ? '启用' : '停用'}「${flagLabel(toggleTarget.name)}」` : ''}
  description={toggleNext
    ? '启用后新请求即可使用该能力；历史数据不受影响。'
    : '停用后新请求立即拒绝（`feature_disabled`）；不删除历史数据、不撤销已提交账务。'}
  onclose={closeToggle}
>
  {#if toggleTarget}
    <form
      method="POST"
      action="?/toggle"
      use:enhance={dialogEnhance(closeToggle)}
      class="stack"
      style="gap:10px;"
    >
      <input type="hidden" name="name" value={toggleTarget.name} />
      <input type="hidden" name="version" value={toggleTarget.version} />
      <input type="hidden" name="enabled" value={String(toggleNext)} />
      <label>
        <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:4px;display:block;">操作原因（写入审计日志，必填）</span>
        <input type="text" name="reason" class="input-field" required placeholder="如：上线 AI 辅助格式化" />
      </label>
      <div style="display:flex;gap:8px;justify-content:flex-end;">
        <button type="button" class="btn ghost sm" onclick={closeToggle}>取消</button>
        <Button text={toggleNext ? '确认启用' : '确认停用'} variant={toggleNext ? 'primary' : 'secondary'} size="sm" type="submit" />
      </div>
    </form>
  {/if}
</Dialog>

<!-- 紧急关闭弹层（危险操作） -->
<Dialog
  open={killOpen}
  title="紧急关闭全部可选能力"
  description="kill switch：AI / 视频 / 下载计费 / OIDC / Marketplace 全部立即禁用并持久化（优先于一切开关）。仅在事故处置时使用。"
  onclose={() => (killOpen = false)}
>
  <form method="POST" action="?/killSwitch" use:enhance={dialogEnhance(() => (killOpen = false))} class="stack" style="gap:10px;">
    <label>
      <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:4px;display:block;">关闭原因（写入审计日志，必填）</span>
      <input type="text" name="reason" class="input-field" required placeholder="如：疑似滥用，紧急止血" />
    </label>
    <div style="display:flex;gap:8px;justify-content:flex-end;">
      <button type="button" class="btn ghost sm" onclick={() => (killOpen = false)}>取消</button>
      <Button text="确认紧急关闭" variant="secondary" size="sm" type="submit" />
    </div>
  </form>
</Dialog>
