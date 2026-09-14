<script lang="ts">
  // 我的附件页（需登录）：可用附件空间（QuotaDisplay）+ 上传入口
  // （AttachmentUploader）+ 本人附件列表（预览/元数据/删除）。
  //
  // 数据：load 服务端直连 GET /api/v1/attachments（扩展端点，含 quota 摘要）；
  // 上传完成 / 删除成功后 invalidateAll() 重新拉取列表与容量（SSR 直连，
  // 不经浏览器 fetch，避免 CSRF/会话双通道）。
  import { enhance } from '$app/forms';
  import { invalidateAll } from '$app/navigation';
  import type { SubmitFunction } from '@sveltejs/kit';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Icon from '$lib/components/ui/Icon.svelte';
  import Seo from '$lib/components/Seo.svelte';
  import { show as showToast } from '$lib/ui/toast';
  import { attachmentContentUrl } from '$lib/api/client';
  import { formatBytes } from '$lib/components/upload/formatBytes';
  import AttachmentUploader from '$lib/components/upload/AttachmentUploader.svelte';
  import QuotaDisplay from '$lib/components/upload/QuotaDisplay.svelte';
  import type { Attachment } from '$lib/api/client';
  import type { MeAttachmentsActionData, MeAttachmentsPageData } from './+page.server';

  let { data, form }: { data: MeAttachmentsPageData; form?: MeAttachmentsActionData | null } =
    $props();

  const items = $derived(data.items);
  const quota = $derived(data.quota);

  // action 返回后 toast 反馈（成功/失败均提示），并刷新列表与容量。
  $effect(() => {
    if (!form) return;
    if (form.messageKind === 'success') {
      showToast(form.message ?? '操作成功', 'success');
    } else if (form.message) {
      showToast(form.message, 'danger');
    }
  });

  /** 附件状态中文标签与徽章配色。 */
  function statusLabel(status: Attachment['status']): string {
    switch (status) {
      case 'ready':
        return '已就绪';
      case 'processing':
        return '处理中';
      case 'pending':
        return '等待上传';
      case 'quarantined':
        return '已隔离';
      case 'deleted':
        return '保留期中';
      default:
        return status;
    }
  }

  function statusBadgeClass(status: Attachment['status']): string {
    if (status === 'ready') return 'badge-success';
    if (status === 'quarantined' || status === 'deleted') return 'badge-danger';
    return 'badge-neutral';
  }

  function formatTs(ms: number): string {
    return new Date(ms).toLocaleString('zh-CN', { hour12: false });
  }

  function isImage(a: Attachment): boolean {
    return a.media_type.startsWith('image/');
  }

  /** 删除表单增强：成功时刷新 load 数据（列表 + 容量），失败时保留输入。 */
  const deleteEnhance: SubmitFunction = () => async ({ result, update }) => {
    if (result.type === 'success') await invalidateAll();
    await update({ reset: false });
  };
</script>

<Seo
  title="我的附件"
  description="查看可用附件空间与已上传附件，上传与删除文件"
  og={{ type: 'website' }}
/>

<div class="container page-content" id="page-attachments">
  <header style="margin-bottom:var(--space-4);">
    <h1 style="display:flex;align-items:center;gap:var(--space-2);margin:0 0 var(--space-2);">
      <Icon name="paperclip" size={24} />
      我的附件
    </h1>
    <p style="margin:0;color:var(--color-text-secondary);font-size:var(--text-sm);">
      可用空间按账号等级策略实时计算；删除仅进入保留期，到期后物理清理并释放空间。
    </p>
  </header>

  {#if data.error}
    <div class="app-notice" role="alert" style="margin-bottom:var(--space-4);">
      附件信息加载失败：{data.error}
    </div>
  {/if}

  <section class="app-card" style="margin-bottom:var(--space-4);" aria-label="可用附件空间">
    <div class="app-card__body">
      <QuotaDisplay {quota} loading={false} error={data.error ? '容量信息暂不可用' : ''} />
    </div>
  </section>

  <section class="app-card" style="margin-bottom:var(--space-4);" aria-label="上传附件">
    <div class="app-card__body">
      <AttachmentUploader
        fetchFn={fetch}
        showQuota={false}
        label="选择要上传的文件"
        onReady={() => invalidateAll()}
      />
    </div>
  </section>

  <section class="app-card" aria-label="已上传附件">
    <header class="app-card__head">
      <h2 style="margin:0;font-size:var(--text-base);">已上传附件（{items.length}）</h2>
    </header>
    <div class="app-card__body">
      {#if items.length === 0}
        <EmptyState
          icon="paperclip"
          title="还没有附件"
          desc="上传第一个文件后，它会出现在这里。"
        />
      {:else}
        <div class="app-table-wrap">
          <table class="app-table" aria-label="附件列表">
            <thead>
              <tr>
                <th style="width:64px;">预览</th>
                <th>文件名</th>
                <th style="width:110px;">大小</th>
                <th style="width:110px;">状态</th>
                <th style="width:170px;">上传时间</th>
                <th style="width:90px;">引用</th>
                <th style="width:90px;">操作</th>
              </tr>
            </thead>
            <tbody>
              {#each items as attachment (attachment.id)}
                <tr>
                  <td>
                    {#if isImage(attachment)}
                      <img
                        class="att-thumb"
                        src={attachmentContentUrl(attachment.id)}
                        alt={attachment.original_name ?? '附件预览'}
                        loading="lazy"
                      />
                    {:else}
                      <span class="att-thumb att-thumb--file"><Icon name="file-text" size={18} /></span>
                    {/if}
                  </td>
                  <td>
                    <a
                      class="text-link"
                      href={attachmentContentUrl(attachment.id)}
                      target="_blank"
                      rel="noopener noreferrer"
                    >
                      {attachment.original_name ?? '未命名附件'}
                    </a>
                    <div style="font-size:var(--text-xs);color:var(--color-text-secondary);">
                      {attachment.media_type}
                    </div>
                  </td>
                  <td>{formatBytes(attachment.size_bytes)}</td>
                  <td>
                    <span class="badge {statusBadgeClass(attachment.status)}" style="font-size:11px;">
                      {statusLabel(attachment.status)}
                    </span>
                  </td>
                  <td style="font-size:var(--text-xs);color:var(--color-text-secondary);">
                    {formatTs(attachment.created_at)}
                  </td>
                  <td style="font-size:var(--text-xs);">{attachment.ref_count ?? 0} 处</td>
                  <td>
                    <form
                      method="POST"
                      action="?/remove"
                      use:enhance={deleteEnhance}
                      style="display:inline;"
                    >
                      <input type="hidden" name="id" value={attachment.id} />
                      <button type="submit" class="btn ghost sm">删除</button>
                    </form>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>
  </section>
</div>

<style>
  .att-thumb {
    width: 48px;
    height: 48px;
    object-fit: cover;
    border-radius: var(--radius-sm, 6px);
    border: 1px solid var(--color-border, #d0d7de);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: var(--color-bg-subtle, #f6f8fa);
    color: var(--color-text-secondary);
  }
</style>
