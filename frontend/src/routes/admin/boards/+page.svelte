<script lang="ts">
  // M03-UI-07：管理板块页——后端裁决状态渲染 + 板块 CRUD 行操作。
  // 原型对齐：prototype/pages/admin-boards.html
  // M18-ADMIN-DIALOG：写操作弹层化——
  // - 「新建板块」常驻表单卡 → 页脚按钮 + Dialog（?/create）；
  // - 行「编辑」展开行 → 按钮 + Dialog（?/update，If-Match version + reason）；
  // - 行「置顶」行内表单 → 按钮 + Dialog（?/update，sort_order=0 + reason）；
  // - 批量：选择列 + BatchBar + 批量启用/停用 Dialog → ?/batchUpdate
  //   （后端已有板块级单条写端点 PATCH /api/v1/admin/boards/{id}，见
  //   backend/src/routes/admin.rs update_admin_board：is_active + reason + If-Match）。
  // 页脚保留 ExportButton（客户端 CSV 导出）作为数据导出能力。
  // M18-ADMIN-OPS（约定 D）：行内写操作收敛为**每行一个「⋮」三点菜单**（RowActionsMenu：
  // 编辑/置顶，菜单项直接打开对应动作的 Dialog 表单节；两节均提交到既有 ?/update，
  // 但字段不同：编辑 = 名称/可见性/发帖策略/排序/状态，置顶 = sort_order=0；
  // 均 If-Match version + reason）。
  // GET 导航「查看」（→ 前台板块页）同样收进菜单（goto 客户端导航），
  // 行内操作有且只有「⋯」触发按钮。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { goto } from '$app/navigation';
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import Dialog from '$lib/components/ui/Dialog.svelte';
  import BatchBar from '$lib/components/admin/BatchBar.svelte';
  import RowActionsMenu from '$lib/components/admin/RowActionsMenu.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import IconPicker from '$lib/components/ui/IconPicker.svelte';
  import { boardVisuals } from '$lib/board-visuals';
  import ExportButton from '$lib/components/admin/ExportButton.svelte';
  import { adminStateLabel } from '$lib/admin';
  import { toastActionResult } from '$lib/ui/action-toast';
  import type { Board } from '$lib/api/types';
  import type { AdminBoardsPageData } from './+page.server';

  let { data, form }: { data: AdminBoardsPageData; form?: AdminBoardsPageData } = $props();

  const loadState = $derived(form?.loadState ?? data.loadState);
  const created = $derived(form?.created === true);
  const message = $derived(
    form?.message ? (form.requestId ? `${form.message}（请求号 ${form.requestId}）` : form.message) : null
  );

  // JS 启用：动作结果走全局 Toast 浮窗（成功绿/失败红）；顶部内联横幅仅保留为
  // 无 JS 回退（SSR HTML 仍渲染，见 hasJs）。
  let hasJs = $state(false);
  $effect(() => {
    hasJs = true;
  });
  let dismissError = $state(false);

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
  let submitting = $state(false);
  const allSelected = $derived(
    displayedItems.length > 0 && displayedItems.every((item) => selectedIds.includes(item.id))
  );
  const someSelected = $derived(
    displayedItems.some((item) => selectedIds.includes(item.id))
  );
  function toggleAll() {
    if (allSelected) {
      selectedIds = selectedIds.filter((id) => !displayedItems.some((i) => i.id === id));
    } else {
      const currentIds = displayedItems.map((i) => i.id);
      selectedIds = Array.from(new Set([...selectedIds, ...currentIds]));
    }
  }
  function toggleRow(id: string) {
    if (selectedIds.includes(id)) selectedIds = selectedIds.filter((x) => x !== id);
    else selectedIds = [...selectedIds, id];
  }

  /** 新建板块 Dialog（?/create）。 */
  let createOpen = $state(false);
  function openCreate(): void {
    createOpen = true;
  }

  /** 行操作弹层（每行一个「⋮」三点菜单共用；菜单项决定动作，target 区分行）。
   * 编辑与置顶同走 ?/update（If-Match version + reason），字段不同 → 按 opsAction 渲染对应表单节。 */
  type BoardOpsAction = 'edit' | 'pin';
  let opsTarget: Board | null = $state(null);
  let opsAction = $state<BoardOpsAction>('edit');
  let opsEditDraft = $state({
    name: '',
    description: '',
    icon: '',
    visibility: 'public',
    posting_mode: 'normal',
    sort_order: 0,
    is_active: 'true',
    reason: ''
  });
  let opsPinReason = $state('');

  function openOps(item: Board, action: BoardOpsAction): void {
    opsTarget = item;
    opsAction = action;
    opsEditDraft = {
      name: item.name,
      description: item.description ?? '',
      icon: item.icon ?? '',
      visibility: item.visibility ?? 'public',
      posting_mode: item.posting_mode ?? 'normal',
      sort_order: item.sort_order ?? 0,
      is_active: item.is_active ? 'true' : 'false',
      reason: ''
    };
    opsPinReason = '';
  }

  function closeOps(): void {
    opsTarget = null;
    opsEditDraft = {
      name: '',
      description: '',
      icon: '',
      visibility: 'public',
      posting_mode: 'normal',
      sort_order: 0,
      is_active: 'true',
      reason: ''
    };
    opsPinReason = '';
  }

  /** 行「⋯」菜单项（约定 D）：编辑 / 置顶 / 查看（GET 导航收进菜单）。 */
  function rowActions(item: Board) {
    return [
      { label: '编辑', run: () => openOps(item, 'edit') },
      { label: '置顶', run: () => openOps(item, 'pin') },
      { label: '查看', run: () => goto(`/boards/${encodeURIComponent(item.slug)}`) }
    ];
  }

  /** Dialog 标题/描述随菜单选定的动作变化（单动作确认，约定 D）。 */
  function opsDialogTitle(item: Board): string {
    return opsAction === 'pin' ? `置顶板块：${item.name}` : `编辑板块：${item.name}`;
  }

  function opsDialogDescription(item: Board): string {
    return opsAction === 'pin'
      ? `将「${item.name}」（/${item.slug}）排序置顶（sort_order=0）；按乐观锁版本提交并写审计。`
      : `编辑「${item.name}」（/${item.slug}）的名称/图标/可见性/发帖策略/排序/状态；按乐观锁版本提交并写审计。`;
  }

  /** 批量启用/停用 Dialog（?/batchUpdate）。 */
  let batchOpen = $state(false);
  let batchActive = $state('true');
  let batchReason = $state('');

  function openBatchSetActive(): void {
    batchActive = 'true';
    batchReason = '';
    batchOpen = true;
  }

  /** 选中行的乐观锁版本（从全量数据中检索，避免受搜索过滤影响丢失 If-Match）。 */
  const allBoardItems = $derived(loadState.state === 'ok' ? loadState.items : []);
  const selectedVersions = $derived(
    selectedIds.map((id) => {
      const item = allBoardItems.find((b) => b.id === id);
      return item ? String(item.version) : '';
    })
  );

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

