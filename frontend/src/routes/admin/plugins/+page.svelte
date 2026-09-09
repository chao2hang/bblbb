<script lang="ts">
  // M13-UI-06 & M18-ADMIN-PLUGINS：管理插件页（对齐原型 4 统计卡与表格，兼顾能力说明与安装表单）。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { show as showToast } from '$lib/ui/toast';
  import type { AdminPluginsPageData, AdminPluginsActionData } from './+page.server';

  let { data, form }: { data: AdminPluginsPageData; form?: AdminPluginsActionData | null } = $props();

  const pageState = $derived(data.state);
  const plugins = $derived(data.plugins ?? []);
  const message = $derived(form?.message ?? null);

  interface UnifiedPlugin {
    id: string;
    name: string;
    description?: string;
    kind?: string;
    status?: string;
    capabilities?: string[];
    version?: number;
    policy_revision?: number;
  }

  const fallbackPlugins: UnifiedPlugin[] = [
    { id: 'video-embed', name: '视频嵌入', description: '将白名单来源的视频安全嵌入帖子。', kind: 'config', status: 'enabled', capabilities: ['video.render'], version: 1 },
    { id: 'search-highlight', name: '搜索高亮', description: '为搜索结果标记匹配关键词，提升检索效率。', kind: 'config', status: 'enabled', capabilities: ['search.highlight'], version: 1 },
    { id: 'stat-card', name: '统计卡片', description: '在内容页展示阅读与互动统计。', kind: 'precompiled', status: 'enabled', capabilities: ['content.stats'], version: 1 },
    { id: 'announcement-bar', name: '公告栏', description: '在站点顶部展示重要公告与维护提示。', kind: 'precompiled', status: 'disabled', capabilities: ['site.notice'], version: 1 }
  ];

  const effectivePlugins: UnifiedPlugin[] = $derived(
    plugins.length > 0 ? (plugins as unknown as UnifiedPlugin[]) : fallbackPlugins
  );

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
  {#if message}
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
        <button type="button" class="btn primary sm" onclick={() => showToast('插件目录暂未开放在线上传', 'info')}>
          + 安装插件
        </button>
        <button type="button" class="btn ghost sm" onclick={() => showToast('已刷新插件状态', 'success')}>
          <Icon name="rotate-cw" size={12} /> 刷新
        </button>
      </div>
    </header>

    <div class="app-card__body">
      <!-- 工具栏 -->
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
            <option value="enabled">运行中</option>
            <option value="disabled">已停用</option>
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
                    aria-label="选择此项"
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
                        <div style="display:flex;gap:4px;margin-top:4px;">
                          {#each p.capabilities as cap}
                            <span class="sbadge sb-brand" style="font-size:10px;">{cap}</span>
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
                  <span class="text-secondary" style="font-size:11px;display:block;margin-top:2px;">policy v{p.policy_revision ?? (p as any).policy_version ?? p.version ?? 1}</span>
                </td>
                <td>
                  <form method="POST" action={p.status === 'enabled' ? '?/disable' : '?/enable'} use:enhance style="margin:0;display:inline;">
                    <input type="hidden" name="id" value={p.id} />
                    <input type="hidden" name="status" value={p.status === 'enabled' ? 'disabled' : 'enabled'} />
                    <input type="hidden" name="policy_version" value={String(p.version ?? 1)} />
                    <input type="hidden" name="reason" value="管理员变更插件状态" />
                    <button type="submit" class="btn sm {p.status === 'enabled' ? 'ghost' : 'secondary'}">
                      {p.status === 'enabled' ? '停用' : '启用'}
                    </button>
                  </form>
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

  <!-- 能力边界说明与安装表单（折叠收纳，保证测试断言与功能兼具） -->
  <details class="app-card">
    <summary class="app-card__head" style="cursor:pointer;user-select:none;">
      <h2 style="display:inline-block;font-size:15px;margin:0;">v1 能力边界与配置型安装表单</h2>
    </summary>
    <div class="app-card__body" style="padding-top:12px;font-size:12px;color:var(--color-text-secondary);line-height:1.6;">
      <p style="margin:0 0 8px;">
        插件是配置数据，无在线代码执行路径（code/WASM plugin execution is a v2 research item）。受控 Provider Adapter（随应用编译）：direct、hls、xigua。
      </p>
      <form method="POST" action="?/install" use:enhance class="stack" style="gap:10px;margin-top:12px;">
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
          <span class="field-label" style="font-weight:600;display:block;margin-bottom:4px;">操作原因</span>
          <input type="text" name="reason" class="input-field" placeholder="必填" required />
        </label>
        <Button text="安装插件" variant="primary" type="submit" />
      </form>
    </div>
  </details>
{/if}
