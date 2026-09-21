<script lang="ts">
  // M13-UI-06 & M18-ADMIN-PLUGINS & M18-ADMIN-BATCH：管理插件页。
  // 约定 A：所有写操作 = 按钮 → Dialog（安装/启用/停用/设置；表单在弹层内，
  // reason 必填写审计，成功后 toastActionResult → update → 关闭弹层）。
  // 约定 B：插件表格选择列 + BatchBar →「批量启用」「批量停用」Dialog
  // （循环 enable/disable 单条端点，If-Match policy_revision 与单条一致）。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { enhance } from '$app/forms';
  import { invalidateAll } from '$app/navigation';
  import type { SubmitFunction } from '@sveltejs/kit';
  import Button from '$lib/components/ui/Button.svelte';
  import Dialog from '$lib/components/ui/Dialog.svelte';
  import BatchBar from '$lib/components/admin/BatchBar.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { toastActionResult } from '$lib/ui/action-toast';
  import { show as showToast } from '$lib/ui/toast';
  import type { AdminPluginsPageData, AdminPluginsActionData } from './+page.server';

  let { data, form }: { data: AdminPluginsPageData; form?: AdminPluginsActionData | null } = $props();

  const pageState = $derived(data.state);
  const plugins = $derived(data.plugins ?? []);
  const message = $derived(form?.message ?? null);

  // JS 启用：动作结果走全局 Toast 浮窗（成功绿/失败红）；顶部内联横幅仅保留为
  // 无 JS 回退（SSR HTML 仍渲染，见 hasJs）。
  let hasJs = $state(false);
  $effect(() => {
    hasJs = true;
  });

  // —— step-up 重新验证（M02-MFA-07）：高敏操作命中 403 step_up_required 时展示 ——
  let reauthLoading = $state(false);
  let reauthCancelled = $state(false);
  let reauthError = $state<string | null>(null);

  $effect(() => {
    if (form?.stepUpRequired) reauthCancelled = false;
  });

  interface UnifiedPlugin {
    id: string;
    name: string;
    description?: string;
    kind?: string;
    status?: string;
    capabilities?: string[];
    subscriptions?: string[];
    version?: number | string;
    policy_revision?: number;
  }


  // P0 整改：只展示服务端返回的插件；空列表 = 真实空态。
  const effectivePlugins: UnifiedPlugin[] = $derived(
    (plugins as unknown as UnifiedPlugin[]) ?? []
  );

  /** 行乐观锁版本：真实插件取 policy_revision，回退投影取 version。 */
  function revisionOf(p: UnifiedPlugin): number {
    return Number(p.policy_revision ?? p.version ?? 1);
  }

  let q = $state('');
  let statusFilter = $state('');
  let selectedIds = $state<string[]>([]);

  const displayedPlugins = $derived.by((): UnifiedPlugin[] => {
    let list: UnifiedPlugin[] = effectivePlugins;
    if (q.trim()) {
      const kw = q.trim().toLowerCase();
      list = list.filter((p) => p.name.toLowerCase().includes(kw) || (p.description ?? '').toLowerCase().includes(kw));
    }
    if (statusFilter === 'enabled') list = list.filter((p) => p.status === 'enabled');
    if (statusFilter === 'disabled') list = list.filter((p) => p.status === 'disabled');
    return list;
  });

  let allSelected = $derived(
    displayedPlugins.length > 0 && selectedIds.length === displayedPlugins.length
  );
  function toggleAll() {
    if (allSelected) selectedIds = [];
    else selectedIds = displayedPlugins.map((p) => p.id);
  }
  function toggleRow(id: string) {
    if (selectedIds.includes(id)) selectedIds = selectedIds.filter((x) => x !== id);
    else selectedIds = [...selectedIds, id];
  }
  function pluginById(id: string): UnifiedPlugin | undefined {
    return effectivePlugins.find((p) => p.id === id);
  }

  /** 弹层表单共用结果处理：toast → update → 成功才关弹层（失败留在弹层改）。 */
  const dialogEnhance = (onSuccess: () => void): SubmitFunction =>
    () => async ({ result, update }) => {
      toastActionResult(result);
      await update();
      if (result.type === 'success') onSuccess();
    };

  // ── 弹层 target 状态（一个 Dialog 服务一类操作，target 区分行）──
  /** 安装插件（?/install：默认 disabled 隔离态）。 */
  let installOpen = $state(false);
  function openInstall(): void {
    installOpen = true;
  }
  function closeInstall(): void {
    installOpen = false;
  }

  /**
   * 行操作弹层（M18-ADMIN-OPS 约定 C：每行一个「操作」按钮，动作在弹层内选）。
   * 插件行动作 = 启用（?/enable，If-Match policy_revision）/ 停用（?/disable）/
   * 设置（?/settings，settings_json 预填）——不同端点 → 弹层内按 tab 分节表单。
   */
  let opsTarget = $state<UnifiedPlugin | null>(null);
  let opsTab = $state<'enable' | 'disable' | 'settings'>('settings');

  function openOps(p: UnifiedPlugin): void {
    opsTarget = p;
    opsTab = p.status === 'enabled' ? 'disable' : 'enable';
    // 预填设置草稿（沿用既有 openSettings 逻辑）
    const real = plugins.find((x) => x.id === p.id);
    settingsJson = real ? JSON.stringify(real.settings ?? {}, null, 2) : '{}';
    enableReason = '';
    disableReason = '';
    settingsReason = '';
  }

  function closeOps(): void {
    opsTarget = null;
  }

  /** 启用分节表单（?/enable：If-Match policy_revision + reason）。 */
  let enableReason = $state('');
  /** 停用分节表单（?/disable：If-Match policy_revision + reason）。 */
  let disableReason = $state('');
  /** 设置分节表单（?/settings：settings_json + If-Match + reason）。 */
  let settingsJson = $state('{}');
  let settingsReason = $state('');

  /** 批量启用 / 批量停用（?/batchEnable、?/batchDisable）。 */
  let batchEnableOpen = $state(false);
  let batchDisableOpen = $state(false);
  let batchEnableReason = $state('');
  let batchDisableReason = $state('');
  function openBatchEnable(): void {
    batchEnableReason = '';
    batchEnableOpen = true;
  }
  function closeBatchEnable(): void {
    batchEnableOpen = false;
  }
  function openBatchDisable(): void {
    batchDisableReason = '';
    batchDisableOpen = true;
  }
  function closeBatchDisable(): void {
    batchDisableOpen = false;
  }

  /** 批量隐藏域：versions 与 ids 顺序一一对应（If-Match policy_revision）。 */
  const selectedVersions = $derived(
    selectedIds.map((id) => String(revisionOf(pluginById(id) ?? { id, name: '' })))
  );

  function batchDialogEnhance(onSuccess: () => void): SubmitFunction {
    return () => async ({ result, update }) => {
      toastActionResult(result);
      await update();
      if (result.type === 'success') {
        selectedIds = [];
        onSuccess();
      }
    };
  }

  function exportPluginsList() {
    const dataToExport = {
      plugins: effectivePlugins,
      exported_at: new Date().toISOString()
    };
    const blob = new Blob([JSON.stringify(dataToExport, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `plugins-list-${new Date().toISOString().slice(0, 10)}.json`;
    a.click();
    URL.revokeObjectURL(url);
    showToast('插件清单已成功导出为 JSON', 'success');
  }
</script>

<svelte:head>
  <title>插件管理 — BBLBB Admin</title>
</svelte:head>

<PageHeader title="插件管理" />

{#if pageState === 'forbidden'}
  <div class="app-card">
    <div class="app-card__body" role="alert">
      <p class="input-hint is-error">无权限（需要 plugin.manage 权限）。</p>
    </div>
  </div>
{:else if pageState === 'error'}
  <div class="app-card">
    <div class="app-card__body" role="alert">
      <p class="input-hint is-error">{data.error || '加载插件失败'}</p>
    </div>
  </div>
{:else}
  {#if message && !hasJs}
    <div class="alert alert-info" role="status" style="margin-bottom:12px;">{message}</div>
  {/if}

  <!-- 4 个统计卡（原型同款） -->
  <div style="display:grid;grid-template-columns:repeat(2, 1fr);gap:14px;margin-bottom:14px;">
    <div class="app-card" style="padding:16px;">
      <div class="text-secondary" style="font-size:12px;margin-bottom:4px;">插件总数</div>
      <div style="font-size:26px;font-weight:700;line-height:1.2;">{effectivePlugins.length}</div>
      <div class="text-secondary" style="font-size:11px;margin-top:4px;">已注册插件</div>
    </div>
    <div class="app-card" style="padding:16px;">
      <div class="text-secondary" style="font-size:12px;margin-bottom:4px;">运行中</div>
      <div style="font-size:26px;font-weight:700;line-height:1.2;color:var(--color-success);">
        {effectivePlugins.filter(p => p.status === 'enabled').length}
      </div>
      <div class="text-secondary" style="font-size:11px;margin-top:4px;">前台正常提供服务</div>
    </div>
    <div class="app-card" style="padding:16px;">
      <div class="text-secondary" style="font-size:12px;margin-bottom:4px;">已停用</div>
      <div style="font-size:26px;font-weight:700;line-height:1.2;color:var(--color-danger);">
        {effectivePlugins.filter(p => p.status === 'disabled').length}
      </div>
      <div class="text-secondary" style="font-size:11px;margin-top:4px;">未激活插件</div>
    </div>
    <div class="app-card" style="padding:16px;">
      <div class="text-secondary" style="font-size:12px;margin-bottom:4px;">最近检查</div>
      <div style="font-size:22px;font-weight:700;line-height:1.4;">刚刚</div>
      <div class="text-secondary" style="font-size:11px;margin-top:4px;">本地 Mock 投影</div>
    </div>
  </div>

  <!-- 插件列表卡片 -->
  <section class="app-card" style="margin-bottom:14px;">
    <header class="app-card__head" style="display:flex;justify-content:space-between;align-items:center;flex-wrap:wrap;gap:10px;">
      <div>
        <h2 style="margin:0;">插件列表</h2>
        <span class="app-muted" style="font-size:12px;">管理已注册插件的状态、权限和运行日志</span>
      </div>
      <div style="display:flex;gap:8px;">
        <Button text="+ 安装插件" variant="primary" size="sm" onclick={openInstall} />
        <button
          type="button"
          class="btn ghost sm"
          onclick={async () => {
            try {
              await invalidateAll();
              showToast('插件状态已同步', 'success');
            } catch {
              showToast('刷新失败，请重试', 'danger');
            }
          }}
        >
          <Icon name="rotate-cw" size={12} /> 刷新
        </button>
      </div>
    </header>

    <div class="app-card__body">
      <!-- 工具栏：单行 flex（窄屏自动换行；修复全宽 select 挤压清除按钮的问题） -->
      <div style="display:flex;align-items:center;flex-wrap:wrap;gap:8px;margin-bottom:14px;">
        <input
          type="search"
          bind:value={q}
          class="app-field"
          placeholder="搜索当前列表..."
          aria-label="搜索当前列表"
          style="flex:1 1 220px;min-width:0;"
        />
        <select
          class="app-select"
          bind:value={statusFilter}
          aria-label="状态筛选"
          style="flex:0 0 auto;width:168px;"
        >
          <option value="">全部状态</option>
          <option value="enabled">运行中</option>
          <option value="disabled">已停用</option>
        </select>
        {#if q || statusFilter}
          <button type="button" class="btn ghost sm" style="flex:0 0 auto;" onclick={() => { q = ''; statusFilter = ''; }}>
            清除
          </button>
        {/if}
      </div>

      <!-- 批量工具条（约定 B：选中后渲染） -->
      <BatchBar count={selectedIds.length} noun="个插件" onclear={() => (selectedIds = [])}>
        <Button text="批量启用" variant="secondary" size="sm" onclick={openBatchEnable} />
        <Button text="批量停用" variant="danger" size="sm" onclick={openBatchDisable} />
      </BatchBar>

      <div class="app-table-wrap">
        <table class="app-table" aria-label="插件列表">
          <thead>
            <tr>
              <th style="width:40px;text-align:center;">
                <input
                  type="checkbox"
                  checked={allSelected}
                  onchange={toggleAll}
                  aria-label="全选当前列表"
                />
              </th>
              <th style="min-width:200px;">插件</th>
              <th>类型</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            {#each displayedPlugins as p (p.id)}
              <tr>
                <td style="text-align:center;">
                  <input
                    type="checkbox"
                    checked={selectedIds.includes(p.id)}
                    onchange={() => toggleRow(p.id)}
                    aria-label="选择插件 {p.name}"
                  />
                </td>
                <td>
                  <div style="display:flex;align-items:center;gap:12px;">
                    <div style="width:36px;height:36px;border-radius:var(--radius-md);background:var(--color-bg-subtle);display:grid;place-items:center;flex:0 0 auto;">
                      <Icon name="puzzle" size={18} />
                    </div>
                    <div>
                      <b>{p.name}</b>
                      <code style="font-size:11px;color:var(--color-text-secondary);display:block;">/{p.id}</code>
                      {#if p.description}
                        <span class="sub" style="display:block;margin-top:2px;font-size:12px;color:var(--color-text-secondary);">{p.description}</span>
                      {/if}
                      {#if p.capabilities && p.capabilities.length > 0}
                        <div style="display:flex;gap:4px;margin-top:4px;flex-wrap:wrap;">
                          {#each p.capabilities as cap}
                            <span class="sbadge sb-brand" style="font-size:10px;">{cap}</span>
                          {/each}
                        </div>
                      {/if}
                      {#if p.subscriptions && p.subscriptions.length > 0}
                        <div style="display:flex;gap:4px;margin-top:2px;flex-wrap:wrap;">
                          {#each p.subscriptions as sub}
                            <span class="sbadge sb-gray" style="font-size:10px;">{sub}</span>
                          {/each}
                        </div>
                      {/if}
                    </div>
                  </div>
                </td>
                <td>
                  <span class="sbadge {p.status === 'enabled' ? 'sb-success' : 'sb-gray'}">
                    {p.status === 'enabled' ? '运行中' : '已停用'}
                  </span>
                  <span class="text-secondary" style="font-size:11px;display:block;margin-top:2px;">policy v{revisionOf(p)}</span>
                </td>
                <td>
                  <div style="display:flex;gap:6px;flex-wrap:wrap;">
                    <!-- 每行一个「操作」按钮：启用/停用/设置在弹层内选择（约定 C） -->
                    <Button text="操作" variant="secondary" size="sm" onclick={() => openOps(p)} />
                  </div>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>

      <footer class="app-card__foot" style="margin-top:14px;display:flex;align-items:center;justify-content:space-between;font-size:12px;color:var(--color-text-secondary);">
        <span>不支持上传和执行任意插件代码（沙箱安全保护）</span>
        <button type="button" class="text-link" style="font-size:12px;background:none;border:none;cursor:pointer;padding:0;" onclick={exportPluginsList}>导出清单</button>
      </footer>
    </div>
  </section>

  <!-- 能力边界说明（折叠收纳）：v1 无在线代码执行；受控 Provider Adapter -->
  <details class="app-card">
    <summary class="app-card__head" style="cursor:pointer;user-select:none;">
      <h2 style="display:inline-block;font-size:15px;margin:0;">v1 能力边界</h2>
    </summary>
    <div class="app-card__body" style="padding-top:12px;font-size:12px;color:var(--color-text-secondary);line-height:1.6;">
      <p style="margin:0 0 8px;">
        插件是配置数据，无在线代码执行路径（code/WASM plugin execution is a v2 research item）。受控 Provider Adapter（随应用编译）：direct、hls、xigua。
      </p>
      <p style="margin:0;">
        安装插件请使用右上角「+ 安装插件」按钮（弹层表单，默认 disabled 隔离态，reason 写审计）。
      </p>
    </div>
  </details>
{/if}

<!-- 安装插件 Dialog（?/install：默认 disabled 隔离态，reason 写审计） -->
<Dialog
  open={installOpen}
  title="安装插件"
  description="安装后插件为 disabled 隔离态，需在列表中手动启用；ID 仅限小写字母/数字/连字符。"
  onclose={closeInstall}
>
  <form method="POST" action="?/install" use:enhance={dialogEnhance(closeInstall)} class="stack" style="gap:10px;">
    <label>
      <span class="field-label" style="font-weight:600;display:block;margin-bottom:4px;">插件 ID</span>
      <input type="text" name="id" class="input-field" placeholder="如：welcome-reward" required />
    </label>
    <label>
      <span class="field-label" style="font-weight:600;display:block;margin-bottom:4px;">名称</span>
      <input type="text" name="name" class="input-field" placeholder="如：新用户欢迎奖励" required />
    </label>
    <label>
      <span class="field-label" style="font-weight:600;display:block;margin-bottom:4px;">能力 (JSON 数组)</span>
      <input type="text" name="capabilities" class="input-field" value='["notification.create"]' required />
    </label>
    <label>
      <span class="field-label" style="font-weight:600;display:block;margin-bottom:4px;">订阅事件 (JSON 数组)</span>
      <input type="text" name="subscriptions" class="input-field" value='["user.verified.v1"]' required />
    </label>
    <label>
      <span class="field-label" style="font-weight:600;display:block;margin-bottom:4px;">设置 Schema (JSON)</span>
      <textarea name="settings_schema" class="input-field" rows="3" value={'{"type":"object","properties":{}}'}></textarea>
    </label>
    <label>
      <span class="field-label" style="font-weight:600;display:block;margin-bottom:4px;">操作原因（写审计）</span>
      <input type="text" name="reason" class="input-field" placeholder="必填" required />
    </label>
    <Button text="安装插件" variant="primary" type="submit" />
  </form>
</Dialog>

{#if pageState !== 'forbidden' && pageState !== 'error'}
  <!-- 行操作 Dialog（约定 C）：chips 选动作（启用/停用/设置），分节表单 → 既有契约 -->
  <Dialog
    open={opsTarget !== null}
    title={opsTarget ? `插件操作：${opsTarget.name}` : '插件操作'}
    description={opsTarget
      ? `对「${opsTarget.name}」（/${opsTarget.id}，policy v${revisionOf(opsTarget)}）执行操作；启停/设置均带 If-Match 乐观锁，原因写审计。`
      : ''}
    onclose={closeOps}
  >
    {#if opsTarget}
      <!-- 动作 chips -->
      <div style="display:flex;gap:8px;flex-wrap:wrap;margin-bottom:var(--space-4);">
        <button
          type="button"
          class="btn sm {opsTab === 'enable' ? 'secondary' : 'ghost'}"
          style={opsTab === 'enable' ? 'border:1px solid var(--color-brand);font-weight:600;' : ''}
          onclick={() => (opsTab = 'enable')}
        >
          启用
        </button>
        <button
          type="button"
          class="btn sm {opsTab === 'disable' ? 'secondary' : 'ghost'}"
          style={opsTab === 'disable' ? 'border:1px solid var(--color-brand);font-weight:600;' : ''}
          onclick={() => (opsTab = 'disable')}
        >
          停用
        </button>
        <button
          type="button"
          class="btn sm {opsTab === 'settings' ? 'secondary' : 'ghost'}"
          style={opsTab === 'settings' ? 'border:1px solid var(--color-brand);font-weight:600;' : ''}
          onclick={() => (opsTab = 'settings')}
        >
          设置
        </button>
      </div>

      {#if opsTab === 'enable'}
        <!-- 启用（?/enable：If-Match policy_revision + reason 审计） -->
        <form method="POST" action="?/enable" use:enhance={dialogEnhance(closeOps)} class="stack" style="gap:10px;">
          <input type="hidden" name="id" value={opsTarget?.id ?? ''} />
          <input type="hidden" name="policy_revision" value={opsTarget ? String(revisionOf(opsTarget)) : ''} />
          <label>
            <span class="field-label" style="font-weight:600;display:block;margin-bottom:4px;">操作原因（写审计）</span>
            <input type="text" name="reason" class="input-field" bind:value={enableReason} placeholder="必填" required />
          </label>
          <Button text="确认启用" variant="primary" size="sm" type="submit" />
        </form>
      {:else if opsTab === 'disable'}
        <!-- 停用（?/disable：If-Match policy_revision + reason 审计） -->
        <form method="POST" action="?/disable" use:enhance={dialogEnhance(closeOps)} class="stack" style="gap:10px;">
          <input type="hidden" name="id" value={opsTarget?.id ?? ''} />
          <input type="hidden" name="policy_revision" value={opsTarget ? String(revisionOf(opsTarget)) : ''} />
          <label>
            <span class="field-label" style="font-weight:600;display:block;margin-bottom:4px;">操作原因（写审计）</span>
            <input type="text" name="reason" class="input-field" bind:value={disableReason} placeholder="必填" required />
          </label>
          <Button text="确认停用" variant="danger" size="sm" type="submit" />
        </form>
      {:else}
        <!-- 设置（?/settings：settings_json + If-Match + reason 审计） -->
        <form method="POST" action="?/settings" use:enhance={dialogEnhance(closeOps)} class="stack" style="gap:10px;">
          <input type="hidden" name="id" value={opsTarget?.id ?? ''} />
          <input type="hidden" name="policy_revision" value={opsTarget ? String(revisionOf(opsTarget)) : ''} />
          <label>
            <span class="field-label" style="font-weight:600;display:block;margin-bottom:4px;">设置 JSON</span>
            <textarea name="settings_json" class="input-field" rows="6" bind:value={settingsJson}></textarea>
          </label>
          <label>
            <span class="field-label" style="font-weight:600;display:block;margin-bottom:4px;">操作原因（写审计）</span>
            <input type="text" name="reason" class="input-field" bind:value={settingsReason} placeholder="必填" required />
          </label>
          <Button text="保存设置" variant="primary" size="sm" type="submit" />
        </form>
      {/if}
    {/if}
  </Dialog>

  <!-- 批量启用 Dialog（?/batchEnable：循环 enable 单条端点） -->
  <Dialog
    open={batchEnableOpen}
    title="批量启用"
    description={`将启用 ${selectedIds.length} 个插件（逐条 If-Match policy_revision 提交，原因写审计）。`}
    onclose={closeBatchEnable}
  >
    <form method="POST" action="?/batchEnable" use:enhance={batchDialogEnhance(closeBatchEnable)} class="stack" style="gap:10px;">
      <input type="hidden" name="ids" value={selectedIds.join(',')} />
      <input type="hidden" name="versions" value={selectedVersions.join(',')} />
      <label>
        <span class="field-label" style="font-weight:600;display:block;margin-bottom:4px;">操作原因（写审计）</span>
        <input type="text" name="reason" class="input-field" bind:value={batchEnableReason} placeholder="必填" required />
      </label>
      <Button text={`批量启用 ${selectedIds.length} 个`} variant="primary" size="sm" type="submit" />
    </form>
  </Dialog>

  <!-- 批量停用 Dialog（?/batchDisable：循环 disable 单条端点） -->
  <Dialog
    open={batchDisableOpen}
    title="批量停用"
    description={`将停用 ${selectedIds.length} 个插件（逐条 If-Match policy_revision 提交，原因写审计）；停用后不再消费新事件。`}
    onclose={closeBatchDisable}
  >
    <form method="POST" action="?/batchDisable" use:enhance={batchDialogEnhance(closeBatchDisable)} class="stack" style="gap:10px;">
      <input type="hidden" name="ids" value={selectedIds.join(',')} />
      <input type="hidden" name="versions" value={selectedVersions.join(',')} />
      <label>
        <span class="field-label" style="font-weight:600;display:block;margin-bottom:4px;">操作原因（写审计）</span>
        <input type="text" name="reason" class="input-field" bind:value={batchDisableReason} placeholder="必填" required />
      </label>
      <Button text={`批量停用 ${selectedIds.length} 个`} variant="danger" size="sm" type="submit" />
    </form>
  </Dialog>
{/if}

<!-- step-up 重新验证（M02-MFA-07）：高敏操作命中 403 step_up_required 时展示。
     无 JS 时 Dialog 以固定层内联渲染，表单仍可用（SSR 基线保留）。 -->
<Dialog
  open={Boolean(form?.stepUpRequired) && !reauthCancelled}
  title="需要重新验证身份"
  description="插件管理属于高风险管理操作，要求近期重新认证。输入当前账号密码完成重新验证后，可继续刚才的操作。"
  onclose={() => (reauthCancelled = true)}
>
  {#if reauthError}
    <div class="alert alert-danger" role="alert" style="margin-bottom:10px;padding:8px 12px;font-size:12px;">
      {reauthError}
    </div>
  {/if}
  <form
    method="POST"
    action="?/reauth"
    use:enhance={() => {
      reauthLoading = true;
      reauthError = null;
      return async ({ result, update }) => {
        reauthLoading = false;
        if (result.type === 'failure') {
          reauthError = (result.data as unknown as AdminPluginsActionData | null)?.message ?? '密码验证失败，请重试';
          return;
        }
        toastActionResult(result);
        await update();
      };
    }}
    style="display:flex;flex-direction:column;gap:10px;"
  >
    <div>
      <label class="input-label" for="plugin-reauth-password">当前账号密码</label>
      <input class="input-field" type="password" id="plugin-reauth-password" name="password" autocomplete="current-password" required />
    </div>
    <div style="display:flex;gap:8px;">
      <Button text={reauthLoading ? '验证中…' : '重新验证'} variant="primary" type="submit" disabled={reauthLoading} />
      <button type="button" class="btn ghost sm" onclick={() => (reauthCancelled = true)}>取消</button>
    </div>
  </form>
</Dialog>
