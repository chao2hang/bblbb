<script lang="ts">
  // M03-UI-07：管理标签页——后端裁决状态渲染 + 标签 CRUD 行操作。
  // 原型对齐：prototype/pages/admin-tags.html
  // M18-ADMIN-DIALOG：写操作弹层化——
  // - 「新建标签」常驻表单卡 → 页脚按钮 + Dialog（?/create）；
  // - 行「编辑」展开行 → 按钮 + Dialog（?/update，name/description + If-Match version + reason）；
  // - 行「启停」行内表单 → 按钮 + Dialog（?/toggle，reason 必填）；
  // - 行「合并」展开行 → 按钮 + DangerConfirm（?/merge，target_id + reason）；
  // - 选择列接入 BatchBar + 批量启用/停用 Dialog → ?/batchToggle
  //   （服务端循环既有 PATCH /api/v1/admin/tags/{id} 启停端点）。
  // M18-ADMIN-OPS（约定 D）：行内写操作收敛为**每行一个「⋮」三点菜单**
  // （RowActionsMenu：编辑/启停/合并，点菜单项直接打开该动作的 Dialog 表单节；
  // 编辑 ?/update / 启停 ?/toggle / 合并 ?/merge：不同端点/不同字段，各带自己的
  // 隐藏 id/version/reason 与提交按钮；合并为破坏性操作，提交按钮转 danger 并附说明；
  // 已合并行不提供「合并」菜单项）。「查看」（GET 导航 → 标签聚合页）同样收进菜单
  // （goto 客户端导航），行内操作有且只有「⋯」触发按钮。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { goto } from '$app/navigation';
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import Dialog from '$lib/components/ui/Dialog.svelte';
  import BatchBar from '$lib/components/admin/BatchBar.svelte';
  import RowActionsMenu from '$lib/components/admin/RowActionsMenu.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import ExportButton from '$lib/components/admin/ExportButton.svelte';
  import { adminStateLabel } from '$lib/admin';
  import { tagSearchUrl } from '$lib/search';
  import { show as showToast } from '$lib/ui/toast';
  import { toastActionResult } from '$lib/ui/action-toast';
  import type { AdminTagItem, AdminTagsActionData, AdminTagsPageData } from './+page.server';

  let { data, form }: { data: AdminTagsPageData; form?: AdminTagsActionData | null } = $props();

  const loadState = $derived(form?.loadState ?? data.loadState);
  const message = $derived(
    form?.message ? (form.requestId ? `${form.message}（请求号 ${form.requestId}）` : form.message) : null
  );

  // JS 启用：动作结果走全局 Toast 浮窗（成功绿/失败红）；顶部内联横幅仅保留为
  // 无 JS 回退（SSR HTML 仍渲染，见 hasJs）。
  let hasJs = $state(false);
  $effect(() => {
    hasJs = true;
  });

  // M18：工具条搜索与复选框（对齐原型表格通用模式）
  let q = $state('');
  let statusFilter = $state('');
  let selectedIds = $state<string[]>([]);
  const displayedItems = $derived.by(() => {
    let list = loadState.state === 'ok' ? loadState.items : [];
    if (q.trim()) {
      const lower = q.trim().toLowerCase();
      list = list.filter((i) => i.name.toLowerCase().includes(lower));
    }
    if (statusFilter === 'active') list = list.filter((i) => i.is_active !== 0 && i.is_active !== false && i.status !== 'merged');
    if (statusFilter === 'inactive') list = list.filter((i) => (i.is_active === 0 || i.is_active === false) && i.status !== 'merged');
    if (statusFilter === 'merged') list = list.filter((i) => i.status === 'merged');
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

  function isActive(item: AdminTagItem): boolean {
    return !(item.is_active === 0 || item.is_active === false);
  }

  /** 新建标签 Dialog（?/create）。 */
  let createOpen = $state(false);
  let createName = $state('');
  let createReason = $state('');

  function openCreate(): void {
    createName = '';
    createReason = '';
    createOpen = true;
  }

  /** 行操作弹层（每行一个「⋮」三点菜单共用；菜单项决定动作，target 区分行）。
   * 编辑（?/update：name/description + If-Match）/ 启停（?/toggle：is_active + If-Match）/
   * 合并（?/merge：target_id + reason）各为独立表单节，Dialog 按 opsAction 渲染对应节。 */
  type TagOpsAction = 'update' | 'toggle' | 'merge';
  let opsTarget: AdminTagItem | null = $state(null);
  let opsAction = $state<TagOpsAction>('update');
  let opsName = $state('');
  let opsDescription = $state('');
  let opsEditReason = $state('');
  let opsToggleReason = $state('');
  let opsMergeToId = $state('');
  let opsMergeReason = $state('');

  function openOps(item: AdminTagItem, action: TagOpsAction): void {
    opsTarget = item;
    opsAction = action;
    opsName = item.name;
    opsDescription = item.description ?? '';
    opsEditReason = '';
    opsToggleReason = '';
    opsMergeToId = '';
    opsMergeReason = '';
  }

  function closeOps(): void {
    opsTarget = null;
    opsName = '';
    opsDescription = '';
    opsEditReason = '';
    opsToggleReason = '';
    opsMergeToId = '';
    opsMergeReason = '';
  }

  /** 行「⋯」菜单项（约定 D；由原 opsOptionsFor 映射）：编辑 / 启停 / 合并 / 查看。 */
  function rowActions(item: AdminTagItem) {
    if (item.status === 'merged') {
      return [{ label: '查看', run: () => goto(tagSearchUrl(item)) }];
    }
    const actions: { label: string; danger?: boolean; run: () => void }[] = [
      { label: '编辑', run: () => openOps(item, 'update') },
      { label: isActive(item) ? '停用' : '启用', run: () => openOps(item, 'toggle') }
    ];
    actions.push({ label: '合并', danger: true, run: () => openOps(item, 'merge') });
    // 「查看」GET 导航收进菜单（原行内 <a> 链接同语义，goto 客户端跳转）。
    actions.push({ label: '查看', run: () => goto(tagSearchUrl(item)) });
    return actions;
  }

  /** Dialog 标题/描述随菜单选定的动作变化（单动作确认，约定 D）。 */
  function opsDialogTitle(item: AdminTagItem): string {
    switch (opsAction) {
      case 'update':
        return `编辑标签：${item.name}`;
      case 'toggle':
        return `${isActive(item) ? '停用' : '启用'}标签：${item.name}`;
      case 'merge':
        return `合并标签：${item.name}`;
    }
  }

  function opsDialogDescription(item: AdminTagItem): string {
    switch (opsAction) {
      case 'update':
        return `编辑「${item.name}」的名称与描述；操作原因写入审计日志。`;
      case 'toggle':
        return `将「${item.name}」${isActive(item) ? '停用' : '启用'}；操作原因写入审计日志。`;
      case 'merge':
        return `将「${item.name}」并入其他标签；合并后帖子关联转移且不可恢复，操作原因写入审计日志。`;
    }
  }

  /** 可并入的目标标签（排除自身与已合并标签，与原展开行逻辑一致）。 */
  function mergeCandidates(item: AdminTagItem): AdminTagItem[] {
    return (loadState.state === 'ok' ? loadState.items : []).filter(
      (t) => t.id !== item.id && t.status !== 'merged'
    );
  }

  /** 批量启用/停用 Dialog（?/batchToggle）。 */
  let batchOpen = $state(false);
  let batchActive = $state('true');
  let batchReason = $state('');

  function openBatchToggle(): void {
    batchActive = 'true';
    batchReason = '';
    batchOpen = true;
  }

  /** 选中行的乐观锁版本（从全量数据中查找，避免受搜索过滤影响丢失 If-Match）。 */
  const allTagItems = $derived(loadState.state === 'ok' ? loadState.items : []);
  const selectedVersions = $derived(
    selectedIds.map((id) => {
      const item = allTagItems.find((t) => t.id === id);
      return item ? String(item.updated_at ?? 1) : '';
    })
  );

</script>

<svelte:head>
  <title>标签管理 — BBLBB Admin</title>
</svelte:head>

<section class="app-card">
  <header class="app-card__head">
    <h2>标签列表</h2>
  </header>

  <div class="app-card__body">
    {#if loadState.state === 'forbidden'}
      <p class="input-hint is-error" role="alert"><Icon name="lock" size={14} /> {adminStateLabel('forbidden')}</p>
    {:else if loadState.state === 'not_implemented'}
      <p class="input-hint" role="note">标签列表接口开发中（M13-ADMIN）。</p>
    {:else if loadState.state === 'error'}
      <p class="input-hint is-error" role="alert">{loadState.message || adminStateLabel('error')}</p>
    {:else if loadState.state === 'ok' && loadState.items.length === 0}
      <p class="input-hint">暂无标签数据。</p>
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
            <option value="active">正常</option>
            <option value="inactive">已停用</option>
            <option value="merged">已合并</option>
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

      <!-- 批量工具条（选中 > 0 时渲染；批量参数在 Dialog 内填写） -->
      <BatchBar count={selectedIds.length} noun="个标签" onclear={() => (selectedIds = [])}>
        <Button text="批量启用/停用" variant="secondary" size="sm" onclick={openBatchToggle} />
      </BatchBar>

      {#if displayedItems.length === 0}
        <p class="input-hint">没有匹配的标签。</p>
      {:else}
        <div class="app-table-wrap">
          <table class="app-table" aria-label="标签列表">
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
                <th>标签</th>
                <th>使用次数</th>
                <th>状态</th>
                <th>操作</th>
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
                      aria-label="选择标签 {item.name}"
                    />
                  </td>
                <td>
                  <b>{item.name}</b>
                </td>
                <td class="num">
                  {item.usage_count ?? 0}
                </td>
                <td>
                  {#if item.status === 'merged'}
                    <span class="sbadge sb-gray">已合并</span>
                  {:else if !isActive(item)}
                    <span class="sbadge sb-gray">已停用</span>
                  {:else}
                    <span class="sbadge sb-success">正常</span>
                  {/if}
                </td>
                <td class="adm-acts">
                  <div style="display:flex;gap:6px;flex-wrap:wrap;align-items:center;">
                    <!-- 行内操作有且只有「⋯」菜单（约定 D）：查看/编辑/启停/合并均在菜单内 -->
                    <RowActionsMenu label="更多操作：标签 {item.name}" actions={rowActions(item)} />
                  </div>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
    {/if}

    {#if message && !hasJs}
      <p class="input-hint" role="status" style="margin-top:12px;">{message}</p>
    {/if}
  </div>

  <footer class="app-card__foot">
    <Button text="新建标签" variant="primary" size="sm" onclick={openCreate} />
    <ExportButton
      label="导出标签"
      filename="admin-tags"
      columns={[
        { key: 'name', label: '名称' },
        { key: 'slug', label: 'slug' },
        { key: 'usage', label: '使用次数' },
        { key: 'status', label: '状态' }
      ]}
      getData={() =>
        displayedItems.map((item) => ({
          name: item.name,
          slug: item.slug,
          usage: item.usage_count ?? 0,
          status: item.status === 'merged' ? '已合并' : !isActive(item) ? '已停用' : '正常'
        }))}
    />
  </footer>
</section>

<!-- 新建标签 Dialog：name + reason（审计）→ ?/create。
     创建成功时 server 只返回 created（无 message）：自定义回调补一条成功 Toast。 -->
<Dialog
  open={createOpen}
  title="新建标签"
  description="名称唯一；操作原因写入审计日志。"
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
      <label class="input-label" for="admin-tag-name">名称</label>
      <input type="text" class="input-field" id="admin-tag-name" name="name" maxlength="40" required bind:value={createName} />
    </div>
    <div class="input-wrapper" style="margin-bottom:var(--space-3);">
      <label class="input-label" for="admin-tag-reason">操作原因（审计）</label>
      <input type="text" class="input-field" id="admin-tag-reason" name="reason" required placeholder="记录到审计日志" bind:value={createReason} />
    </div>
    <Button text={submitting ? '创建中...' : '创建标签'} variant="primary" size="sm" type="submit" disabled={submitting} />
  </form>
</Dialog>

<!-- 行操作 Dialog（约定 D）：动作由「⋮」菜单项决定，弹层内单动作确认——
     编辑（?/update：name/description + If-Match）/ 启停（?/toggle：is_active + If-Match）/
     合并（?/merge：target_id + reason，破坏性 → danger 提交按钮）。各节独立提交，
     Dialog 按 opsAction 渲染对应表单节，target 区分行。 -->
<Dialog
  open={opsTarget !== null}
  title={opsTarget ? opsDialogTitle(opsTarget) : '标签操作'}
  description={opsTarget ? opsDialogDescription(opsTarget) : ''}
  onclose={closeOps}
>
  <!-- 节 1：编辑名称/描述（?/update，If-Match version）——菜单项「编辑」进入 -->
  {#if opsTarget && opsAction === 'update'}
  <section style="border:1px solid var(--color-border);border-radius:var(--radius-md);padding:12px 14px;margin-bottom:var(--space-3);">
    <h3 style="margin:0 0 10px;font-size:13px;font-weight:600;">编辑名称 / 描述</h3>
    <form
      method="POST"
      action="?/update"
      use:enhance={() => {
        submitting = true;
        return async ({ result, update }) => {
          submitting = false;
          toastActionResult(result);
          await update({ reset: false });
          if (result.type === 'success') closeOps();
        };
      }}
    >
      <input type="hidden" name="id" value={opsTarget?.id ?? ''} />
      <input type="hidden" name="version" value={opsTarget ? String(opsTarget.updated_at ?? 1) : ''} />
      <div class="input-wrapper" style="margin-bottom:var(--space-3);">
        <label class="input-label" for="tg-ops-name">标签名称</label>
        <input type="text" class="input-field" id="tg-ops-name" name="name" required maxlength="40" bind:value={opsName} />
      </div>
      <div class="input-wrapper" style="margin-bottom:var(--space-3);">
        <label class="input-label" for="tg-ops-desc">描述（清空即删除描述）</label>
        <input type="text" class="input-field" id="tg-ops-desc" name="description" maxlength="200" placeholder="标签用途说明" bind:value={opsDescription} />
      </div>
      <div class="input-wrapper" style="margin-bottom:var(--space-3);">
        <label class="input-label" for="tg-ops-edit-reason">操作原因（审计）</label>
        <input type="text" class="input-field" id="tg-ops-edit-reason" name="reason" required placeholder="如：统一命名规范" bind:value={opsEditReason} />
      </div>
      <Button text={submitting ? '保存中...' : '保存编辑'} variant="primary" size="sm" type="submit" disabled={submitting} />
    </form>
  </section>
  {/if}

  <!-- 节 2：启用/停用（?/toggle，is_active 隐藏字段 + If-Match version）——菜单项「停用/启用」进入 -->
  {#if opsTarget && opsAction === 'toggle'}
  <section style="border:1px solid var(--color-border);border-radius:var(--radius-md);padding:12px 14px;margin-bottom:var(--space-3);">
    <h3 style="margin:0 0 10px;font-size:13px;font-weight:600;">{opsTarget && isActive(opsTarget) ? '停用标签' : '启用标签'}</h3>
    <form
      method="POST"
      action="?/toggle"
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
      <input type="hidden" name="version" value={opsTarget ? String(opsTarget.updated_at ?? 1) : ''} />
      <input type="hidden" name="is_active" value={opsTarget ? (isActive(opsTarget) ? 'false' : 'true') : ''} />
      <div class="input-wrapper" style="margin-bottom:var(--space-3);">
        <label class="input-label" for="tg-ops-toggle-reason">操作原因（审计）</label>
        <input type="text" class="input-field" id="tg-ops-toggle-reason" name="reason" required placeholder="如：清理失效标签" bind:value={opsToggleReason} />
      </div>
      <Button
        text={submitting ? '处理中...' : (opsTarget && isActive(opsTarget) ? '确认停用' : '确认启用')}
        variant="primary"
        size="sm"
        type="submit"
        disabled={submitting}
      />
    </form>
  </section>
  {/if}

  <!-- 节 3：合并（?/merge，target_id + reason；已合并行无「合并」菜单项，此处仍保留守卫） -->
  {#if opsTarget && opsAction === 'merge' && opsTarget.status !== 'merged'}
    <section style="border:1px solid var(--color-border);border-radius:var(--radius-md);padding:12px 14px;">
      <h3 style="margin:0 0 10px;font-size:13px;font-weight:600;">合并到其他标签</h3>
      <form
        method="POST"
        action="?/merge"
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
        <input type="hidden" name="id" value={opsTarget.id} />
        <div class="input-wrapper" style="margin-bottom:var(--space-3);">
          <label class="input-label" for="tg-ops-merge-target">并入目标标签</label>
          <select class="input-field" id="tg-ops-merge-target" name="target_id" bind:value={opsMergeToId} required>
            <option value="" disabled>选择目标…</option>
            {#each mergeCandidates(opsTarget) as t (t.id)}
              <option value={t.id}>{t.name}</option>
            {/each}
          </select>
        </div>
        <div class="input-wrapper" style="margin-bottom:var(--space-3);">
          <label class="input-label" for="tg-ops-merge-reason">操作原因（审计）</label>
          <input type="text" class="input-field" id="tg-ops-merge-reason" name="reason" bind:value={opsMergeReason} required placeholder="如：同义标签归并" />
        </div>
        <p class="input-hint is-error" style="margin:0 0 10px;">
          合并后帖子关联转移至目标标签，源标签标记「已合并」并停用，不可恢复。
        </p>
        <Button text={submitting ? '合并中...' : '确认合并'} variant="danger" size="sm" type="submit" disabled={submitting} />
      </form>
    </section>
  {/if}
</Dialog>

<!-- 批量启用/停用 Dialog：ids/versions 一一对应（If-Match）+ reason 必填 → ?/batchToggle -->
<Dialog
  open={batchOpen}
  title="批量启用/停用"
  description={`将对 ${selectedIds.length} 个标签执行同一启停动作；原因写审计，逐条按乐观锁版本提交。`}
  onclose={() => (batchOpen = false)}
>
  <form
    method="POST"
    action="?/batchToggle"
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
      <label class="input-label" for="tg-batch-active">目标状态</label>
      <select id="tg-batch-active" name="is_active" class="input-field" bind:value={batchActive}>
        <option value="true">启用</option>
        <option value="false">停用</option>
      </select>
    </div>
    <div class="input-wrapper" style="margin-bottom:var(--space-3);">
      <label class="input-label" for="tg-batch-reason">操作原因（审计）</label>
      <input type="text" class="input-field" id="tg-batch-reason" name="reason" required placeholder="必填" bind:value={batchReason} />
    </div>
    <Button text={submitting ? '执行中...' : '确认执行'} variant="primary" size="sm" type="submit" disabled={submitting} />
  </form>
</Dialog>
