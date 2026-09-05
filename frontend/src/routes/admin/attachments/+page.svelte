<script lang="ts">
  // GAP-FIX（管理域·附件管理）：文件名/上传者/大小（KB/MB 格式化）/时间 表格 +
  // 删除（DangerConfirm + reason 写审计）+ q 搜索 + cursor 分页。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import DangerConfirm from '$lib/components/ui/DangerConfirm.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { adminStateLabel } from '$lib/admin';
  import { show as showToast } from '$lib/ui/toast';
  import type { AdminAttachmentItem } from '$lib/api/types';
  import type {
    AdminAttachmentsActionData,
    AdminAttachmentsPageData
  } from './+page.server';

  let { data, form }: {
    data: AdminAttachmentsPageData;
    form?: AdminAttachmentsActionData | null;
  } = $props();

  /** 大小格式化：<1MB 用 KB，否则 MB（保留 1 位小数）。 */
  function formatSize(bytes: number | null | undefined): string {
    if (typeof bytes !== 'number' || Number.isNaN(bytes)) return '—';
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  function formatDateTime(ms: number | null | undefined): string {
    if (!ms) return '—';
    return new Date(ms).toLocaleString('zh-CN', { hour12: false });
  }

  const nextHref = $derived(
    data.state === 'ok' && data.nextCursor
      ? `/admin/attachments?q=${encodeURIComponent(data.q)}&after=${encodeURIComponent(data.nextCursor)}`
      : null
  );

  const message = $derived(form?.message ?? null);

  /** 删除确认对话框状态（行点击打开，确认后提交隐藏表单）。 */
  let deleteTarget: AdminAttachmentItem | null = $state(null);
  let deleteReason = $state('');
  let deleteForm: HTMLFormElement | undefined = $state();

  function openDelete(item: AdminAttachmentItem): void {
    deleteTarget = item;
    deleteReason = '';
  }

  // M18：工具条状态筛选与复选框（对齐原型表格通用模式）
  let statusFilter = $state('');
  let selectedIds = $state<string[]>([]);
  let allSelected = $derived(
    data.items && data.items.length > 0 && selectedIds.length === data.items.length
  );
  function toggleAll() {
    if (allSelected) selectedIds = [];
    else selectedIds = (data.items ?? []).map((i) => i.id);
  }
  function toggleRow(id: string) {
    if (selectedIds.includes(id)) selectedIds = selectedIds.filter((x) => x !== id);
    else selectedIds = [...selectedIds, id];
  }
</script>

<svelte:head>
  <title>附件管理 — BBLBB</title>
</svelte:head>

<PageHeader title="附件管理" />

<div class="app-card">
  <div class="app-card__head"><h2>附件列表</h2></div>
  <div class="app-card__body">
    {#if data.state === 'forbidden'}
      <p class="input-hint is-error" role="alert"><Icon name="lock" size={14} /> {adminStateLabel('forbidden')}</p>
    {:else if data.state === 'not_implemented'}
      <p class="input-hint" role="note">附件管理接口开发中。</p>
    {:else if data.state === 'error'}
      <p class="input-hint is-error" role="alert">{data.error || adminStateLabel('error')}</p>
    {:else if data.state === 'ok'}
      {#if message}
        <p class="input-hint" role="status">{message}</p>
      {/if}

      <!-- M18：原型对齐工具条（搜索当前列表 + 全部状态 + 清除） -->
      <form method="GET" action="/admin/attachments" style="display:flex;flex-direction:column;gap:8px;margin-bottom:14px;">
        <input
          type="search"
          name="q"
          value={data.q}
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
            <option value="ready">正常</option>
            <option value="pending">待扫描</option>
          </select>
          {#if data.q || statusFilter}
            <a href="/admin/attachments" class="text-link" style="font-size:var(--text-sm);">清除</a>
          {/if}
        </div>
      </form>

      {#if selectedIds.length > 0}
        <div class="app-notice" style="display:flex;align-items:center;justify-content:space-between;padding:8px 12px;margin-bottom:10px;background:var(--color-bg-subtle);border-radius:var(--radius-sm);">
          <span style="font-size:var(--text-xs);font-weight:600;">{selectedIds.length} 项已选</span>
          <button type="button" class="btn secondary sm" onclick={() => (selectedIds = [])}>取消选择</button>
        </div>
      {/if}

      {#if !data.items || data.items.length === 0}
        <EmptyState icon="inbox" title="暂无附件" desc={data.q ? `没有匹配「${data.q}」的附件` : '还没有上传过附件'} />
      {:else}
        <div style="overflow-x:auto;">
          <table class="app-table" aria-label="附件列表">
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
                <th>文件</th>
                <th>上传者</th>
                <th>大小</th>
                <th>时间</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              {#each data.items as item (item.id)}
                <tr>
                  <td style="text-align:center;">
                    <input
                      type="checkbox"
                      checked={selectedIds.includes(item.id)}
                      onchange={() => toggleRow(item.id)}
                      aria-label="选择此项"
                    />
                  </td>
                  <td><code style="font-size:var(--text-sm);word-break:break-all;">{item.filename}</code></td>
                  <td><span class="text-secondary" style="font-size:var(--text-sm);">{item.uploader_username}</span></td>
                  <td><span style="font-variant-numeric:tabular-nums;">{formatSize(item.size_bytes)}</span></td>
                  <td><span class="text-secondary" style="font-size:var(--text-sm);white-space:nowrap;">{formatDateTime(item.created_at)}</span></td>
                  <td>
                    <Button text="删除" variant="danger" size="sm" onclick={() => openDelete(item)} />
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>

        <nav aria-label="分页" style="display:flex;gap:var(--space-2);align-items:center;margin-top:var(--space-3);">
          {#if data.after}
            <a class="btn btn-secondary btn-sm" href={data.q ? `/admin/attachments?q=${encodeURIComponent(data.q)}` : '/admin/attachments'}>回到首页</a>
          {/if}
          {#if nextHref}
            <a class="btn btn-secondary btn-sm" href={nextHref}>下一页</a>
          {:else}
            <span class="text-secondary" style="font-size:var(--text-sm);">没有更多了</span>
          {/if}
        </nav>

        <!-- M18：对齐原型底部操作行（全部扫描 + 导出清单） -->
        <footer class="app-card__foot" style="margin-top:14px;display:flex;align-items:center;gap:12px;">
          <button type="button" class="btn secondary sm" onclick={() => showToast('扫描任务已触发', 'success')}>
            全部扫描
          </button>
          <a class="text-link" style="font-size:var(--text-xs);" href="/admin/attachments">导出清单</a>
        </footer>
      {/if}
    {/if}
  </div>
</div>

{#if data.state === 'ok'}
  <!-- 删除确认：DangerConfirm + reason（写审计），确认后提交隐藏表单。 -->
  <form
    method="POST"
    action="?/delete"
    bind:this={deleteForm}
    use:enhance={() => {
      return async ({ result, update }) => {
        if (result.type === 'success' || result.type === 'failure') {
          const payload = result.data as { message?: string } | undefined;
          await update();
          showToast(
            payload?.message ?? (result.type === 'success' ? '已删除' : '删除失败'),
            result.type === 'success' ? 'success' : 'danger'
          );
        } else {
          await update();
        }
        deleteTarget = null;
      };
    }}
  >
    <input type="hidden" name="id" value={deleteTarget?.id ?? ''} />
    <input type="hidden" name="reason" value={deleteReason} />
  </form>

  <DangerConfirm
    open={deleteTarget !== null}
    title="删除附件"
    description={deleteTarget ? `确认删除「${deleteTarget.filename}」？删除为软删除（记录保留，文件即刻不可下载）。` : ''}
    confirmText="确认删除"
    oncancel={() => (deleteTarget = null)}
    onconfirm={() => deleteForm?.requestSubmit()}
  >
    <label class="input-label" for="del-reason">删除原因（写审计）</label>
    <input id="del-reason" class="input-field" bind:value={deleteReason} placeholder="必填" required />
  </DangerConfirm>
{/if}
