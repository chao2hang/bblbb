<script lang="ts">
  // M10-UI-06 & M18-ADMIN-VIDEO：管理端视频配置（对齐原型转码队列与白名单）。
  // 约定 A：所有写操作 = 按钮 → Dialog（表单在弹层内，reason 必填写审计，
  // 成功后 toastActionResult → update → 关闭弹层并清理 target）。
  // 转码队列为只读展示（Mock 投影，无对应写端点）→ 不提供选择列（无死 UI）。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { untrack } from 'svelte';
  import { enhance } from '$app/forms';
  import type { SubmitFunction } from '@sveltejs/kit';
  import { invalidateAll } from '$app/navigation';
  import Button from '$lib/components/ui/Button.svelte';
  import Dialog from '$lib/components/ui/Dialog.svelte';
  import { show as showToast } from '$lib/ui/toast';
  import { toastActionResult } from '$lib/ui/action-toast';
  import { videoProviderLabel } from '$lib/video/labels';
  import type { AdminVideoActionData, AdminVideoPageData } from './+page.server';
  import type { VideoProviderPolicyView } from '$lib/api/types';

  let { data, form }: { data: AdminVideoPageData; form?: AdminVideoActionData | null } = $props();

  const pageState = $derived(data.state);
  const policies = $derived(data.policies);
  const items = $derived(policies?.items ?? []);


  let q = $state('');
  let statusFilter = $state('');
  let refreshing = $state(false);

  let enableEmbed = $state(untrack(() => data.whitelistConfig?.enableEmbed ?? true));
  let domainWhitelist = $state(untrack(() => data.whitelistConfig?.domainWhitelist ?? 'youtube.com, bilibili.com, v.qq.com, youku.com'));
  let strictMode = $state(untrack(() => data.whitelistConfig?.strictMode ?? 'strict'));
  let fallbackMode = $state(untrack(() => data.whitelistConfig?.fallbackMode ?? 'safe_link'));
  let dismissedMessage = $state(false);

  // JS 启用：动作结果走全局 Toast 浮窗（成功绿/失败红）；顶部内联横幅仅保留为
  // 无 JS 回退（SSR HTML 仍渲染，见 hasJs）。
  let hasJs = $state(false);
  $effect(() => {
    hasJs = true;
  });

  $effect(() => {
    if (form?.whitelistConfig) {
      enableEmbed = form.whitelistConfig.enableEmbed;
      domainWhitelist = form.whitelistConfig.domainWhitelist;
      strictMode = form.whitelistConfig.strictMode;
      fallbackMode = form.whitelistConfig.fallbackMode;
    }
  });

  // P0 整改：转码队列为后端未提供的能力，不再伪造任务（此前恒显 4 条 mock）。
  const displayedTasks: Array<{ id: string; source: string; size: string; status: string }> = [];

  /** 弹层表单共用结果处理：toast → update → 成功才关弹层（失败留在弹层改）。 */
  const dialogEnhance = (onSuccess: () => void): SubmitFunction =>
    () => async ({ result, update }) => {
      toastActionResult(result);
      await update();
      if (result.type === 'success') onSuccess();
    };

  async function handleRefreshQueue() {
    refreshing = true;
    try {
      await invalidateAll();
      showToast('转码队列状态已刷新', 'success');
    } catch {
      showToast('刷新失败', 'danger');
    } finally {
      refreshing = false;
    }
  }

  function exportTasks() {
    const blob = new Blob([JSON.stringify(displayedTasks, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `video-tasks-${new Date().toISOString().slice(0, 10)}.json`;
    a.click();
    URL.revokeObjectURL(url);
    showToast('任务清单已导出为 JSON', 'success');
  }

  // ── 弹层 target 状态（一个 Dialog 服务一类操作，target 区分 Provider）──
  /** 白名单与安全配置（?/save-whitelist）。 */
  let whitelistOpen = $state(false);
  function openWhitelist(): void {
    dismissedMessage = false;
    whitelistOpen = true;
  }
  function closeWhitelist(): void {
    whitelistOpen = false;
  }

  /** Provider 策略字段（save/test Dialog 共用，open 时按 target 预填）。 */
  let policyTarget = $state<VideoProviderPolicyView | null>(null);
  let policyMode = $state<'save' | 'test' | null>(null);
  let policyEnabled = $state(true);
  let policyHosts = $state('');
  let policyEmbedHosts = $state('');
  let policyMediaTypes = $state('');
  let policyReason = $state('');
  let policyNum = $state<Record<string, string>>({});
  const NUMERIC_FIELD_LABELS: Array<[string, string]> = [
    ['max_duration_seconds', '最长时长（秒）'],
    ['max_bytes', '最大字节'],
    ['max_redirects', '最大重定向'],
    ['hls_max_depth', 'HLS 最大深度'],
    ['hls_max_segments', 'HLS 最大分片'],
    ['hls_max_bytes', 'HLS 最大字节'],
    ['timeout_ms', '超时（ms）']
  ];
  function openPolicy(item: VideoProviderPolicyView, mode: 'save' | 'test'): void {
    policyTarget = item;
    policyMode = mode;
    policyEnabled = item.enabled;
    policyHosts = (item.allowed_hosts ?? []).join(', ');
    policyEmbedHosts = (item.embed_hosts ?? []).join(', ');
    policyMediaTypes = (item.allowed_media_types ?? []).join(', ');
    policyReason = '';
    policyNum = {};
    for (const [field] of NUMERIC_FIELD_LABELS) {
      const v = (item as unknown as Record<string, unknown>)[field];
      policyNum[field] = v == null ? '' : String(v);
    }
  }
  function closePolicy(): void {
    policyTarget = null;
    policyMode = null;
  }

  /** 恢复默认策略（?/reset-default）。 */
  let resetTarget = $state<VideoProviderPolicyView | null>(null);
  let resetReason = $state('');
  function openReset(item: VideoProviderPolicyView): void {
    resetTarget = item;
    resetReason = '';
  }
  function closeReset(): void {
    resetTarget = null;
  }
</script>

<svelte:head>
  <title>视频插件 — BBLBB Admin</title>
</svelte:head>

<PageHeader title="视频插件" />

{#if pageState === 'not_implemented'}
  <div class="app-card">
    <div class="app-card__body" role="status">
      <p class="input-hint">视频管理接口开发中。核心论坛功能不受影响。</p>
    </div>
  </div>
{:else if pageState === 'forbidden'}
  <div class="app-card">
    <div class="app-card__body" role="alert">
      <p class="input-hint is-error">没有权限访问视频管理。</p>
    </div>
  </div>
{:else}
  {#if policies?.enabled === false}
    <div class="app-notice" role="note" style="margin-bottom:14px;">
      视频功能未开放（Feature Flag 默认关闭）。
    </div>
  {/if}

  {#if form?.message && !hasJs}
    <div class="alert alert-info" role="status" style="margin-bottom:12px;padding:10px 14px;background:var(--color-bg-subtle);border-radius:var(--radius-sm);font-size:13px;">
      {form.message}
    </div>
  {/if}

  <!-- Provider 策略测试结果（?/test 动作回传，仅 JS 客户端可见） -->
  {#if form?.testResult}
    <div
      class="alert {form.testResult.ok ? 'alert-info' : 'alert-danger'}"
      role="status"
      style="margin-bottom:12px;padding:10px 14px;background:var(--color-bg-subtle);border-radius:var(--radius-sm);font-size:13px;"
    >
      Provider「{form.provider}」测试结果：{form.testResult.message}{form.testResult.elapsed_ms != null ? `（${form.testResult.elapsed_ms} ms）` : ''}
    </div>
  {/if}

  <!-- 卡片 1：转码队列（原型同款表格；只读 Mock 投影，无写端点 → 无选择列） -->
  <section class="app-card" style="margin-bottom:14px;">
    <header class="app-card__head">
      <h2>转码队列</h2>
    </header>
    <div class="app-card__body">
      <!-- 原型通用工具栏 -->
      <div style="display:flex;flex-direction:column;gap:8px;margin-bottom:14px;">
        <input
          type="search"
          bind:value={q}
          class="app-field"
          placeholder="搜索当前列表..."
          aria-label="搜索当前列表"
        />
        <div style="display:flex;align-items:center;justify-content:space-between;gap:8px;">
          <select
            class="app-select"
            bind:value={statusFilter}
            aria-label="状态筛选"
            style="min-width:140px;"
          >
            <option value="">全部状态</option>
            <option value="completed">已完成</option>
            <option value="processing">转码中</option>
            <option value="failed">失败</option>
          </select>
          {#if q || statusFilter}
            <button type="button" class="btn ghost sm" onclick={() => { q = ''; statusFilter = ''; }}>
              清除
            </button>
          {/if}
        </div>
      </div>

      <div class="app-table-wrap">
        <table class="app-table" aria-label="转码任务列表">
          <thead>
            <tr>
              <th>任务 ID</th>
              <th>视频来源</th>
              <th>文件大小</th>
              <th>状态</th>
            </tr>
          </thead>
          <tbody>
            {#if displayedTasks.length === 0}
              <tr>
                <td colspan="4" style="text-align:center;padding:24px;color:var(--color-text-secondary);">
                  当前筛选下没有转码任务
                </td>
              </tr>
            {:else}
              {#each displayedTasks as task (task.id)}
                <tr>
                  <td>
                    <code style="padding:2px 6px;background:var(--color-bg-subtle);border-radius:3px;">{task.id}</code>
                  </td>
                  <td>{task.source}</td>
                  <td>{task.size}</td>
                  <td>
                    <span class="sbadge {task.status === 'completed' ? 'sb-success' : task.status === 'processing' ? 'sb-warning' : task.status === 'failed' ? 'sb-danger' : 'sb-gray'}">
                      {task.status === 'completed' ? '已转码' : task.status === 'processing' ? '转码中' : task.status === 'failed' ? '失败' : '排队中'}
                    </span>
                  </td>
                </tr>
              {/each}
            {/if}
          </tbody>
        </table>
      </div>

      <footer class="app-card__foot" style="margin-top:14px;display:flex;align-items:center;justify-content:space-between;">
        <button type="button" class="text-link" style="font-size:12px;background:none;border:none;cursor:pointer;" disabled={refreshing} onclick={handleRefreshQueue}>
          {refreshing ? '刷新中…' : '刷新队列'}
        </button>
        <Button text="导出任务" variant="secondary" size="sm" onclick={exportTasks} />
      </footer>
    </div>
  </section>

  <!-- 卡片 2：来源白名单与安全（写操作 = 按钮 → Dialog） -->
  <section class="app-card" style="margin-bottom:14px;">
    <header class="app-card__head" style="display:flex;align-items:center;justify-content:space-between;gap:8px;flex-wrap:wrap;">
      <h2>来源白名单与安全</h2>
      <Button text="编辑白名单与安全" variant="primary" size="sm" onclick={openWhitelist} />
    </header>
    <div class="app-card__body">
      {#if form?.message && !dismissedMessage && form?.whitelistConfig !== undefined}
        <div
          class="alert {form?.message?.includes('不正确') || form?.message?.includes('失败') ? 'alert-danger' : 'alert-info'}"
          role="status"
          style="margin-bottom:12px;padding:10px 14px;background:var(--color-bg-subtle);border-radius:var(--radius-sm);font-size:13px;display:flex;justify-content:space-between;align-items:center;"
        >
          <span>{form.message}</span>
          <button type="button" class="btn ghost sm" onclick={() => (dismissedMessage = true)}>关闭</button>
        </div>
      {/if}
      <div class="text-secondary" style="font-size:13px;line-height:1.6;">
        当前配置：嵌入解析{enableEmbed ? '已启用' : '已停用'} · 白名单域名 {domainWhitelist.split(/[\n,]/).map((d) => d.trim()).filter(Boolean).length} 个 ·
        {strictMode === 'strict' ? '严格模式' : '宽松模式'} ·
        回退：{fallbackMode === 'safe_link' ? '显示安全链接' : fallbackMode === 'placeholder' ? '显示占位图' : '完全隐藏'}
      </div>
    </div>
  </section>

  <!-- 逐 Provider 策略配置（折叠收纳；写操作 = 按钮 → Dialog） -->
  {#if items.length > 0}
    <details class="app-card">
      <summary class="app-card__head" style="cursor:pointer;user-select:none;">
        <h2 style="display:inline-block;font-size:15px;margin:0;">逐 Provider 详细策略</h2>
      </summary>
      <div class="app-card__body" style="padding-top:12px;display:flex;flex-direction:column;gap:14px;">
        {#each items as item}
          <div style="border:1px solid var(--color-border);padding:14px;border-radius:var(--radius-sm);">
            <b>{videoProviderLabel(item.provider)}（{item.provider}）</b>
            {#if !item.enabled}
              <span class="sbadge sb-gray" style="margin-left:8px;">已停用</span>
            {/if}
            <div style="font-size:12px;color:var(--color-text-secondary);margin:4px 0 10px;">
              审计：策略版本 v{item.policy_version ?? 1} · 更新于 最近 · 服务端写入审计
            </div>
            <div style="display:flex;gap:8px;align-items:center;flex-wrap:wrap;">
              <Button text="保存策略" variant="primary" size="sm" onclick={() => openPolicy(item, 'save')} />
              <Button text="测试" variant="secondary" size="sm" onclick={() => openPolicy(item, 'test')} />
              <Button text="恢复默认" variant="ghost" size="sm" onclick={() => openReset(item)} />
            </div>
          </div>
        {/each}
      </div>
    </details>
  {/if}
{/if}

<!-- 白名单与安全 Dialog（?/save-whitelist） -->
<Dialog
  open={whitelistOpen}
  title="编辑白名单与安全"
  description="域名仅允许填写域名本身（不含协议前缀）；保存后立即影响新视频解析。"
  onclose={closeWhitelist}
>
  <form
    method="POST"
    action="?/save-whitelist"
    use:enhance={() => {
      dismissedMessage = false;
      return async ({ result, update }) => {
        await update();
        if (result.type === 'success') {
          showToast('视频白名单与安全配置已保存', 'success');
          closeWhitelist();
        } else if (result.type === 'failure') {
          showToast((result.data as any)?.message ?? '保存失败，请检查输入', 'danger');
        }
      };
    }}
    class="stack"
    style="gap:14px;"
  >
    <label style="display:flex;align-items:center;gap:8px;font-size:13px;font-weight:600;cursor:pointer;">
      <input type="checkbox" name="enable_embed" bind:checked={enableEmbed} />
      启用视频嵌入解析
    </label>

    <label>
      <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">域名白名单（逗号分隔）</span>
      <input type="text" name="domain_whitelist" class="input-field" bind:value={domainWhitelist} style="width:100%;" />
    </label>

    <label>
      <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">严格模式</span>
      <select class="app-select" name="strict_mode" bind:value={strictMode} style="width:100%;">
        <option value="strict">严格模式（仅允许完全匹配白名单）</option>
        <option value="loose">宽松模式</option>
      </select>
    </label>

    <label>
      <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">解析失败回退</span>
      <select class="app-select" name="fallback_mode" bind:value={fallbackMode} style="width:100%;">
        <option value="safe_link">显示安全链接</option>
        <option value="placeholder">显示占位图</option>
        <option value="hide">完全隐藏</option>
      </select>
    </label>

    <div>
      <button type="submit" class="btn primary">保存白名单</button>
    </div>
  </form>
</Dialog>

<!-- 保存 Provider 策略 Dialog（?/save：If-Match expected_version + reason 审计） -->
<Dialog
  open={policyTarget !== null && policyMode === 'save'}
  title={`保存策略 — ${policyTarget ? videoProviderLabel(policyTarget.provider) : ''}`}
  description={policyTarget ? `更新 ${policyTarget.provider} 策略（当前 v${policyTarget.policy_version ?? 1}，If-Match 乐观锁）；保存立即影响新解析。` : ''}
  onclose={closePolicy}
>
  <form method="POST" action="?/save" use:enhance={dialogEnhance(closePolicy)} class="stack" style="gap:12px;">
    <input type="hidden" name="provider" value={policyTarget?.provider ?? ''} />
    <input type="hidden" name="expected_version" value={String(policyTarget?.policy_version ?? 1)} />
    <label style="display:flex;align-items:center;gap:8px;font-size:13px;font-weight:600;cursor:pointer;">
      <input type="checkbox" name="enabled" value="true" bind:checked={policyEnabled} />
      启用该 Provider
    </label>
    <!-- 未勾选时提交 enabled=false（checkbox 不随表单提交，由隐藏域兜底） -->
    <input type="hidden" name="enabled" value="false" />
    <label>
      <span class="field-label" style="display:block;margin-bottom:4px;">允许播放域名（逗号分隔）</span>
      <input type="text" name="allowed_hosts" class="input-field" bind:value={policyHosts} style="width:100%;" />
    </label>
    <label>
      <span class="field-label" style="display:block;margin-bottom:4px;">允许嵌入域名（逗号分隔）</span>
      <input type="text" name="embed_hosts" class="input-field" bind:value={policyEmbedHosts} style="width:100%;" />
    </label>
    <label>
      <span class="field-label" style="display:block;margin-bottom:4px;">允许媒体类型（逗号分隔）</span>
      <input type="text" name="allowed_media_types" class="input-field" bind:value={policyMediaTypes} style="width:100%;" />
    </label>
    <div style="display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:8px;">
      {#each NUMERIC_FIELD_LABELS as [field, label] (field)}
        <label>
          <span class="field-label" style="display:block;margin-bottom:4px;">{label}（空=清除）</span>
          <input type="text" name={field} class="input-field" bind:value={policyNum[field]} />
        </label>
      {/each}
    </div>
    <label>
      <span class="field-label" style="display:block;margin-bottom:4px;">操作原因（写审计，必填）</span>
      <input type="text" name="reason" class="input-field" bind:value={policyReason} required placeholder="必填（写审计）" />
    </label>
    <Button text="保存策略" variant="primary" size="sm" type="submit" />
  </form>
</Dialog>

<!-- 测试 Provider Dialog（?/test：按表单候选值测试，Idempotency-Key 幂等） -->
<Dialog
  open={policyTarget !== null && policyMode === 'test'}
  title={`测试 — ${policyTarget ? videoProviderLabel(policyTarget.provider) : ''}`}
  description={policyTarget ? `按下方候选配置测试 ${policyTarget.provider} 连通性（结果脱敏，不回显内部探测详情）。` : ''}
  onclose={closePolicy}
>
  <form method="POST" action="?/test" use:enhance={dialogEnhance(closePolicy)} class="stack" style="gap:12px;">
    <input type="hidden" name="provider" value={policyTarget?.provider ?? ''} />
    <input type="hidden" name="expected_version" value={String(policyTarget?.policy_version ?? 1)} />
    <label>
      <span class="field-label" style="display:block;margin-bottom:4px;">允许播放域名（逗号分隔）</span>
      <input type="text" name="allowed_hosts" class="input-field" bind:value={policyHosts} style="width:100%;" />
    </label>
    <label>
      <span class="field-label" style="display:block;margin-bottom:4px;">允许嵌入域名（逗号分隔）</span>
      <input type="text" name="embed_hosts" class="input-field" bind:value={policyEmbedHosts} style="width:100%;" />
    </label>
    <label>
      <span class="field-label" style="display:block;margin-bottom:4px;">允许媒体类型（逗号分隔）</span>
      <input type="text" name="allowed_media_types" class="input-field" bind:value={policyMediaTypes} style="width:100%;" />
    </label>
    <label>
      <span class="field-label" style="display:block;margin-bottom:4px;">操作原因（写审计，必填）</span>
      <input type="text" name="reason" class="input-field" bind:value={policyReason} required placeholder="必填（写审计）" />
    </label>
    <Button text="开始测试" variant="secondary" size="sm" type="submit" />
  </form>
</Dialog>

<!-- 恢复默认策略 Dialog（?/reset-default：reason 审计） -->
<Dialog
  open={resetTarget !== null}
  title={`恢复默认策略 — ${resetTarget ? videoProviderLabel(resetTarget.provider) : ''}`}
  description={resetTarget ? `将 ${resetTarget.provider} 策略恢复为默认值（If-Match 乐观锁，写审计）。` : ''}
  onclose={closeReset}
>
  <form method="POST" action="?/reset-default" use:enhance={dialogEnhance(closeReset)} class="stack" style="gap:12px;">
    <input type="hidden" name="provider" value={resetTarget?.provider ?? ''} />
    <input type="hidden" name="expected_version" value={String(resetTarget?.policy_version ?? 1)} />
    <label>
      <span class="field-label" style="display:block;margin-bottom:4px;">操作原因（写审计）</span>
      <input type="text" name="reason" class="input-field" bind:value={resetReason} placeholder="留空则记录「恢复 Provider 默认策略」" />
    </label>
    <Button text="确认恢复默认" variant="primary" size="sm" type="submit" />
  </form>
</Dialog>
