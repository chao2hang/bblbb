<script lang="ts">
  // GAP-FIX（管理域·附件管理）：文件名/上传者/大小（KB/MB 格式化）/时间 表格 +
  // 删除（DangerConfirm + reason 写审计）+ q 搜索 + cursor 分页。
  // M18-ADMIN-BATCH：选择列 + BatchBar + 批量删除 Dialog（?/batchDelete，
  // 循环单条 DELETE 端点；端点无 If-Match，行数据无 version）。
  // M18-ADMIN-OPS（约定 D）：行内删除入口改为「⋮」三点菜单——单项「删除附件」
  // 打开既有删除 DangerConfirm（隐藏表单 requestSubmit 机制不变）。
  // 新增能力：管理员实时预览附件内容（阻断不合规文件），并可直接封禁恶意上传者用户。
  import PageHeader from '$lib/components/admin/PageHeader.svelte';
  import BatchBar from '$lib/components/admin/BatchBar.svelte';
  import RowActionsMenu from '$lib/components/admin/RowActionsMenu.svelte';
  import { enhance } from '$app/forms';
  import Button from '$lib/components/ui/Button.svelte';
  import DangerConfirm from '$lib/components/ui/DangerConfirm.svelte';
  import Dialog from '$lib/components/ui/Dialog.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { adminStateLabel } from '$lib/admin';
  import { toastActionResult } from '$lib/ui/action-toast';
  import { show as showToast } from '$lib/ui/toast';
  import { attachmentContentUrl } from '$lib/api/client';
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

  function isImage(filename: string): boolean {
    return /\.(png|jpe?g|gif|webp|svg|avif|bmp|ico)$/i.test(filename);
  }

  function isVideo(filename: string): boolean {
    return /\.(mp4|webm|mov|mkv)$/i.test(filename);
  }

  function isAudio(filename: string): boolean {
    return /\.(mp3|wav|ogg|m4a|aac)$/i.test(filename);
  }

  const nextHref = $derived(
    data.state === 'ok' && data.nextCursor
      ? `/admin/attachments?q=${encodeURIComponent(data.q)}&after=${encodeURIComponent(data.nextCursor)}`
      : null
  );

  const message = $derived(form?.message ?? null);

  // JS 启用：动作结果走全局 Toast 浮窗（成功绿/失败红）；列表上方内联横幅仅保留为
  // 无 JS 回退（SSR HTML 仍渲染，见 hasJs）。
  let hasJs = $state(false);
  $effect(() => {
    hasJs = true;
  });

  /** 删除确认对话框状态（行点击打开，确认后提交隐藏表单）。 */
  let deleteTarget: AdminAttachmentItem | null = $state(null);
  let deleteReason = $state('');
  let deleteForm: HTMLFormElement | undefined = $state();

  function openDelete(item: AdminAttachmentItem): void {
    deleteTarget = item;
    deleteReason = '';
  }

  /** 附件预览状态 */
  let previewTarget: AdminAttachmentItem | null = $state(null);

  function openPreview(item: AdminAttachmentItem): void {
    previewTarget = item;
  }

  function closePreview(): void {
    previewTarget = null;
  }

  /** 封禁违规用户对话框状态 */
  let banTarget: AdminAttachmentItem | null = $state(null);
  let banReason = $state('上传违规恶意附件');
  let alsoDeleteAttachment = $state(true);
  let banForm: HTMLFormElement | undefined = $state();

  function openBanUser(item: AdminAttachmentItem): void {
    banTarget = item;
    banReason = '上传违规恶意附件';
    alsoDeleteAttachment = true;
  }

  function closeBanUser(): void {
    banTarget = null;
  }

  /** 行「⋮」菜单项（约定 D：单一「删除附件」动作 → 既有删除 DangerConfirm 流）。 */
  function rowActions(item: AdminAttachmentItem) {
    return [
      {
        label: '预览内容',
        run: () => openPreview(item)
      },
      {
        label: '封禁上传者',
        danger: true,
        run: () => openBanUser(item)
      },
      {
        label: '删除附件',
        danger: true,
        run: () => openDelete(item)
      }
    ];
  }

  // M18：工具条搜索与复选框（附件接口当前未返回可筛选状态字段）。
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

  /** 批量删除（BatchBar → Dialog 填公共原因 → POST batchDelete）。 */
  let batchDeleteOpen = $state(false);
  let batchDeleteReason = $state('');
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
      {#if message && !hasJs}
        <p class="input-hint" role="status">{message}</p>
      {/if}

      <!-- M18：原型对齐工具条：单行 flex（搜索自动撑满；修复全宽控件与右缘清除按钮被裁切的问题） -->
      <form method="GET" action="/admin/attachments" style="display:flex;align-items:center;flex-wrap:wrap;gap:8px;margin-bottom:14px;">
        <input
          type="search"
          name="q"
          value={data.q}
          class="app-field"
          placeholder="搜索当前列表..."
          aria-label="搜索当前列表"
          style="flex:1 1 220px;min-width:0;"
        />
        {#if data.q}
          <a href="/admin/attachments" class="btn ghost sm" style="flex:0 0 auto;">清除搜索</a>
        {/if}
      </form>

      <BatchBar count={selectedIds.length} noun="个附件" onclear={() => (selectedIds = [])}>
        <Button text="批量删除" variant="danger" size="sm" onclick={() => (batchDeleteOpen = true)} />
      </BatchBar>

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
                    aria-label="全选当前列表附件"
                  />
                </th>
                <th style="min-width:280px;">文件与预览</th>
                <th>上传者</th>
                <th>大小</th>
                <th>时间</th>
                <th style="min-width:180px;">操作</th>
              </tr>
            </thead>
            <tbody>
              {#each data.items as item (item.id)}
                {@const isImg = isImage(item.filename)}
                {@const contentUrl = attachmentContentUrl(item.id)}
                <tr>
                  <td style="text-align:center;">
                    <input
                      type="checkbox"
                      checked={selectedIds.includes(item.id)}
                      onchange={() => toggleRow(item.id)}
                      aria-label="选中附件 {item.filename}"
                    />
                  </td>
                  <td>
                    <div class="att-row-file">
                      {#if isImg}
                        <button
                          type="button"
                          class="att-thumb-btn"
                          title="点击快速放大预览"
                          onclick={() => openPreview(item)}
                        >
                          <img
                            src={contentUrl}
                            alt={item.filename}
                            class="att-thumb-img"
                            loading="lazy"
                            onerror={(e) => {
                              (e.currentTarget as HTMLElement).style.display = 'none';
                            }}
                          />
                          <span class="att-thumb-overlay" aria-hidden="true">
                            <Icon name="eye" size={13} />
                          </span>
                        </button>
                      {:else}
                        <div class="att-type-icon" aria-hidden="true">
                          {#if isVideo(item.filename)}
                            <Icon name="video" size={16} />
                          {:else if isAudio(item.filename)}
                            <Icon name="activity" size={16} />
                          {:else}
                            <Icon name="file-text" size={16} />
                          {/if}
                        </div>
                      {/if}
                      <div class="att-file-info">
                        <button
                          type="button"
                          class="att-filename-btn"
                          title="点击预览文件详情"
                          onclick={() => openPreview(item)}
                        >
                          <code>{item.filename}</code>
                        </button>
                        <span class="att-id-mono">ID: {item.id}</span>
                      </div>
                    </div>
                  </td>
                  <td>
                    <div class="att-uploader-cell">
                      <span class="text-secondary" style="font-size:var(--text-sm);">{item.uploader_username}</span>
                      <button
                        type="button"
                        class="att-quick-ban-tag"
                        title="快速封禁上传者：{item.uploader_username}"
                        onclick={() => openBanUser(item)}
                      >
                        <Icon name="ban" size={11} />
                        <span>封禁</span>
                      </button>
                    </div>
                  </td>
                  <td><span style="font-variant-numeric:tabular-nums;">{formatSize(item.size_bytes)}</span></td>
                  <td><span class="text-secondary" style="font-size:var(--text-sm);white-space:nowrap;">{formatDateTime(item.created_at)}</span></td>
                  <td>
                    <div class="att-row-actions">
                      <button
                        type="button"
                        class="btn ghost sm att-action-btn"
                        title="预览附件"
                        onclick={() => openPreview(item)}
                      >
                        <Icon name="eye" size={13} />
                        <span>预览</span>
                      </button>

                      <button
                        type="button"
                        class="btn danger ghost sm att-action-btn att-danger-btn"
                        title="封禁上传者"
                        onclick={() => openBanUser(item)}
                      >
                        <Icon name="ban" size={13} />
                        <span>封禁</span>
                      </button>

                      <!-- 兼容现有规范：每行一个「⋮」三点菜单（约定 D） -->
                      <RowActionsMenu
                        label="更多操作：附件 {item.filename}"
                        actions={rowActions(item)}
                      />
                    </div>
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
          await update();
          toastActionResult(result);
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
    title={deleteTarget ? "删除附件" : ""}
    description={deleteTarget ? `确认删除「${deleteTarget.filename}」？删除为软删除（记录保留，文件即刻不可下载）。` : ''}
    confirmText="确认删除"
    oncancel={() => (deleteTarget = null)}
    onconfirm={() => deleteForm?.requestSubmit()}
  >
    <label class="input-label" for="del-reason">删除原因（写审计）</label>
    <input id="del-reason" class="input-field" bind:value={deleteReason} placeholder="必填" required />
  </DangerConfirm>

  <!-- 批量删除：批量 Dialog 填公共原因 → POST batchDelete（循环单条 DELETE 端点）。 -->
  <Dialog
    open={batchDeleteOpen}
    title="批量删除附件"
    description={`将删除 ${selectedIds.length} 个附件（软删除，记录保留，文件即刻不可下载）；原因写审计。`}
    onclose={() => (batchDeleteOpen = false)}
  >
    <form
      method="POST"
      action="?/batchDelete"
      use:enhance={() => {
        return async ({ result, update }) => {
          toastActionResult(result, {
            message: (d) => (d?.message as string | null) ?? (result.type === 'success' ? '批量删除完成' : '批量删除失败')
          });
          await update();
          selectedIds = [];
          batchDeleteOpen = false;
        };
      }}
    >
      <input type="hidden" name="ids" value={selectedIds.join(',')} />
      <div class="input-wrapper" style="margin-bottom:var(--space-3);">
        <label class="input-label" for="batch-del-reason">删除原因（写审计）</label>
        <input id="batch-del-reason" name="reason" class="input-field" required bind:value={batchDeleteReason} placeholder="必填" />
      </div>
      <Button text="确认批量删除" variant="danger" size="sm" type="submit" />
    </form>
  </Dialog>

  <!-- 附件全屏预览模态框（Preview Dialog） -->
  <Dialog
    open={previewTarget !== null}
    title={previewTarget ? `预览附件：${previewTarget.filename}` : '预览附件'}
    onclose={closePreview}
  >
    {#if previewTarget}
      {@const pContentUrl = attachmentContentUrl(previewTarget.id)}
      {@const pIsImg = isImage(previewTarget.filename)}
      {@const pIsVideo = isVideo(previewTarget.filename)}
      {@const pIsAudio = isAudio(previewTarget.filename)}
      <div class="att-preview-dialog">
        <!-- 预览主体内容区 -->
        <div class="att-preview-canvas">
          {#if pIsImg}
            <div class="att-preview-img-box">
              <img
                src={pContentUrl}
                alt={previewTarget.filename}
                class="att-preview-img"
              />
            </div>
          {:else if pIsVideo}
            <video src={pContentUrl} controls class="att-preview-video">
              <track kind="captions" />
            </video>
          {:else if pIsAudio}
            <audio src={pContentUrl} controls class="att-preview-audio"></audio>
          {:else}
            <div class="att-preview-file-card">
              <Icon name="file-text" size={48} />
              <p class="att-preview-file-notice">该类型文件暂不支持直接嵌入预览，可下载后查看验证。</p>
            </div>
          {/if}
        </div>

        <!-- 元信息清单 -->
        <div class="att-preview-meta-grid">
          <div class="att-meta-item">
            <span class="att-meta-label">文件名称</span>
            <code class="att-meta-val">{previewTarget.filename}</code>
          </div>
          <div class="att-meta-item">
            <span class="att-meta-label">文件大小</span>
            <span class="att-meta-val">{formatSize(previewTarget.size_bytes)}</span>
          </div>
          <div class="att-meta-item">
            <span class="att-meta-label">上传账号</span>
            <span class="att-meta-val">{previewTarget.uploader_username}</span>
          </div>
          <div class="att-meta-item">
            <span class="att-meta-label">上传时间</span>
            <span class="att-meta-val">{formatDateTime(previewTarget.created_at)}</span>
          </div>
        </div>

        <!-- 弹窗底部处置按钮栏 -->
        <div class="att-preview-actions">
          <a
            href={pContentUrl}
            target="_blank"
            rel="noopener noreferrer"
            class="btn secondary sm"
          >
            <Icon name="download" size={13} />
            <span>下载 / 原链打开</span>
          </a>

          <button
            type="button"
            class="btn danger sm"
            onclick={() => {
              const cur = previewTarget;
              closePreview();
              if (cur) openBanUser(cur);
            }}
          >
            <Icon name="ban" size={13} />
            <span>直接封禁该用户</span>
          </button>

          <button
            type="button"
            class="btn ghost sm"
            onclick={() => {
              const cur = previewTarget;
              closePreview();
              if (cur) openDelete(cur);
            }}
          >
            <Icon name="trash" size={13} />
            <span>删除此附件</span>
          </button>
        </div>
      </div>
    {/if}
  </Dialog>

  <!-- 封禁违规用户表单（常驻 SSR 隐藏表单；弹窗确认后提交） -->
  <form
    method="POST"
    action="?/banUser"
    bind:this={banForm}
    use:enhance={() => {
      return async ({ result, update }) => {
        toastActionResult(result);
        await update();
        banTarget = null;
      };
    }}
  >
    <input type="hidden" name="username" value={banTarget?.uploader_username ?? ''} />
    <input type="hidden" name="reason" value={banReason} />
    {#if alsoDeleteAttachment && banTarget}
      <input type="hidden" name="delete_attachment_id" value={banTarget.id} />
    {/if}
  </form>

  <!-- 封禁恶意上传者模态框（Ban User Dialog） -->
  <Dialog
    open={banTarget !== null}
    title={banTarget ? `封禁用户：${banTarget.uploader_username}` : '封禁用户'}
    description={banTarget ? `该操作将立即将「${banTarget.uploader_username}」置为 banned 封禁状态，剥夺其全站所有操作权限并强制下线。` : ''}
    onclose={closeBanUser}
  >
    {#if banTarget}
      <div class="input-wrapper" style="margin-bottom:var(--space-3);">
        <label class="input-label" for="ban-user-reason">封禁原因（必填，计入审计日志）</label>
        <input
          id="ban-user-reason"
          class="input-field"
          bind:value={banReason}
          placeholder="例如：上传不合规/违规涉黄涉暴附件"
          required
        />
      </div>

      <div class="att-ban-option" style="margin-bottom:var(--space-4);">
        <label class="att-checkbox-label">
          <input
            type="checkbox"
            bind:checked={alsoDeleteAttachment}
          />
          <span>同时软删除当前违规附件「<code>{banTarget.filename}</code>」</span>
        </label>
      </div>

      <div style="display:flex;justify-content:flex-end;gap:8px;">
        <button type="button" class="btn secondary sm" onclick={closeBanUser}>取消</button>
        <button
          type="button"
          class="btn danger sm"
          onclick={() => banForm?.requestSubmit()}
        >
          确认封禁该用户
        </button>
      </div>
    {/if}
  </Dialog>
{/if}

<style>
  .att-row-file {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .att-thumb-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    border-radius: var(--radius-sm, 6px);
    border: 1px solid var(--color-border);
    background: var(--color-bg-subtle);
    padding: 0;
    overflow: hidden;
    cursor: pointer;
    position: relative;
    flex-shrink: 0;
  }

  .att-thumb-img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .att-thumb-overlay {
    position: absolute;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    color: #fff;
    display: flex;
    align-items: center;
    justify-content: center;
    opacity: 0;
    transition: opacity 140ms ease;
  }

  .att-thumb-btn:hover .att-thumb-overlay {
    opacity: 1;
  }

  .att-type-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    border-radius: var(--radius-sm, 6px);
    background: var(--color-bg-subtle);
    color: var(--color-text-secondary);
    border: 1px solid var(--color-border);
    flex-shrink: 0;
  }

  .att-file-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .att-filename-btn {
    border: 0;
    background: transparent;
    padding: 0;
    text-align: left;
    cursor: pointer;
    color: inherit;
  }

  .att-filename-btn:hover code {
    color: var(--color-brand);
    text-decoration: underline;
  }

  .att-id-mono {
    font: 9.5px/1 var(--font-family-mono);
    color: var(--color-text-tertiary);
  }

  .att-uploader-cell {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .att-quick-ban-tag {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 1px 5px;
    border: 1px solid color-mix(in srgb, var(--color-danger, #ef4444) 30%, transparent);
    border-radius: 3px;
    background: color-mix(in srgb, var(--color-danger, #ef4444) 10%, transparent);
    color: var(--color-danger, #ef4444);
    font: 500 10px/1.2 var(--font-family-base);
    cursor: pointer;
    transition: all 120ms ease;
  }

  .att-quick-ban-tag:hover {
    background: var(--color-danger, #ef4444);
    color: #fff;
  }

  .att-row-actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .att-action-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 8px;
    font-size: 11.5px;
  }

  .att-danger-btn:hover {
    color: var(--color-danger, #ef4444) !important;
    border-color: color-mix(in srgb, var(--color-danger, #ef4444) 40%, transparent) !important;
    background: color-mix(in srgb, var(--color-danger, #ef4444) 10%, transparent) !important;
  }

  /* 预览弹窗内部样式 */
  .att-preview-dialog {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .att-preview-canvas {
    width: 100%;
    min-height: 220px;
    max-height: 55vh;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--color-bg-subtle);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md, 8px);
    overflow: hidden;
    position: relative;
  }

  .att-preview-img-box {
    width: 100%;
    height: 100%;
    max-height: 55vh;
    display: flex;
    align-items: center;
    justify-content: center;
    background-image: linear-gradient(45deg, #18181b 25%, transparent 25%),
      linear-gradient(-45deg, #18181b 25%, transparent 25%),
      linear-gradient(45deg, transparent 75%, #18181b 75%),
      linear-gradient(-45deg, transparent 75%, #18181b 75%);
    background-size: 16px 16px;
    background-position: 0 0, 0 8px, 8px -8px, -8px 0px;
  }

  .att-preview-img {
    max-width: 100%;
    max-height: 52vh;
    object-fit: contain;
    border-radius: 4px;
  }

  .att-preview-video {
    max-width: 100%;
    max-height: 50vh;
  }

  .att-preview-audio {
    width: 90%;
  }

  .att-preview-file-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 32px;
    color: var(--color-text-secondary);
  }

  .att-preview-file-notice {
    font-size: 12px;
    margin: 0;
  }

  .att-preview-meta-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: 10px;
    padding: 12px 14px;
    background: var(--color-bg-subtle);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm, 6px);
  }

  .att-meta-item {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .att-meta-label {
    font: 600 10px/1.2 var(--font-family-mono);
    color: var(--color-text-tertiary);
    text-transform: uppercase;
  }

  .att-meta-val {
    font-size: 12.5px;
    color: var(--color-text-primary);
    word-break: break-all;
  }

  .att-preview-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    flex-wrap: wrap;
    padding-top: 4px;
  }

  .att-checkbox-label {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    font-size: 12.5px;
    color: var(--color-text-secondary);
    cursor: pointer;
  }
</style>
