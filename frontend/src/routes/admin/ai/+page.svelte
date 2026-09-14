<script lang="ts">
  // M09-UI-06 & M18-ADMIN-AI：大模型设置管理页。
  // 约定 A（按钮→弹层）：Provider 新增 = 按钮 + Dialog（?/saveProvider）；
  // 全站策略 = 按钮 + Dialog（?/save）。
  // M18-ADMIN-OPS（约定 D）：Provider 行「测试 / 编辑 / 删除」多个写按钮收敛为
  // 每行一个「⋮」三点菜单（RowActionsMenu）→ 点菜单项打开该操作的单动作确认
  // Dialog（编辑 ?/saveProvider；连接测试 ?/test 结果就地展示；删除
  // ?/deleteProvider reason 必填——danger 提交按钮）。
  // 任务行「重试 / 取消」两个写按钮收敛为「⋮」菜单 → Dialog 单动作确认
  // （重试 ?/retry；取消 ?/cancel reason 必填——danger 提交按钮）。
  // 任务表选择列 + BatchBar「批量重试」（?/batchRetry，服务端循环单条端点
  // /admin/ai/tasks/{id}/retry）保持不变。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import Dialog from '$lib/components/ui/Dialog.svelte';
  import BatchBar from '$lib/components/admin/BatchBar.svelte';
  import RowActionsMenu from '$lib/components/admin/RowActionsMenu.svelte';
  import { show as showToast } from '$lib/ui/toast';
  import { toastActionResult } from '$lib/ui/action-toast';
  import type { AiAdminTaskRow } from '$lib/api/types';
  import type { AdminAiActionData, AdminAiPageData } from './+page.server';

  let { data, form }: { data: AdminAiPageData; form?: AdminAiActionData | null } = $props();

  const loadState = $derived(data.state);
  const config = $derived(data.config);
  const tasks = $derived(data.tasks);
  const error = $derived(data.error);

  const channels = $derived(
    Array.isArray(config?.providers) && config.providers.length > 0
      ? config.providers.map((p) => {
          const rawStatus = (p as any).status === 'disabled' ? 'disabled' : ((p as any).status || 'enabled');
          const firstEnabled = config.providers?.find((item: any) => item.status !== 'disabled');
          return {
            id: p.id,
            initial: (p.name ?? 'P').charAt(0).toUpperCase() || 'P',
            name: p.name ?? '未命名提供商',
            adapter_type: (p as any).adapter_type || (p as any).api_type || 'openai_compatible',
            url: p.base_url || 'https://api.openai.com/v1',
            secretConfigured: p.secret_configured === true,
            status: p.secret_configured ? '密钥已配置' : '未配置密钥',
            rawStatus,
            isDefault: rawStatus === 'enabled' && firstEnabled?.id === p.id,
            models: [(p as any).default_model || (p as any).model || 'gpt-4o-mini']
          };
        })
      : []
  );

  const enabledChannels = $derived(channels.filter((c) => c.rawStatus === 'enabled'));

  /** 任务行视图（SSR 渲染用投影）。 */
  interface AiTaskRowView {
    id: string;
    task: string;
    source: string;
    errorCode?: string;
    status: string;
  }

  const taskList = $derived.by<AiTaskRowView[]>(() => {
    const list: any[] = Array.isArray(tasks) ? (tasks as any) : ((tasks as any)?.items ?? []);
    const out: AiTaskRowView[] = [];
    for (const t of list) {
      out.push({
        id: t.id,
        task: t.purpose || t.task_type || 'AI任务',
        source: t.error_code ? `错误：${t.error_code}` : '主题内容',
        errorCode: t.error_code,
        status: t.status
      });
    }
    return out;
  });

  // ── Provider 新增 Dialog（页头「添加渠道」入口；?/saveProvider） ──
  // 编辑分节复用同一组字段状态（providerFields snippet），两弹层互斥打开。
  let providerDialogOpen = $state(false);
  let providerName = $state('');
  let providerAdapterType = $state('openai_compatible');
  let providerBaseUrl = $state('');
  let providerDefaultModel = $state('');
  let providerStatus = $state('enabled');
  let providerApiKey = $state('');
  let providerSecretConfigured = $state(false);
  let providerReason = $state('');

  function resetProviderDraft(): void {
    providerName = '';
    providerAdapterType = 'openai_compatible';
    providerBaseUrl = '';
    providerDefaultModel = '';
    providerStatus = 'enabled';
    providerApiKey = '';
    providerSecretConfigured = false;
    providerReason = '';
  }

  function openAddProvider(): void {
    resetProviderDraft();
    providerReason = '添加 AI 模型渠道';
    providerDialogOpen = true;
  }

  function closeProviderDialog(): void {
    providerDialogOpen = false;
    resetProviderDraft();
  }

  // ── Provider 行操作「⋮」菜单 + 弹层（约定 D：菜单项决定动作，单动作确认） ──
  // 按 providerOpsAction 渲染对应表单节：编辑（?/saveProvider）/ 测试
  // （?/test，结果就地展示）/ 删除（?/deleteProvider，reason 必填，danger 提交按钮）。
  // 草稿随关闭清空。
  type ProviderOpsAction = 'edit' | 'test' | 'deleteProvider';
  let providerOpsTarget: (typeof channels)[number] | null = $state(null);
  let providerOpsAction = $state<ProviderOpsAction>('edit');
  let providerOpsTestBaseUrl = $state('');
  let providerOpsTestOutput: { ok: boolean; message: string } | null = $state(null);
  let providerOpsDeleteReason = $state('');

  function openProviderOps(ch: (typeof channels)[number], action: ProviderOpsAction): void {
    providerOpsTarget = ch;
    providerOpsAction = action;
    // 编辑分节草稿
    providerName = ch.name;
    providerAdapterType = ch.adapter_type || 'openai_compatible';
    providerBaseUrl = ch.url;
    providerDefaultModel = ch.models?.[0] || 'gpt-4o-mini';
    providerStatus = ch.rawStatus || 'enabled';
    providerApiKey = '';
    providerSecretConfigured = ch.secretConfigured;
    providerReason = `更新渠道 ${ch.name}`;
    // 测试分节草稿
    providerOpsTestBaseUrl = ch.url;
    providerOpsTestOutput = null;
    // 删除分节草稿
    providerOpsDeleteReason = '';
  }

  function closeProviderOps(): void {
    providerOpsTarget = null;
    providerOpsTestBaseUrl = '';
    providerOpsTestOutput = null;
    providerOpsDeleteReason = '';
    resetProviderDraft();
  }

  /** 渠道行「⋮」菜单项：编辑渠道 / 测试连接 / 删除渠道（危险）。 */
  function providerRowActions(ch: (typeof channels)[number]) {
    return [
      { label: '编辑渠道', run: () => openProviderOps(ch, 'edit') },
      { label: '测试连接', run: () => openProviderOps(ch, 'test') },
      { label: '删除渠道', danger: true, run: () => openProviderOps(ch, 'deleteProvider') }
    ];
  }

  /** 渠道弹层标题/描述随菜单选定动作切换（单动作确认，非分节选择）。 */
  const providerOpsActionMeta = $derived.by(() => {
    if (!providerOpsTarget) return null;
    switch (providerOpsAction) {
      case 'edit':
        return {
          title: `编辑渠道：${providerOpsTarget.name}`,
          description: '渠道名称与 Base URL 必填；API Key 仅写入受保护 Secret Store（不回显）。'
        };
      case 'test':
        return {
          title: `测试连接：${providerOpsTarget.name}`,
          description: '连接测试（固定脱敏探针）；结果就地展示，不会泄露或存储密钥。'
        };
      case 'deleteProvider':
        return {
          title: `删除渠道：${providerOpsTarget.name}`,
          description: '危险操作：删除后该渠道的任务将失去可用供应商，不可恢复；原因写审计。'
        };
    }
  });

  // ── 任务行操作「⋮」菜单 + 弹层（约定 D：单动作确认） ──
  // 按 taskOpsAction 渲染对应表单节：重试（?/retry）/ 取消（?/cancel，reason 必填，
  // danger 提交按钮）。
  type TaskOpsAction = 'retry' | 'cancel';
  let taskOpsTarget: AiTaskRowView | null = $state(null);
  let taskOpsAction = $state<TaskOpsAction>('retry');
  let taskOpsRetryReason = $state('');
  let taskOpsCancelReason = $state('');

  function openTaskOps(t: AiTaskRowView, action: TaskOpsAction): void {
    taskOpsTarget = t;
    taskOpsAction = action;
    taskOpsRetryReason = '';
    taskOpsCancelReason = '';
  }

  function closeTaskOps(): void {
    taskOpsTarget = null;
    taskOpsRetryReason = '';
    taskOpsCancelReason = '';
  }

  /** 任务行「⋮」菜单项：重试 / 取消（危险）。 */
  function taskRowActions(t: AiTaskRowView) {
    return [
      { label: '重试', run: () => openTaskOps(t, 'retry') },
      { label: '取消', danger: true, run: () => openTaskOps(t, 'cancel') }
    ];
  }

  /** 任务弹层标题/描述随菜单选定动作切换（单动作确认，非分节选择）。 */
  const taskOpsActionMeta = $derived.by(() => {
    if (!taskOpsTarget) return null;
    switch (taskOpsAction) {
      case 'retry':
        return {
          title: `重试任务：${taskOpsTarget.id}`,
          description: '重试将任务重新入队；操作原因写审计。'
        };
      case 'cancel':
        return {
          title: `取消任务：${taskOpsTarget.id}`,
          description: '危险操作：取消后任务不再执行，不可恢复；原因写审计。'
        };
    }
  });

  // ── 批量重试（约定 B：选择列 + BatchBar + 批量 Dialog → ?/batchRetry） ──
  let selectedTasks = $state(new Set<string>());
  function toggleSelectTask(id: string): void {
    const next = new Set(selectedTasks);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    selectedTasks = next;
  }
  const allTasksSelected = $derived(taskList.length > 0 && taskList.every((t) => selectedTasks.has(t.id)));
  function toggleSelectAllTasks(): void {
    if (allTasksSelected) selectedTasks = new Set();
    else selectedTasks = new Set(taskList.map((t) => t.id));
  }
  let batchRetryOpen = $state(false);

  // ── 全站策略 Dialog（?/save） ──
  let strategyOpen = $state(false);

  // JS 启用：动作结果走全局 Toast 浮窗；顶部内联横幅仅保留为无 JS 回退。
  let hasJs = $state(false);
  $effect(() => {
    hasJs = true;
  });
