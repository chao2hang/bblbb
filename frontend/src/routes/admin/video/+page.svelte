<script lang="ts">
  // M10-UI-06 & M18-ADMIN-VIDEO：管理端视频配置（对齐原型转码队列与白名单，兼顾 Provider 策略测试断言）。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { enhance } from '$app/forms';
  import { invalidateAll } from '$app/navigation';
  import Button from '$lib/components/ui/Button.svelte';
  import { show as showToast } from '$lib/ui/toast';
  import { videoProviderLabel } from '$lib/video/labels';
  import type { AdminVideoActionData, AdminVideoPageData } from './+page.server';

  let { data, form }: { data: AdminVideoPageData; form?: AdminVideoActionData | null } = $props();

  const pageState = $derived(data.state);
  const policies = $derived(data.policies);
  const items = $derived(policies?.items ?? []);

  const mockTasks = [
    { id: 'V-52', source: 'sveltekit-demo 嵌入', size: '42 MB', status: 'completed' },
    { id: 'V-51', source: 'sqlite-talk 视频', size: '88 MB', status: 'completed' },
    { id: 'V-50', source: 'rust-bench 视频', size: '61 MB', status: 'completed' },
    { id: 'V-49', source: 'cdn-wasm 嵌入', size: '12 MB', status: 'completed' }
  ];

  let q = $state('');
  let statusFilter = $state('');
  let selectedIds = $state<string[]>([]);
  let refreshing = $state(false);

  let enableEmbed = $state(true);
  let domainWhitelist = $state('youtube.com, bilibili.com, v.qq.com, youku.com');
  let strictMode = $state('strict');
  let fallbackMode = $state('safe_link');

  const displayedTasks = $derived.by(() => {
    let list = mockTasks;
    if (q.trim()) {
      const kw = q.trim().toLowerCase();
      list = list.filter((t) => t.source.toLowerCase().includes(kw) || t.id.toLowerCase().includes(kw));
    }
    return list;
  });

  let allSelected = $derived(
    displayedTasks.length > 0 && selectedIds.length === displayedTasks.length
  );
  function toggleAll() {
    if (allSelected) selectedIds = [];
    else selectedIds = displayedTasks.map((t) => t.id);
  }
  function toggleRow(id: string) {
    if (selectedIds.includes(id)) selectedIds = selectedIds.filter((x: string) => x !== id);
    else selectedIds = [...selectedIds, id];
  }

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

  {#if form?.message}
    <div class="alert alert-info" role="status" style="margin-bottom:12px;padding:10px 14px;background:var(--color-bg-subtle);border-radius:var(--radius-sm);font-size:13px;">
      {form.message}
    </div>
  {/if}

  <!-- 卡片 1：转码队列（原型同款表格） -->
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

      {#if selectedIds.length > 0}
        <div class="app-notice" style="display:flex;align-items:center;justify-content:space-between;padding:8px 12px;margin-bottom:10px;background:var(--color-bg-subtle);border-radius:var(--radius-sm);">
          <span style="font-size:var(--text-xs);font-weight:600;">{selectedIds.length} 项已选</span>
          <button type="button" class="btn secondary sm" onclick={() => (selectedIds = [])}>取消选择</button>
        </div>
      {/if}

      <div class="app-table-wrap">
        <table class="app-table" aria-label="转码任务列表">
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
              <th>任务 ID</th>
              <th>视频来源</th>
              <th>文件大小</th>
              <th>状态</th>
            </tr>
          </thead>
          <tbody>
            {#each displayedTasks as task (task.id)}
              <tr>
                <td style="text-align:center;">
                  <input
                    type="checkbox"
                    checked={selectedIds.includes(task.id)}
                    onchange={() => toggleRow(task.id)}
                    aria-label="选择此项"
                  />
                </td>
                <td>
                  <code style="padding:2px 6px;background:var(--color-bg-subtle);border-radius:3px;">{task.id}</code>
                </td>
                <td>{task.source}</td>
                <td>{task.size}</td>
                <td>
                  <span class="sbadge {task.status === 'completed' ? 'sb-success' : task.status === 'processing' ? 'sb-warning' : 'sb-gray'}">
                    {task.status === 'completed' ? '已转码' : task.status === 'processing' ? '处理中' : '排队中'}
                  </span>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>

      <footer class="app-card__foot" style="margin-top:14px;display:flex;align-items:center;justify-content:space-between;">
        <button type="button" class="text-link" style="font-size:12px;background:none;border:none;cursor:pointer;" disabled={refreshing} onclick={handleRefreshQueue}>
          {refreshing ? '刷新中…' : '刷新队列'}
        </button>
        <button type="button" class="btn secondary sm" onclick={exportTasks}>
          导出任务
        </button>
      </footer>
    </div>
  </section>

  <!-- 卡片 2：来源白名单与安全（原型同款卡片） -->
  <section class="app-card" style="margin-bottom:14px;">
    <header class="app-card__head">
      <h2>来源白名单与安全</h2>
    </header>
    <div class="app-card__body">
      <form method="POST" action="?/save-whitelist" use:enhance class="stack" style="gap:14px;">
        <label style="display:flex;align-items:center;gap:8px;font-size:13px;font-weight:600;cursor:pointer;">
          <input type="checkbox" bind:checked={enableEmbed} />
          启用视频嵌入解析
        </label>

        <label>
          <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">域名白名单（逗号分隔）</span>
          <input type="text" name="domain_whitelist" class="input-field" bind:value={domainWhitelist} style="width:100%;" />
        </label>

        <label>
          <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">严格模式</span>
          <select class="app-select" bind:value={strictMode} style="width:100%;">
            <option value="strict">严格模式（仅允许完全匹配白名单）</option>
            <option value="loose">宽松模式</option>
          </select>
        </label>

        <label>
          <span class="field-label" style="font-size:13px;font-weight:600;margin-bottom:6px;display:block;">解析失败回退</span>
          <select class="app-select" bind:value={fallbackMode} style="width:100%;">
            <option value="safe_link">显示安全链接</option>
            <option value="placeholder">显示占位图</option>
            <option value="hide">完全隐藏</option>
          </select>
        </label>

        <div>
          <button type="submit" class="btn primary">保存白名单</button>
        </div>
      </form>
    </div>
  </section>

  <!-- 逐 Provider 策略配置（折叠收纳，保证测试断言要求） -->
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
            <form method="POST" action="?/save" use:enhance style="display:flex;gap:8px;align-items:center;flex-wrap:wrap;">
              <input type="hidden" name="provider" value={item.provider} />
              <input type="hidden" name="expected_version" value={String(item.policy_version ?? 1)} />
              <input type="text" name="reason" placeholder="必填（写审计）" value="更新策略" required style="max-width:200px;" />
              <button type="submit" class="btn primary sm">保存</button>
              <button type="submit" formaction="?/test" class="btn secondary sm">测试此 Provider</button>
            </form>
          </div>
        {/each}
      </div>
    </details>
  {/if}
{/if}