<section class="app-card">
  <header class="app-card__head">
    <h2>板块列表</h2>
  </header>

  <div class="app-card__body">
    {#if message && !dismissError && !hasJs}
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
      <p class="input-hint" role="note">板块列表接口开发中（M13-ADMIN）。</p>
    {:else if loadState.state === 'error'}
      <p class="input-hint is-error" role="alert">{loadState.message || adminStateLabel('error')}</p>
    {:else if loadState.state === 'ok' && loadState.items.length === 0}
      <p class="input-hint">暂无板块数据。</p>
    {:else if loadState.state === 'ok'}
      <!-- M18：原型对齐工具栏：单行 flex（窄屏自动换行；修复全宽 select 挤压清除按钮的问题） -->
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
          <option value="active">已启用</option>
          <option value="inactive">已停用</option>
        </select>
        {#if q || statusFilter}
          <button
            type="button"
            class="btn ghost sm"
            style="flex:0 0 auto;"
            onclick={() => { q = ''; statusFilter = ''; }}
          >清除</button>
        {/if}
      </div>

      <!-- 批量工具条（选中 > 0 时渲染；批量参数在 Dialog 内填写） -->
      <BatchBar count={selectedIds.length} noun="个板块" onclear={() => (selectedIds = [])}>
        <Button text="批量启用/停用" variant="secondary" size="sm" onclick={openBatchSetActive} />
      </BatchBar>

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
                <th style="min-width:190px;">操作</th>
              </tr>
            </thead>
            <tbody>
              {#each displayedItems as item (item.id)}
                {@const visuals = boardVisuals(item.slug, item.icon)}
                <tr>
                  <td style="text-align:center;">
                    <input
                      type="checkbox"
                      checked={selectedIds.includes(item.id)}
                      onchange={() => toggleRow(item.id)}
                      aria-label="选择板块 {item.name}"
                    />
                  </td>
                <td>
                  <div style="display:flex;align-items:center;gap:8px;min-width:0;">
                    <span
                      class="adm-board-icon"
                      style="display:inline-grid;place-items:center;width:28px;height:28px;flex:none;border:1px solid color-mix(in srgb, {visuals.color} 28%, var(--color-border));background:color-mix(in srgb, {visuals.color} 8%, transparent);color:{visuals.color};border-radius:var(--radius-sm);"
                      aria-hidden="true"
                    >
                      <Icon name={visuals.icon} size={15} />
                    </span>
                    <div style="min-width:0;">
                      <b>{item.name}</b>
                      {#if item.description}
                        <span class="sub" style="display:block;margin-top:2px;">{item.description}</span>
                      {/if}
                    </div>
                  </div>
                </td>
                <td>
                  <span class="text-secondary" style="font-family:var(--font-family-mono);font-size:12px;">/{item.slug}</span>
                </td>
                <td><a class="text-link" href="/boards/{item.slug}" target="_blank" rel="noopener noreferrer" style="font-size:12px;">{item.post_count ?? 0}</a></td>
                <td><span class="sbadge {visibilityBadge(item.visibility).cls}">{visibilityBadge(item.visibility).label}</span></td>
                <td><span class="text-secondary" style="font-size:12px;">{postingModeLabel(item.posting_mode)}</span></td>
                <td>
                  <span class="text-secondary" style="font-size:12px;">
                    {(item.moderators ?? []).length ? (item.moderators ?? []).join('、') : '—'}
                  </span>
                </td>
                <td><span class="sbadge {item.is_active ? 'sb-success' : 'sb-gray'}">{item.is_active ? '启用' : '停用'}</span></td>
                <td class="adm-acts">
                  <div style="display:flex;gap:6px;flex-wrap:wrap;align-items:center;">
                    <!-- 行内操作有且只有「⋯」菜单（约定 D）：查看/编辑/置顶均在菜单内 -->
                    <RowActionsMenu label="更多操作：板块 {item.name}" actions={rowActions(item)} />
                  </div>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
    {/if}

    {#if created && !hasJs}
      <p class="input-hint" role="status" style="margin-top:14px;">板块已创建。</p>
    {/if}
    {#if message && !hasJs}
      <p class="input-hint is-error" role="alert" style="margin-top:14px;">{message}</p>
    {/if}
  </div>

  <footer class="app-card__foot">
    <Button text="新建板块" variant="primary" size="sm" onclick={openCreate} />
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
        displayedItems.map((item) => ({
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

<!-- 新建板块 Dialog：name/slug/description/visibility + reason（审计）→ ?/create -->
<Dialog
  open={createOpen}
  title="新建板块"
  description="slug 小写字母/数字/连字符且唯一；创建原因写入审计日志。"
  onclose={() => (createOpen = false)}
>
  <form
    method="POST"
    action="?/create"
    use:enhance={() => {
      submitting = true;
      return async ({ result, update }) => {
        submitting = false;
        toastActionResult(result);
        await update();
        // 仅成功时关闭弹层：失败保留已填内容便于修正（错误经 Toast/横幅呈现）。
        if (result.type === 'success') createOpen = false;
      };
    }}
  >
    <div class="input-wrapper" style="margin-bottom:var(--space-3);">
      <label class="input-label" for="admin-board-name">名称</label>
      <input type="text" class="input-field" id="admin-board-name" name="name" maxlength="100" required />
    </div>
    <div class="input-wrapper" style="margin-bottom:var(--space-3);">
      <label class="input-label" for="admin-board-slug">slug</label>
      <input type="text" class="input-field" id="admin-board-slug" name="slug" maxlength="120" pattern="[a-z0-9-]+" required />
      <p class="input-hint" style="margin:4px 0 0;">小写字母/数字/连字符，唯一。</p>
    </div>
    <div class="input-wrapper" style="margin-bottom:var(--space-3);">
      <label class="input-label" for="admin-board-desc">说明</label>
      <textarea class="input-field" id="admin-board-desc" name="description" rows="3" maxlength="2000"></textarea>
    </div>
    <div style="margin-bottom:var(--space-3);">
      <IconPicker name="icon" id="admin-board-icon" />
    </div>
    <div class="input-wrapper" style="margin-bottom:var(--space-3);">
      <label class="input-label" for="admin-board-visibility">可见性</label>
      <select class="input-field" id="admin-board-visibility" name="visibility">
        <option value="public">public（公开）</option>
        <option value="members">members（登录成员）</option>
        <option value="restricted">restricted（需加入）</option>
        <option value="hidden">hidden（管理可见）</option>
      </select>
    </div>
    <div class="input-wrapper" style="margin-bottom:var(--space-3);">
      <label class="input-label" for="admin-board-posting-mode">发帖策略</label>
      <select class="input-field" id="admin-board-posting-mode" name="posting_mode">
        <option value="normal">normal（正常发帖）</option>
        <option value="approval">approval（先审后发）</option>
        <option value="readonly">readonly（只读）</option>
        <option value="closed">closed（关闭）</option>
      </select>
    </div>
    <div class="input-wrapper" style="margin-bottom:var(--space-3);">
      <label class="input-label" for="admin-board-reason">创建原因（审计）</label>
      <input type="text" class="input-field" id="admin-board-reason" name="reason" placeholder="如：新增技术专区" required />
    </div>
    <Button text={submitting ? '提交中...' : '提交创建'} variant="primary" size="sm" type="submit" disabled={submitting} />
  </form>
</Dialog>

<!-- 行操作 Dialog（约定 D）：动作由「⋮」菜单项决定，弹层内单动作确认——编辑与置顶均提交到
     既有 ?/update（If-Match version + reason），字段不同 → 按 opsAction 渲染对应表单节。 -->
<Dialog
  open={opsTarget !== null}
  title={opsTarget ? opsDialogTitle(opsTarget) : '板块操作'}
  description={opsTarget ? opsDialogDescription(opsTarget) : ''}
  onclose={closeOps}
>
  <!-- 节 1：编辑板块配置（?/update，If-Match version + reason）——菜单项「编辑」进入 -->
  {#if opsTarget && opsAction === 'edit'}
  <section style="border:1px solid var(--color-border);border-radius:var(--radius-md);padding:12px 14px;margin-bottom:var(--space-3);">
    <h3 style="margin:0 0 10px;font-size:13px;font-weight:600;">编辑板块配置</h3>
    <form
      method="POST"
      action="?/update"
      use:enhance={() => {
        submitting = true;
        return async ({ result, update }) => {
          submitting = false;
          toastActionResult(result);
          await update();
          if (result.type === 'success') closeOps();
        };
      }}
    >
      <input type="hidden" name="id" value={opsTarget?.id ?? ''} />
      <input type="hidden" name="version" value={opsTarget ? String(opsTarget.version) : ''} />
      <div class="input-wrapper" style="margin-bottom:var(--space-3);">
        <label class="input-label" for="eb-ops-name">名称</label>
        <input type="text" class="input-field" id="eb-ops-name" name="name" maxlength="100" required bind:value={opsEditDraft.name} />
      </div>
      <div class="input-wrapper" style="margin-bottom:var(--space-3);">
        <label class="input-label" for="eb-ops-desc">说明</label>
        <textarea class="input-field" id="eb-ops-desc" name="description" rows="2" maxlength="2000" bind:value={opsEditDraft.description}></textarea>
      </div>
      <div style="margin-bottom:var(--space-3);">
        <IconPicker name="icon" id="eb-ops-icon" bind:value={opsEditDraft.icon} />
      </div>
      <div class="input-wrapper" style="margin-bottom:var(--space-3);">
        <label class="input-label" for="eb-ops-vis">可见性</label>
        <select class="input-field" id="eb-ops-vis" name="visibility" bind:value={opsEditDraft.visibility}>
          <option value="public">公开</option>
          <option value="members">登录成员</option>
          <option value="restricted">需加入</option>
          <option value="hidden">管理可见</option>
        </select>
      </div>
      <div class="input-wrapper" style="margin-bottom:var(--space-3);">
        <label class="input-label" for="eb-ops-mode">发帖策略</label>
        <select class="input-field" id="eb-ops-mode" name="posting_mode" bind:value={opsEditDraft.posting_mode}>
          <option value="normal">正常发帖</option>
          <option value="approval">先审后发</option>
          <option value="readonly">只读</option>
          <option value="closed">关闭</option>
        </select>
      </div>
      <div class="input-wrapper" style="margin-bottom:var(--space-3);">
        <label class="input-label" for="eb-ops-sort">排序（0 = 置顶）</label>
        <input type="number" class="input-field" id="eb-ops-sort" name="sort_order" min="0" step="1" bind:value={opsEditDraft.sort_order} />
      </div>
      <div class="input-wrapper" style="margin-bottom:var(--space-3);">
        <label class="input-label" for="eb-ops-active">状态</label>
        <select class="input-field" id="eb-ops-active" name="is_active" bind:value={opsEditDraft.is_active}>
          <option value="true">启用</option>
          <option value="false">停用</option>
        </select>
      </div>
      <div class="input-wrapper" style="margin-bottom:var(--space-3);">
        <label class="input-label" for="eb-ops-reason">操作原因（审计）</label>
        <input type="text" class="input-field" id="eb-ops-reason" name="reason" placeholder="如：调整板块可见性" required bind:value={opsEditDraft.reason} />
      </div>
      <Button text={submitting ? '保存中...' : '保存编辑'} variant="primary" size="sm" type="submit" disabled={submitting} />
    </form>
  </section>
  {/if}

  <!-- 节 2：置顶（?/update：sort_order=0 隐藏字段 + If-Match version + reason）——菜单项「置顶」进入 -->
  {#if opsTarget && opsAction === 'pin'}
  <section style="border:1px solid var(--color-border);border-radius:var(--radius-md);padding:12px 14px;">
    <h3 style="margin:0 0 10px;font-size:13px;font-weight:600;">置顶板块</h3>
    <form
      method="POST"
      action="?/update"
      use:enhance={() => {
        submitting = true;
        return async ({ result, update }) => {
          submitting = false;
          toastActionResult(result);
          await update();
          if (result.type === 'success') closeOps();
        };
      }}
    >
      <input type="hidden" name="id" value={opsTarget?.id ?? ''} />
      <input type="hidden" name="version" value={opsTarget ? String(opsTarget.version) : ''} />
      <input type="hidden" name="sort_order" value="0" />
      <div class="input-wrapper" style="margin-bottom:var(--space-3);">
        <label class="input-label" for="eb-ops-pin-reason">置顶原因（审计）</label>
        <input type="text" class="input-field" id="eb-ops-pin-reason" name="reason" placeholder="如：活动公告置顶" required bind:value={opsPinReason} />
      </div>
      <Button text={submitting ? '提交中...' : '确认置顶'} variant="primary" size="sm" type="submit" disabled={submitting} />
    </form>
  </section>
  {/if}
</Dialog>

<!-- 批量启用/停用 Dialog：ids/versions 一一对应（If-Match）+ reason 必填 → ?/batchUpdate -->
<Dialog
  open={batchOpen}
  title="批量启用/停用"
  description={`将对 ${selectedIds.length} 个板块执行同一启停动作；原因写审计，逐条按乐观锁版本提交。`}
  onclose={() => (batchOpen = false)}
>
  <form
    method="POST"
    action="?/batchUpdate"
    use:enhance={() => {
      submitting = true;
      return async ({ result, update }) => {
        submitting = false;
        toastActionResult(result);
        await update();
        if (result.type === 'success') {
          selectedIds = [];
          batchOpen = false;
        }
      };
    }}
  >
    <input type="hidden" name="ids" value={selectedIds.join(',')} />
    <input type="hidden" name="versions" value={selectedVersions.join(',')} />
    <div class="input-wrapper" style="margin-bottom:var(--space-3);">
      <label class="input-label" for="eb-batch-active">目标状态</label>
      <select id="eb-batch-active" name="is_active" class="input-field" bind:value={batchActive}>
        <option value="true">启用</option>
        <option value="false">停用</option>
      </select>
    </div>
    <div class="input-wrapper" style="margin-bottom:var(--space-3);">
      <label class="input-label" for="eb-batch-reason">操作原因（审计）</label>
      <input type="text" class="input-field" id="eb-batch-reason" name="reason" required placeholder="必填" bind:value={batchReason} />
    </div>
    <Button text={submitting ? '执行中...' : '确认执行'} variant="primary" size="sm" type="submit" disabled={submitting} />
  </form>
</Dialog>
