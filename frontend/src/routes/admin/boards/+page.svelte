<script lang="ts">
  // M03-UI-07：管理板块页——后端裁决状态渲染 + 新建板块表单。
  // 原型对齐：prototype/pages/admin-boards.html
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import ExportButton from '$lib/components/admin/ExportButton.svelte';
  import { adminStateLabel } from '$lib/admin';
  import type { AdminBoardsPageData } from './+page.server';

  let { data, form }: { data: AdminBoardsPageData; form?: AdminBoardsPageData } = $props();

  const loadState = $derived(form?.loadState ?? data.loadState);
  const created = $derived(form?.created === true);
  const message = $derived(
    form?.message ? (form.requestId ? `${form.message}（请求号 ${form.requestId}）` : form.message) : null
  );
  let dismissError = $state(false);

  let showCreate = $state(false);
  /** 当前行内编辑表单（每次一行）。 */
  let editingId = $state<string | null>(null);

  // M18：工具条搜索与复选框（对齐原型表格通用模式）
  let q = $state('');
  let statusFilter = $state('');
  let selectedIds = $state<string[]>([]);
  const displayedItems = $derived.by(() => {
    let list = loadState.state === 'ok' ? loadState.items : [];
    if (q.trim()) {
      const lower = q.trim().toLowerCase();
      list = list.filter(
        (i) => i.name.toLowerCase().includes(lower) || (i.description ?? '').toLowerCase().includes(lower)
      );
    }
    if (statusFilter === 'active') list = list.filter((i) => i.is_active);
    if (statusFilter === 'inactive') list = list.filter((i) => !i.is_active);
    return list;
  });
  let allSelected = $derived(
    displayedItems.length > 0 && selectedIds.length === displayedItems.length
  );
  function toggleAll() {
    if (allSelected) selectedIds = [];
    else selectedIds = displayedItems.map((i) => i.id);
  }
  function toggleRow(id: string) {
    if (selectedIds.includes(id)) selectedIds = selectedIds.filter((x) => x !== id);
    else selectedIds = [...selectedIds, id];
  }

  /** 可见性/发帖策略 → 产品文案与徽章（值域见 0003/0022 CHECK）。 */
  function visibilityBadge(v: string | null | undefined): { cls: string; label: string } {
    switch (v) {
      case 'public':
        return { cls: 'sb-success', label: '公开' };
      case 'members':
        return { cls: 'sb-brand', label: '登录成员' };
      case 'restricted':
        return { cls: 'sb-hot', label: '需加入' };
      case 'hidden':
        return { cls: 'sb-gray', label: '隐藏' };
      default:
        return { cls: 'sb-gray', label: v || '—' };
    }
  }
  function postingModeLabel(m: string | null | undefined): string {
    switch (m) {
      case 'normal':
        return '正常发帖';
      case 'approval':
        return '先审后发';
      case 'readonly':
        return '只读';
      case 'closed':
        return '关闭';
      default:
        return m || '—';
    }
  }

</script>

<svelte:head>
  <title>板块管理 — BBLBB Admin</title>
</svelte:head>

<PageHeader title="板块管理" />