</script>

<svelte:head>
  <title>大模型设置 — BBLBB Admin</title>
</svelte:head>

<PageHeader title="大模型设置" />

{#if loadState === 'not_implemented'}
  <div class="app-card">
    <div class="app-card__body" role="status">
      <p class="input-hint">AI 管理接口开发中。核心论坛功能不受影响。</p>
    </div>
  </div>
{:else if loadState === 'forbidden'}
  <div class="app-card">
    <div class="app-card__body" role="alert">
      <p class="input-hint is-error">没有权限访问 AI 管理。</p>
    </div>
  </div>
{:else if loadState === 'error' && !config}
  <div class="app-card">
    <div class="app-card__body" role="alert">
      <p class="input-hint is-error">加载失败：{error}</p>
    </div>
  </div>
{:else}
  <div class="admin-ai-actions">
    <p>配置多个模型渠道，按业务场景选择最合适的模型。</p>
    <Button text="添加渠道" variant="primary" size="sm" onclick={openAddProvider} />
  </div>

  <!-- 4 个统计卡 -->
  <div style="display:grid;grid-template-columns:repeat(2, 1fr);gap:14px;margin-bottom:14px;">
    <div class="app-card" style="padding:16px;">
      <div class="text-secondary" style="font-size:12px;margin-bottom:4px;">已启用渠道</div>
      <div style="font-size:26px;font-weight:700;line-height:1.2;">
        {channels.filter(c => c.rawStatus === 'enabled').length} / {channels.length}
      </div>
    </div>
    <div class="app-card" style="padding:16px;">
      <div class="text-secondary" style="font-size:12px;margin-bottom:4px;">可用模型数</div>
      <div style="font-size:26px;font-weight:700;line-height:1.2;">
        {channels.reduce((acc, c) => acc + c.models.length, 0)}
      </div>
    </div>
    <div class="app-card" style="padding:16px;">
      <div class="text-secondary" style="font-size:12px;margin-bottom:4px;">数据模式</div>
      <div style="font-size:20px;font-weight:700;line-height:1.2;color:var(--color-brand);">
        {config?.data_mode ?? 'redacted'}
      </div>
      <div class="text-secondary" style="font-size:11px;margin-top:2px;">
        {config?.enabled ? '全站 AI 能力已启用' : '全站 AI 能力已关闭'}
      </div>
    </div>
    <div class="app-card" style="padding:16px;">
      <div class="text-secondary" style="font-size:12px;margin-bottom:4px;">每日预算上限</div>
      <div style="font-size:20px;font-weight:700;line-height:1.2;color:var(--color-brand);">
        {config?.budgets?.site_daily_tokens ? `${config.budgets.site_daily_tokens} tokens` : '不限'}
      </div>
      <div class="text-secondary" style="font-size:11px;margin-top:2px;">
        单用户上限：{config?.budgets?.per_user_daily_tokens ? `${config.budgets.per_user_daily_tokens} tokens` : '不限'}
      </div>
    </div>
  </div>

  <!-- 卡片 1：模型渠道（操作 = 按钮 + 弹层） -->
  <section class="app-card" style="margin-bottom:14px;">
    <header class="app-card__head" style="display:flex;justify-content:space-between;align-items:center;flex-wrap:wrap;gap:10px;">
      <div style="min-width:0;flex:1 1 200px;">
        <h2 style="margin:0;">模型渠道</h2>
        <span class="app-muted" style="font-size:12px;">配置大模型供应商与端点，支持添加、修改配置和安全删除。</span>
      </div>
      <div style="display:flex;align-items:center;gap:10px;flex-shrink:0;white-space:nowrap;">
        <span class="text-secondary" style="font-size:12px;">{channels.length} 个渠道</span>
        <Button text="添加渠道" variant="primary" size="sm" onclick={openAddProvider} />
      </div>
    </header>
    <div class="app-card__body" style="display:flex;flex-direction:column;gap:12px;">
      {#if channels.length === 0}
        <div style="border:1px dashed var(--color-border);border-radius:6px;padding:32px 16px;text-align:center;">
          <p style="margin:0 0 12px;font-size:13px;color:var(--color-text-secondary);">尚未配置任何 AI 模型渠道。</p>
          <Button text="添加第一个渠道" variant="primary" size="sm" onclick={openAddProvider} />
        </div>
      {:else}
        {#each channels as ch (ch.id)}
          <div class="app-card" style="border:1px solid var(--color-border);padding:14px;">
            <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:8px;">
              <div style="display:flex;align-items:center;gap:10px;">
                <div style="width:28px;height:28px;border-radius:4px;background:var(--color-bg-subtle);display:grid;place-items:center;font-weight:700;font-size:13px;">
                  {ch.initial}
                </div>
                <div>
                  <b style="font-size:14px;">{ch.name}</b>
                  <span class="text-secondary" style="display:block;font-size:11px;">{ch.url}</span>
                </div>
              </div>
              <div style="display:flex;gap:6px;align-items:center;">
                <span class="badge {ch.rawStatus === 'enabled' ? 'badge-success' : 'badge-neutral'}" style="font-size:11px;">
                  {ch.rawStatus === 'enabled' ? '已启用' : '已停用'}
                </span>
                <span class="badge {ch.secretConfigured ? 'badge-success' : 'badge-warning'}" style="font-size:11px;">
                  {ch.status}
                </span>
              </div>
            </div>

            <div style="display:flex;gap:6px;flex-wrap:wrap;margin:10px 0;align-items:center;">
              <span class="badge badge-neutral" style="font-size:11px;">类型: {ch.adapter_type}</span>
              {#each ch.models as m}
                <code style="padding:2px 6px;background:var(--color-bg-subtle);border-radius:3px;font-size:11px;">{m}</code>
              {/each}
            </div>

            <div style="display:flex;gap:8px;align-items:center;padding-top:6px;border-top:1px solid var(--color-border);">
              <!-- 每行一个「⋮」动作菜单（约定 D）：菜单项打开对应单动作确认 Dialog -->
              <RowActionsMenu
                label="更多操作：渠道 {ch.name}"
                actions={providerRowActions(ch)}
              />
              {#if ch.isDefault}
                <span class="text-secondary" style="margin-left:auto;font-size:11px;">默认渠道</span>
              {/if}
            </div>
          </div>
        {/each}
      {/if}
      <p class="text-secondary" style="font-size:11px;margin:4px 0 0;">
        密钥只写入受保护 Secret Store，任何页面都不会显示明文或片段。
      </p>
    </div>
  </section>

  <!-- 卡片 2：业务场景路由 -->
  <section class="app-card" style="margin-bottom:14px;">
    <header class="app-card__head" style="display:flex;justify-content:space-between;align-items:center;">
      <div>
        <h2 style="margin:0;">业务场景路由</h2>
        <span class="app-muted" style="font-size:12px;">不同功能可以使用不同渠道和模型，修改后新任务立即生效。</span>
      </div>
      <span class="text-secondary" style="font-size:12px;">按场景指定</span>
    </header>
    <div class="app-card__body" style="display:flex;flex-direction:column;gap:14px;">
      <div>
        <b style="font-size:13px;">草稿格式修复</b>
        <span class="text-secondary" style="display:block;font-size:11px;margin-bottom:6px;">整理 Markdown 结构与排版</span>
        <select class="app-select" style="width:100%;margin-bottom:6px;" disabled={enabledChannels.length === 0}>
          {#if enabledChannels.length > 0}
            {#each enabledChannels as c}<option>{c.name} · {c.adapter_type}</option>{/each}
          {:else}
            <option value="">暂无可用启用渠道（请先添加并启用渠道）</option>
          {/if}
        </select>
        <select class="app-select" style="width:100%;" disabled={enabledChannels.length === 0}>
          {#if enabledChannels.length > 0}
            {#each enabledChannels as c}{#each c.models as m}<option>{m}</option>{/each}{/each}
          {:else}
            <option value="">暂无可用模型</option>
          {/if}
        </select>
      </div>

      <div>
        <b style="font-size:13px;">内容摘要</b>
        <span class="text-secondary" style="display:block;font-size:11px;margin-bottom:6px;">生成主题摘要与通知预览</span>
        <select class="app-select" style="width:100%;margin-bottom:6px;" disabled={enabledChannels.length === 0}>
          {#if enabledChannels.length > 0}
            {#each enabledChannels as c}<option>{c.name} · {c.adapter_type}</option>{/each}
          {:else}
            <option value="">暂无可用启用渠道（请先添加并启用渠道）</option>
          {/if}
        </select>
        <select class="app-select" style="width:100%;" disabled={enabledChannels.length === 0}>
          {#if enabledChannels.length > 0}
            {#each enabledChannels as c}{#each c.models as m}<option>{m}</option>{/each}{/each}
          {:else}
            <option value="">暂无可用模型</option>
          {/if}
        </select>
      </div>

      <div>
        <b style="font-size:13px;">敏感词复核</b>
        <span class="text-secondary" style="display:block;font-size:11px;margin-bottom:6px;">发布前的内容安全检查</span>
        <select class="app-select" style="width:100%;margin-bottom:6px;" disabled={enabledChannels.length === 0}>
          {#if enabledChannels.length > 0}
            {#each enabledChannels as c}<option>{c.name} · {c.adapter_type}</option>{/each}
          {:else}
            <option value="">暂无可用启用渠道（请先添加并启用渠道）</option>
          {/if}
        </select>
        <select class="app-select" style="width:100%;" disabled={enabledChannels.length === 0}>
          {#if enabledChannels.length > 0}
            {#each enabledChannels as c}{#each c.models as m}<option>{m}</option>{/each}{/each}
          {:else}
            <option value="">暂无可用模型</option>
          {/if}
        </select>
      </div>

      <div>
        <b style="font-size:13px;">标题翻译</b>
        <span class="text-secondary" style="display:block;font-size:11px;margin-bottom:6px;">将标题翻译为站点默认语言</span>
        <select class="app-select" style="width:100%;margin-bottom:6px;" disabled={enabledChannels.length === 0}>
          {#if enabledChannels.length > 0}
            {#each enabledChannels as c}<option>{c.name} · {c.adapter_type}</option>{/each}
          {:else}
            <option value="">暂无可用启用渠道（请先添加并启用渠道）</option>
          {/if}
        </select>
        <select class="app-select" style="width:100%;" disabled={enabledChannels.length === 0}>
          {#if enabledChannels.length > 0}
            {#each enabledChannels as c}{#each c.models as m}<option>{m}</option>{/each}{/each}
          {:else}
            <option value="">暂无可用模型</option>
          {/if}
        </select>
      </div>

      <div style="display:flex;justify-content:space-between;align-items:center;margin-top:8px;">
        <span class="text-secondary" style="font-size:11px;">选择结果会写入服务端配置并记录审计。</span>
        <button type="button" class="btn primary sm" disabled title="场景路由保存 action 尚未接入">保存场景配置</button>
      </div>
    </div>
  </section>

  <!-- 卡片 3：全站数据策略与预算（只读概览 + 按钮 + Dialog 编辑） -->
  <section class="app-card" style="margin-bottom:14px;">
    <header class="app-card__head" style="display:flex;justify-content:space-between;align-items:center;gap:10px;flex-wrap:wrap;">
      <div>
        <h2 style="margin:0;">全站 AI 策略与预算</h2>
        <span class="app-muted" style="font-size:12px;">
          全站开关 / 数据脱敏级别（{config?.data_mode ?? 'redacted'}）/ 功能 Flag 与每日 Token 限制。
        </span>
      </div>
      <Button text="编辑全局策略" variant="primary" size="sm" onclick={() => (strategyOpen = true)} />
    </header>
  </section>

  <!-- 卡片 4：任务队列（行按钮 + 批量重试） -->
  <section class="app-card">
    <header class="app-card__head">
      <h2 style="margin:0;">任务队列</h2>
    </header>
    <div class="app-card__body">
      <BatchBar count={selectedTasks.size} noun="个任务" onclear={() => (selectedTasks = new Set())}>
        <Button text="批量重试" variant="secondary" size="sm" onclick={() => (batchRetryOpen = true)} />
      </BatchBar>
      <div class="app-table-wrap">
        <table class="app-table" aria-label="任务队列">
          <thead>
            <tr>
              <th style="width:40px;text-align:center;">
                <input
                  type="checkbox"
                  aria-label="全选任务"
                  checked={allTasksSelected}
                  onchange={toggleSelectAllTasks}
                />
              </th>
              <th>任务号</th>
              <th>任务</th>
              <th>来源</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            {#each taskList as t (t.id)}
              <tr>
                <td style="text-align:center;">
                  <input
                    type="checkbox"
                    aria-label="选中任务 {t.id}"
                    checked={selectedTasks.has(t.id)}
                    onchange={() => toggleSelectTask(t.id)}
                  />
                </td>
                <td><code style="padding:2px 6px;background:var(--color-bg-subtle);border-radius:3px;">{t.id}</code></td>
                <td>{t.task}</td>
                <td>
                  <span>{t.source}</span>
                  {#if t.errorCode}<span class="text-secondary" style="font-size:11px;display:block;">{t.errorCode}</span>{/if}
                </td>
                <td style="white-space:nowrap;">
                  <!-- 每行一个「⋮」动作菜单（约定 D）：重试/取消打开对应单动作确认 Dialog -->
                  <RowActionsMenu
                    label="更多操作：任务 {t.id}"
                    actions={taskRowActions(t)}
                  />
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
      <footer class="app-card__foot" style="margin-top:14px;">
        <button type="button" class="text-link" style="font-size:12px;background:none;border:none;cursor:pointer;" disabled title="任务清理 action 尚未接入">
          清理已完成
        </button>
      </footer>
    </div>
  </section>
{/if}

<!-- Provider 表单字段（snippet）：「添加渠道」Dialog 与行操作弹层「编辑渠道」分节共用，
     字段状态绑定同一组 provider* 变量（两弹层互斥打开）。 -->
{#snippet providerFields()}
  <label class="app-form-field">
    <span class="app-field-label" style="font-size:12px;font-weight:600;display:block;margin-bottom:4px;">渠道名称 *</span>
    <input
      type="text"
      name="provider_name"
      class="input-field"
      style="width:100%;box-sizing:border-box;"
      bind:value={providerName}
      placeholder="如：OpenAI、DeepSeek、本地 Ollama"
      required
    />
  </label>

  <label class="app-form-field">
    <span class="app-field-label" style="font-size:12px;font-weight:600;display:block;margin-bottom:4px;">接口类型 *</span>
    <select
      name="provider_adapter_type"
      class="app-select"
      style="width:100%;box-sizing:border-box;"
      bind:value={providerAdapterType}
    >
      <option value="openai_compatible">OpenAI 兼容 (OpenAI / DeepSeek / Ollama / FastChat)</option>
      <option value="anthropic">Anthropic (Claude API)</option>
      <option value="custom">自定义适配器 / 内部网关</option>
    </select>
  </label>

  <label class="app-form-field">
    <span class="app-field-label" style="font-size:12px;font-weight:600;display:block;margin-bottom:4px;">Base URL *</span>
    <input
      type="text"
      name="provider_base_url"
      class="input-field"
      style="width:100%;box-sizing:border-box;"
      bind:value={providerBaseUrl}
      placeholder="https://api.openai.com/v1"
      required
    />
  </label>

  <label class="app-form-field">
    <span class="app-field-label" style="font-size:12px;font-weight:600;display:block;margin-bottom:4px;">默认模型</span>
    <input
      type="text"
      name="provider_default_model"
      class="input-field"
      style="width:100%;box-sizing:border-box;"
      bind:value={providerDefaultModel}
      placeholder="如：gpt-4o-mini、deepseek-chat、qwen2.5:7b"
    />
  </label>

  <label class="app-form-field">
    <span class="app-field-label" style="font-size:12px;font-weight:600;display:block;margin-bottom:4px;">
      API Key / 访问凭据
      {#if providerSecretConfigured}
        <span class="badge badge-success" style="font-size:10px;margin-left:6px;">已有密钥</span>
      {/if}
    </span>
    <input
      type="password"
      name="provider_api_key"
      class="input-field"
      style="width:100%;box-sizing:border-box;"
      bind:value={providerApiKey}
      placeholder={providerSecretConfigured ? '留空表示保持当前密钥不变' : 'sk-...（不回显明文）'}
    />
    <span class="app-field-help" style="display:block;margin-top:2px;font-size:11px;color:var(--color-text-secondary);">
      密钥仅由服务端安全隔离存储与调用，前端页面不回显明文或片段。
    </span>
  </label>

  <label class="app-form-field">
    <span class="app-field-label" style="font-size:12px;font-weight:600;display:block;margin-bottom:4px;">渠道状态</span>
    <select
      name="provider_status"
      class="app-select"
      style="width:100%;box-sizing:border-box;"
      bind:value={providerStatus}
    >
      <option value="enabled">启用 (enabled)</option>
      <option value="disabled">停用 (disabled)</option>
    </select>
  </label>

  <label class="app-form-field">
    <span class="app-field-label" style="font-size:12px;font-weight:600;display:block;margin-bottom:4px;">操作原因 *（审计日志）</span>
    <input
      type="text"
      name="reason"
      class="input-field"
      style="width:100%;box-sizing:border-box;"
      bind:value={providerReason}
      placeholder="必填（写审计）"
      required
    />
  </label>
{/snippet}

<!-- 添加渠道：Dialog 内表单（页头入口，?/saveProvider，expected_version + If-Match 由服务端带）。
     行内编辑入口收进下方「渠道操作」弹层的编辑分节。 -->
<Dialog
  open={providerDialogOpen}
  title="添加模型渠道"
  description="渠道名称与 Base URL 必填；API Key 仅写入受保护 Secret Store（不回显）。"
  onclose={closeProviderDialog}
>
  <form
    method="POST"
    action="?/saveProvider"
    use:enhance={() => {
      return async ({ result, update }) => {
        toastActionResult(result);
        await update({ reset: false });
        if (result.type === 'success') closeProviderDialog();
      };
    }}
    style="display:flex;flex-direction:column;gap:12px;"
  >
    <input type="hidden" name="expected_version" value={config?.version ?? 1} />
    {@render providerFields()}

    <footer style="display:flex;justify-content:flex-end;gap:8px;margin-top:8px;padding-top:10px;border-top:1px solid var(--color-border);">
      <button type="button" class="btn ghost sm" onclick={closeProviderDialog}>取消</button>
      <button type="submit" class="btn primary sm">确认添加</button>
    </footer>
  </form>
</Dialog>

<!-- Provider 行操作 Dialog（约定 D：动作由「⋮」菜单选定，单动作确认 + reason 必填）：
     按 providerOpsAction 渲染对应表单节，各节独立提交既有契约；成功 toastActionResult →
     update → 关弹层清 target/草稿（测试分节结果就地展示，不自动关闭）。 -->
<Dialog
  open={providerOpsTarget !== null}
  title={providerOpsActionMeta?.title ?? '渠道操作'}
  description={providerOpsActionMeta?.description ?? ''}
  onclose={closeProviderOps}
>
  {#if providerOpsAction === 'edit'}
    <!-- 动作节：编辑渠道（?/saveProvider，expected_version + provider_id 隐藏字段） -->
    <form
      method="POST"
      action="?/saveProvider"
      use:enhance={() => {
        return async ({ result, update }) => {
          toastActionResult(result);
          await update({ reset: false });
          if (result.type === 'success') closeProviderOps();
        };
      }}
      style="display:flex;flex-direction:column;gap:12px;"
    >
      <input type="hidden" name="expected_version" value={config?.version ?? 1} />
      <input type="hidden" name="provider_id" value={providerOpsTarget?.id ?? ''} />
      {@render providerFields()}
      <footer style="display:flex;justify-content:flex-end;gap:8px;">
        <button type="button" class="btn ghost sm" onclick={closeProviderOps}>取消</button>
        <button type="submit" class="btn primary sm">保存渠道修改</button>
      </footer>
    </form>
  {:else if providerOpsAction === 'test'}
    <!-- 动作节：测试连接（?/test；结果就地展示，成功后保留弹层便于查看） -->
    <form
      method="POST"
      action="?/test"
      use:enhance={() => {
        return async ({ result, update }) => {
          const isResult = result.type === 'success' || result.type === 'failure';
          const d = (isResult ? ((result as { data?: unknown }).data ?? null) : null) as {
            testResult?: { ok: boolean; message: string } | null;
            message?: string;
          } | null;
          if (result.type === 'success' && d?.testResult) {
            providerOpsTestOutput = { ok: d.testResult.ok, message: d.testResult.message };
            showToast(d.testResult.message, d.testResult.ok ? 'success' : 'danger');
          } else {
            providerOpsTestOutput = { ok: false, message: d?.message ?? '测试连接失败' };
            toastActionResult(result);
          }
          await update({ reset: false });
        };
      }}
      style="display:flex;flex-direction:column;gap:10px;"
    >
      <label class="app-form-field">
        <span class="app-field-label" style="font-size:12px;font-weight:600;display:block;margin-bottom:4px;">Base URL</span>
        <input
          type="text"
          name="base_url"
          class="input-field"
          style="width:100%;box-sizing:border-box;"
          bind:value={providerOpsTestBaseUrl}
          required
        />
      </label>
      {#if providerOpsTestOutput}
        <span style="font-size:12px;color:{providerOpsTestOutput.ok ? 'var(--color-success, #38a169)' : 'var(--color-danger, #e53e3e)'};">
          {providerOpsTestOutput.message}
        </span>
      {/if}
      <div style="display:flex;justify-content:flex-end;">
        <Button text="运行探针" variant="secondary" size="sm" type="submit" />
      </div>
    </form>
  {:else if providerOpsAction === 'deleteProvider'}
    <!-- 动作节：删除渠道（?/deleteProvider，reason 必填；危险动作 → danger 提交按钮） -->
    <form
      method="POST"
      action="?/deleteProvider"
      use:enhance={() => {
        return async ({ result, update }) => {
          toastActionResult(result);
          await update({ reset: false });
          if (result.type === 'success') closeProviderOps();
        };
      }}
      style="display:flex;flex-direction:column;gap:10px;"
    >
      <input type="hidden" name="provider_id" value={providerOpsTarget?.id ?? ''} />
      <input type="hidden" name="expected_version" value={config?.version ?? 1} />
      <label class="app-form-field">
        <span class="app-field-label" style="font-size:12px;font-weight:600;display:block;margin-bottom:4px;">删除原因 *（写审计）</span>
        <input
          type="text"
          name="reason"
          class="input-field"
          style="width:100%;box-sizing:border-box;"
          bind:value={providerOpsDeleteReason}
          placeholder="必填（写审计）"
          required
        />
      </label>
      <p class="text-secondary" style="font-size:11px;margin:0;">删除后该渠道的任务将失去可用供应商，不可恢复。</p>
      <div style="display:flex;justify-content:flex-end;">
        <Button text="确认删除渠道" variant="danger" size="sm" type="submit" />
      </div>
    </form>
  {/if}
</Dialog>

<!-- 全站策略与预算：Dialog 内表单（?/save，If-Match 版本 + reason 必填）。 -->
<Dialog
  open={strategyOpen}
  title="编辑全站 AI 策略与预算"
  description="全站 AI 能力开关、数据脱敏级别、功能 Flag 与每日 Token 限制；保存写入审计。"
  onclose={() => (strategyOpen = false)}
>
  <form
    method="POST"
    action="?/save"
    use:enhance={() => {
      return async ({ result, update }) => {
        toastActionResult(result);
        await update({ reset: false });
        if (result.type === 'success') strategyOpen = false;
      };
    }}
    style="display:flex;flex-direction:column;gap:14px;"
  >
    <input type="hidden" name="expected_version" value={config?.version ?? 4} />

    <div style="display:grid;grid-template-columns:repeat(auto-fit, minmax(240px, 1fr));gap:14px;">
      <label class="app-form-field">
        <span class="app-field-label" style="font-size:12px;font-weight:600;display:block;margin-bottom:4px;">全站 AI 能力开关</span>
        <div style="display:flex;align-items:center;gap:8px;padding-top:4px;">
          <input type="checkbox" name="enabled" checked={config?.enabled ?? false} />
          <span style="font-size:13px;">启用站点级 AI 特性</span>
        </div>
      </label>

      <label class="app-form-field">
        <span class="app-field-label" style="font-size:12px;font-weight:600;display:block;margin-bottom:4px;">数据发送策略</span>
        <select name="data_mode" class="app-select" style="width:100%;">
          <option value="disabled" selected={config?.data_mode === 'disabled'}>disabled（不发送）</option>
          <option value="metadata_only" selected={config?.data_mode === 'metadata_only'}>metadata_only（仅元数据）</option>
          <option value="redacted" selected={config?.data_mode === 'redacted'}>redacted（脱敏）</option>
          <option value="full_with_consent" selected={config?.data_mode === 'full_with_consent'}>full_with_consent（逐次同意）</option>
        </select>
      </label>
    </div>

    <div>
      <span class="app-field-label" style="font-size:12px;font-weight:600;display:block;margin-bottom:6px;">启用功能特性</span>
      <div style="display:flex;flex-wrap:wrap;gap:16px;">
        <label style="display:inline-flex;align-items:center;gap:6px;font-size:13px;">
          <input type="checkbox" name="flag_formatting" checked={config?.flags?.formatting !== false} />
          格式化排版
        </label>
        <label style="display:inline-flex;align-items:center;gap:6px;font-size:13px;">
          <input type="checkbox" name="flag_seo" checked={config?.flags?.seo !== false} />
          SEO 建议
        </label>
        <label style="display:inline-flex;align-items:center;gap:6px;font-size:13px;">
          <input type="checkbox" name="flag_tagging" checked={config?.flags?.tagging !== false} />
          标签抽取
        </label>
        <label style="display:inline-flex;align-items:center;gap:6px;font-size:13px;">
          <input type="checkbox" name="flag_moderation" checked={config?.flags?.moderation !== false} />
          内容审核辅助
        </label>
      </div>
    </div>

    <div style="display:grid;grid-template-columns:repeat(auto-fit, minmax(240px, 1fr));gap:14px;">
      <label class="app-form-field">
        <span class="app-field-label" style="font-size:12px;font-weight:600;display:block;margin-bottom:4px;">每用户每日 token 预算</span>
        <input
          type="number"
          name="budget_per_user_daily_tokens"
          class="input-field"
          style="width:100%;box-sizing:border-box;"
          min="0"
          value={config?.budgets?.per_user_daily_tokens ?? ''}
          placeholder="留空表示不限制"
        />
      </label>

      <label class="app-form-field">
        <span class="app-field-label" style="font-size:12px;font-weight:600;display:block;margin-bottom:4px;">全站每日 token 预算</span>
        <input
          type="number"
          name="budget_site_daily_tokens"
          class="input-field"
          style="width:100%;box-sizing:border-box;"
          min="0"
          value={config?.budgets?.site_daily_tokens ?? ''}
          placeholder="留空表示不限制"
        />
      </label>
    </div>

    <label class="app-form-field">
      <span class="app-field-label" style="font-size:12px;font-weight:600;display:block;margin-bottom:4px;">操作原因 *（写入审计日志）</span>
      <input
        type="text"
        name="reason"
        class="input-field"
        style="width:100%;box-sizing:border-box;"
        placeholder="必填（写审计）"
        value="AI配置更新"
        required
      />
    </label>

    <div style="display:flex;justify-content:flex-end;gap:8px;">
      <button type="button" class="btn ghost sm" onclick={() => (strategyOpen = false)}>取消</button>
      <button type="submit" class="btn primary sm">保存全站策略</button>
    </div>
  </form>
</Dialog>

<!-- 任务行操作 Dialog（约定 D：动作由「⋮」菜单选定，单动作确认 + reason 必填）：
     按 taskOpsAction 渲染对应表单节，各节独立提交既有契约；成功 toastActionResult →
     update → 关弹层清 target/草稿。 -->
<Dialog
  open={taskOpsTarget !== null}
  title={taskOpsActionMeta?.title ?? '任务操作'}
  description={taskOpsActionMeta?.description ?? ''}
  onclose={closeTaskOps}
>
  {#if taskOpsAction === 'retry'}
    <!-- 动作节：重试（?/retry，task_id 隐藏域 + reason 必填） -->
    <form
      method="POST"
      action="?/retry"
      use:enhance={() => {
        return async ({ result, update }) => {
          toastActionResult(result);
          await update({ reset: false });
          if (result.type === 'success') closeTaskOps();
        };
      }}
      style="display:flex;flex-direction:column;gap:10px;"
    >
      <input type="hidden" name="task_id" value={taskOpsTarget?.id ?? ''} />
      <label class="app-form-field">
        <span class="app-field-label" style="font-size:12px;font-weight:600;display:block;margin-bottom:4px;">操作原因 *（写审计）</span>
        <input type="text" name="reason" class="input-field" style="width:100%;box-sizing:border-box;" bind:value={taskOpsRetryReason} placeholder="必填（写审计）" required />
      </label>
      <div style="display:flex;justify-content:flex-end;">
        <Button text="确认重试" variant="primary" size="sm" type="submit" />
      </div>
    </form>
  {:else if taskOpsAction === 'cancel'}
    <!-- 动作节：取消（?/cancel，task_id + reason 必填；危险动作 → danger 提交按钮） -->
    <form
      method="POST"
      action="?/cancel"
      use:enhance={() => {
        return async ({ result, update }) => {
          toastActionResult(result);
          await update({ reset: false });
          if (result.type === 'success') closeTaskOps();
        };
      }}
      style="display:flex;flex-direction:column;gap:10px;"
    >
      <input type="hidden" name="task_id" value={taskOpsTarget?.id ?? ''} />
      <label class="app-form-field">
        <span class="app-field-label" style="font-size:12px;font-weight:600;display:block;margin-bottom:4px;">取消原因 *（写审计）</span>
        <input type="text" name="reason" class="input-field" style="width:100%;box-sizing:border-box;" bind:value={taskOpsCancelReason} placeholder="必填（写审计）" required />
      </label>
      <p class="text-secondary" style="font-size:11px;margin:0;">取消后任务不再执行，不可恢复。</p>
      <div style="display:flex;justify-content:flex-end;">
        <Button text="确认取消任务" variant="danger" size="sm" type="submit" />
      </div>
    </form>
  {/if}
</Dialog>

<!-- 批量重试：Dialog 内批量隐藏表单（ids；服务端循环单条端点）。 -->
<Dialog
  open={batchRetryOpen}
  title="批量重试任务"
  description={`将重新入队 ${selectedTasks.size} 个任务；原因写审计。`}
  onclose={() => (batchRetryOpen = false)}
>
  <form
    method="POST"
    action="?/batchRetry"
    use:enhance={() => {
      return async ({ result, update }) => {
        toastActionResult(result);
        await update({ reset: false });
        if (result.type === 'success') {
          selectedTasks = new Set();
          batchRetryOpen = false;
        }
      };
    }}
    style="display:flex;flex-direction:column;gap:12px;"
  >
    <input type="hidden" name="ids" value={[...selectedTasks].join(',')} />
    <label class="app-form-field">
      <span class="app-field-label" style="font-size:12px;font-weight:600;display:block;margin-bottom:4px;">操作原因 *（写审计）</span>
      <input type="text" name="reason" class="input-field" placeholder="必填（写审计）" required />
    </label>
    <div style="display:flex;justify-content:flex-end;gap:8px;">
      <button type="button" class="btn ghost sm" onclick={() => (batchRetryOpen = false)}>取消</button>
      <Button text="确认批量重试" variant="primary" size="sm" type="submit" />
    </div>
  </form>
</Dialog>

<style>
  .admin-ai-actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    margin-bottom: var(--space-4);
  }

  .admin-ai-actions p {
    margin: 0;
    color: var(--color-text-secondary);
    font-size: var(--text-sm);
  }

  @media (max-width: 560px) {
    .admin-ai-actions {
      align-items: stretch;
      flex-direction: column;
    }
  }
</style>
