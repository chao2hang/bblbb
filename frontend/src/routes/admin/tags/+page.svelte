<script lang="ts">
  // M03-UI-07：管理标签页——后端裁决状态渲染 + 新建标签表单。
  // 原型对齐：prototype/pages/admin-tags.html
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import ExportButton from '$lib/components/admin/ExportButton.svelte';
  import { adminStateLabel } from '$lib/admin';
  import type { AdminTagsActionData, AdminTagsPageData } from './+page.server';

  let { data, form }: { data: AdminTagsPageData; form?: AdminTagsActionData | null } = $props();

  const loadState = $derived(form?.loadState ?? data.loadState);
  /** 合并操作的目标选择（每行展开）。 */
  let mergingId = $state<string | null>(null);
  const created = $derived(form?.created === true);
  const message = $derived(
    form?.message ? (form.requestId ? `${form.message}（请求号 ${form.requestId}）` : form.message) : null
  );

  let showCreate = $state(false);

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
    if (statusFilter === 'inactive') list = list.filter((i) => i.is_active === 0 || i.is_active === false);
    if (statusFilter === 'merged') list = list.filter((i) => i.status === 'merged');
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

</script>

<svelte:head>
  <title>标签管理 — BBLBB Admin</title>
</svelte:head>

<PageHeader title="标签管理" />

<section class="app-card">
  <header class="app-card__head">
    <h2>标签列表</h2>
  </header>

  <div class="app-card__body">
    {#if loadState.state === 'forbidden'}
      <p class="input-hint is-error" role="alert"><Icon name="lock" size={14} /> {adminStateLabel('forbidden')}</p>
    {:else if loadState.state === 'not_implemented'}
      <p class="input-hint" role="note">标签列表接口开发中（M13-ADMIN）。创建表单已可用。</p>
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

      {#if selectedIds.length > 0}
        <div class="app-notice" style="display:flex;align-items:center;justify-content:space-between;padding:8px 12px;margin-bottom:10px;background:var(--color-bg-subtle);border-radius:var(--radius-sm);">
          <span style="font-size:var(--text-xs);font-weight:600;">{selectedIds.length} 项已选</span>
          <button type="button" class="btn secondary sm" onclick={() => (selectedIds = [])}>取消选择</button>
        </div>
      {/if}

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
                      aria-label="选择此项"
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
                  {:else if item.is_active === 0 || item.is_active === false}
                    <span class="sbadge sb-gray">已停用</span>
                  {:else}
                    <span class="sbadge sb-success">正常</span>
                  {/if}
                </td>
                <td class="adm-acts">
                  <a class="btn ghost sm" href="/search?tag={item.slug}">查看</a>
                  <form
                    method="POST"
                    action="?/toggle"
                    use:enhance
                    style="display:inline-flex;gap:4px;align-items:center;margin:0;"
                  >
                    <input type="hidden" name="id" value={item.id} />
                    <input type="hidden" name="version" value={item.updated_at ?? 1} />
                    <input type="hidden" name="is_active" value={item.is_active === 0 || item.is_active === false ? 'true' : 'false'} />
                    <input
                      type="text"
                      class="input-field"
                      name="reason"
                      placeholder="原因（审计）"
                      required
                      aria-label="{item.is_active === 0 || item.is_active === false ? '启用' : '停用'} {item.name} 的原因"
                      style="width:110px;height:30px;"
                    />
                    <button type="submit" class="btn secondary sm">
                      {item.is_active === 0 || item.is_active === false ? '启用' : '停用'}
                    </button>
                  </form>
                  <button
                    type="button"
                    class="btn ghost sm"
                    onclick={() => (mergingId = mergingId === item.id ? null : item.id)}
                  >
                    合并
                  </button>
                </td>
              </tr>
              {#if mergingId === item.id}
                <tr>
                  <td colspan="4" style="background:var(--color-bg-subtle);">
                    <form method="POST" action="?/merge" use:enhance style="display:flex;gap:10px;align-items:flex-end;flex-wrap:wrap;padding:10px 4px;">
                      <input type="hidden" name="id" value={item.id} />
                      <div>
                        <label class="input-label" for={`tg-target-${item.id}`}>并入目标标签</label>
                        <select class="input-field" id={`tg-target-${item.id}`} name="target_id" required style="width:200px;">
                          <option value="" disabled selected>选择目标…</option>
                          {#each loadState.items as t (t.id)}
                            {#if t.id !== item.id && t.status !== 'merged'}
                              <option value={t.id}>{t.name}</option>
                            {/if}
                          {/each}
                        </select>
                      </div>
                      <div>
                        <label class="input-label" for={`tg-reason-${item.id}`}>操作原因（审计）</label>
                        <input type="text" class="input-field" id={`tg-reason-${item.id}`} name="reason" required placeholder="如：同义标签归并" style="width:200px;" />
                      </div>
                      <div style="display:flex;gap:8px;">
                        <button type="submit" class="btn primary sm">确认合并</button>
                        <button type="button" class="btn ghost sm" onclick={() => (mergingId = null)}>取消</button>
                      </div>
                      <p class="input-hint" style="margin:0;">合并后：帖子关联转移至目标，源标签标记「已合并」并停用。</p>
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

    {#if message}
      <p class="input-hint" role="status" style="margin-top:12px;">{message}</p>
    {/if}

    {#if created}
      <p class="input-hint" role="status" style="margin-top:14px;">标签已创建。</p>
    {/if}
    {#if message}
      <p class="input-hint is-error" role="alert" style="margin-top:14px;">{message}</p>
    {/if}
  </div>

  <footer class="app-card__foot">
    <button type="button" class="btn primary sm" onclick={() => (showCreate = !showCreate)}>
      {showCreate ? '收起表单' : '新建标签'}
    </button>
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
        (loadState.state === 'ok' ? loadState.items : []).map((item) => ({
          name: item.name,
          slug: item.slug,
          usage: item.usage_count ?? 0,
          status: item.status === 'merged' ? '已合并' : item.is_active === 0 || item.is_active === false ? '已停用' : '正常'
        }))}
    />
  </footer>
</section>

<!-- 新建标签表单 -->
<section class="app-card" style="margin-top:16px;{showCreate || loadState.state === 'not_implemented' ? '' : 'display:none;'}">
  <header class="app-card__head">
    <h2>新建标签</h2>
  </header>

  <div class="app-card__body">
    <form method="POST" action="?/create" use:enhance style="display:flex;flex-direction:column;gap:14px;max-width:520px;">
      <div>
        <label class="input-label" for="admin-tag-name">名称</label>
        <input type="text" class="input-field" id="admin-tag-name" name="name" maxlength="40" required />
      </div>

      <div>
        <label class="input-label" for="admin-tag-reason">操作原因（审计）</label>
        <input type="text" class="input-field" id="admin-tag-reason" name="reason" required placeholder="记录到审计日志" />
      </div>

      <div style="margin-top:8px;">
        <button type="submit" class="btn primary sm">创建标签</button>
      </div>
    </form>
  </div>
</section>