<section class="app-card">
  <header class="app-card__head">
    <h2>板块列表</h2>
  </header>

  <div class="app-card__body">
    {#if message && !dismissError}
      <div
        class="alert {created ? 'alert-success' : 'alert-danger'}"
        role="status"
        style="margin-bottom:14px;padding:10px 14px;background:var(--color-bg-subtle);border-radius:var(--radius-sm);font-size:13px;display:flex;justify-content:space-between;align-items:center;"
      >
        <span>{message}</span>
        <button type="button" class="btn ghost sm" onclick={() => (dismissError = true)}>关闭</button>
      </div>
    {/if}

    {#if loadState.state === 'forbidden'}
      <p class="input-hint is-error" role="alert"><Icon name="lock" size={14} /> {adminStateLabel('forbidden')}</p>
    {:else if loadState.state === 'not_implemented'}
      <p class="input-hint" role="note">板块列表接口开发中（M13-ADMIN）。创建表单已可用。</p>
    {:else if loadState.state === 'error'}
      <p class="input-hint is-error" role="alert">{loadState.message || adminStateLabel('error')}</p>
    {:else if loadState.state === 'ok' && loadState.items.length === 0}
      <p class="input-hint">暂无板块数据。</p>
    {:else if loadState.state === 'ok'}
      <!-- M18：原型对齐工具栏（搜索当前列表 + 全部状态 + 清除） -->
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
            <option value="active">已启用</option>
            <option value="inactive">已停用</option>
          </select>
          {#if q || statusFilter}
            <button
              type="button"
              class="btn ghost sm"
              onclick={() => { q = ''; statusFilter = ''; }}
            >清除</button>
          {/if}
        </div>
      </div>

      {#if selectedIds.length > 0}
        <div class="app-notice" style="display:flex;align-items:center;justify-content:space-between;padding:8px 12px;margin-bottom:10px;background:var(--color-bg-subtle);border-radius:var(--radius-sm);">
          <span style="font-size:var(--text-xs);font-weight:600;">{selectedIds.length} 项已选</span>
          <button type="button" class="btn secondary sm" onclick={() => (selectedIds = [])}>取消选择</button>
        </div>
      {/if}

      {#if displayedItems.length === 0}
        <p class="input-hint">没有匹配的板块。</p>
      {:else}
        <div class="app-table-wrap">
          <table class="app-table" aria-label="板块列表">
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
                <th style="min-width:200px;">板块 / 描述</th>
                <th>路径</th>
                <th>主题数</th>
                <th>可见性</th>
                <th>发帖策略</th>
                <th>版主</th>
                <th>状态</th>
                <th style="min-width:150px;">操作</th>
              </tr>
            </thead>
            <tbody>
              {#each displayedItems as item (item.id)}
                <tr>
                  <td style="text-align:center;">
                    <input
                      type="checkbox"
                      checked={selectedIds.includes(item.id)}
                      onchange={() => toggleRow(item.id)}
                      aria-label="选择此项"
                    />
                  </td>
                <td>
                  <b>{item.name}</b>
                  {#if item.description}
                    <span class="sub" style="display:block;margin-top:2px;">{item.description}</span>
                  {/if}
                </td>
                <td>
                  <span class="text-secondary" style="font-family:var(--font-family-mono);font-size:12px;">/{item.slug}</span>
                </td>
                <td><a class="text-link" href="/b/{item.slug}" style="font-size:12px;">{item.post_count ?? 0}</a></td>
                <td><span class="sbadge {visibilityBadge(item.visibility).cls}">{visibilityBadge(item.visibility).label}</span></td>
                <td><span class="text-secondary" style="font-size:12px;">{postingModeLabel(item.posting_mode)}</span></td>
                <td>
                  <span class="text-secondary" style="font-size:12px;">
                    {(item.moderators ?? []).length ? (item.moderators ?? []).join('、') : '—'}
                  </span>
                </td>
                <td><span class="sbadge {item.is_active ? 'sb-success' : 'sb-gray'}">{item.is_active ? '启用' : '停用'}</span></td>
                <td class="adm-acts">
                  <a class="btn ghost sm" href="/boards/{item.slug}">查看</a>
                  <button type="button" class="btn secondary sm" onclick={() => (editingId = editingId === item.id ? null : item.id)}>
                    编辑
                  </button>
                  <form method="POST" action="?/update" use:enhance style="display:inline-flex;margin:0;" onsubmit={() => (editingId = null)}>
                    <input type="hidden" name="id" value={item.id} />
                    <input type="hidden" name="version" value={item.version} />
                    <input type="hidden" name="sort_order" value="0" />
                    <input type="hidden" name="name" value={item.name} />
                    <input
                      type="text"
                      name="reason"
                      placeholder="原因（审计）"
                      required
                      aria-label={`置顶 ${item.name} 的原因`}
                      style="width:110px;height:30px;"
                    />
                    <button type="submit" class="btn ghost sm">置顶</button>
                  </form>
                </td>
              </tr>
              {#if editingId === item.id}
                <tr>
                  <td colspan="8" style="background:var(--color-bg-subtle);">
                    <form method="POST" action="?/update" use:enhance style="display:flex;flex-wrap:wrap;gap:12px;align-items:flex-end;padding:10px 4px;">
                      <input type="hidden" name="id" value={item.id} />
                      <input type="hidden" name="version" value={item.version} />
                      <div>
                        <label class="input-label" for={`eb-name-${item.id}`}>名称</label>
                        <input type="text" class="input-field" id={`eb-name-${item.id}`} name="name" value={item.name} maxlength="100" required style="width:180px;" />
                      </div>
                      <div>
                        <label class="input-label" for={`eb-vis-${item.id}`}>可见性</label>
                        <select class="input-field" id={`eb-vis-${item.id}`} name="visibility" style="width:150px;">
                          <option value="public" selected={item.visibility === 'public'}>公开</option>
                          <option value="members" selected={item.visibility === 'members'}>登录成员</option>
                          <option value="restricted" selected={item.visibility === 'restricted'}>需加入</option>
                          <option value="hidden" selected={item.visibility === 'hidden'}>管理可见</option>
                        </select>
                      </div>
                      <div>
                        <label class="input-label" for={`eb-mode-${item.id}`}>发帖策略</label>
                        <select class="input-field" id={`eb-mode-${item.id}`} name="posting_mode" style="width:150px;">
                          <option value="normal" selected={item.posting_mode === 'normal'}>正常发帖</option>
                          <option value="approval" selected={item.posting_mode === 'approval'}>先审后发</option>
                          <option value="readonly" selected={item.posting_mode === 'readonly'}>只读</option>
                          <option value="closed" selected={item.posting_mode === 'closed'}>关闭</option>
                        </select>
                      </div>
                      <div>
                        <label class="input-label" for={`eb-sort-${item.id}`}>排序（0 = 置顶）</label>
                        <input type="number" class="input-field" id={`eb-sort-${item.id}`} name="sort_order" value={item.sort_order} min="0" step="1" style="width:120px;" />
                      </div>
                      <div>
                        <label class="input-label" for={`eb-active-${item.id}`}>状态</label>
                        <select class="input-field" id={`eb-active-${item.id}`} name="is_active" style="width:110px;">
                          <option value="true" selected={!!item.is_active}>启用</option>
                          <option value="false" selected={item.is_active === 0}>停用</option>
                        </select>
                      </div>
                      <div>
                        <label class="input-label" for={`eb-reason-${item.id}`}>操作原因（审计）</label>
                        <input type="text" class="input-field" id={`eb-reason-${item.id}`} name="reason" placeholder="如：调整板块可见性" required style="width:200px;" />
                      </div>
                      <div style="display:flex;gap:8px;">
                        <button type="submit" class="btn primary sm">保存</button>
                        <button type="button" class="btn ghost sm" onclick={() => (editingId = null)}>取消</button>
                      </div>
                    </form>
                  </td>
                </tr>
              {/if}
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
    {/if}

    {#if created}
      <p class="input-hint" role="status" style="margin-top:14px;">板块已创建。</p>
    {/if}
    {#if message}
      <p class="input-hint is-error" role="alert" style="margin-top:14px;">{message}</p>
    {/if}
  </div>

  <footer class="app-card__foot">
    <button type="button" class="btn primary sm" onclick={() => (showCreate = !showCreate)}>
      {showCreate ? '收起表单' : '新建板块'}
    </button>
    <ExportButton
      label="导出板块"
      filename="admin-boards"
      columns={[
        { key: 'name', label: '名称' },
        { key: 'slug', label: '路径' },
        { key: 'posts', label: '主题数' },
        { key: 'visibility', label: '可见性' },
        { key: 'mode', label: '发帖策略' },
        { key: 'mods', label: '版主' },
        { key: 'active', label: '状态' }
      ]}
      getData={() =>
        (loadState.state === 'ok' ? loadState.items : []).map((item) => ({
          name: item.name,
          slug: `/${item.slug}`,
          posts: item.post_count ?? 0,
          visibility: visibilityBadge(item.visibility).label,
          mode: postingModeLabel(item.posting_mode),
          mods: (item.moderators ?? []).join('|'),
          active: item.is_active ? '启用' : '停用'
        }))}
    />
  </footer>
</section>

<!-- 新建板块表单（受控展开或默认呈现） -->
<section class="app-card" style="margin-top:16px;{showCreate || loadState.state === 'not_implemented' ? '' : 'display:none;'}">
  <header class="app-card__head">
    <h2>新建板块</h2>
  </header>

  <div class="app-card__body">
    <form method="POST" action="?/create" use:enhance style="display:flex;flex-direction:column;gap:14px;max-width:520px;">
      <div>
        <label class="input-label" for="admin-board-name">名称</label>
        <input type="text" class="input-field" id="admin-board-name" name="name" maxlength="100" required />
      </div>

      <div>
        <label class="input-label" for="admin-board-slug">slug</label>
        <input type="text" class="input-field" id="admin-board-slug" name="slug" maxlength="120" pattern="[a-z0-9-]+" required />
        <p class="input-hint" style="margin:4px 0 0;">小写字母/数字/连字符，唯一。</p>
      </div>

      <div>
        <label class="input-label" for="admin-board-desc">说明</label>
        <textarea class="input-field" id="admin-board-desc" name="description" rows="3" maxlength="2000"></textarea>
      </div>

      <div>
        <label class="input-label" for="admin-board-visibility">可见性</label>
        <select class="input-field" id="admin-board-visibility" name="visibility">
          <option value="public">public（公开）</option>
          <option value="members">members（登录成员）</option>
          <option value="restricted">restricted（需加入）</option>
          <option value="hidden">hidden（管理可见）</option>
        </select>
      </div>

      <div>
        <label class="input-label" for="admin-board-reason">创建原因（审计）</label>
        <input type="text" class="input-field" id="admin-board-reason" name="reason" placeholder="如：新增技术专区" required />
      </div>

      <div style="margin-top:8px;">
        <button type="submit" class="btn primary sm">提交创建</button>
      </div>
    </form>
  </div>
</section>
