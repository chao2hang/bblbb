<!-- M06-UI-03：附件选择器——Cover/头像/封面引用只能选择本人 ready 附件。
  - 数据：GET /attachments（本人列表），前端过滤 status=ready && owner=self。
  - 预览：稳定内容端点 /attachments/{id}/content（后端鉴权后流式或 302 短期
    签名 URL）。S3 URL 过期只是临时链接失效（M06-UI-04）：图片 onerror 时
    重新请求 content 端点（缓存剔除）换取新跳转，不删除附件、不缓存旧 URL。
  - 键盘：radiogroup + radio 语义；无选中时提交禁用。
  - 安全：不渲染任何签名 URL 进 DOM 属性（只用相对稳定端点）。
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import {
    attachmentContentUrl,
    listMyAttachments,
    type Attachment
  } from '$lib/api/client';
  import { formatBytes } from './formatBytes';

  let {
    fetchFn = fetch,
    selectedId = null,
    accept = '',
    onSelect
  }: {
    fetchFn?: typeof fetch;
    selectedId?: string | null;
    /** MIME 过滤（可选）。 */
    accept?: string;
    onSelect?: (attachment: Attachment) => void;
  } = $props();

  let attachments = $state<Attachment[]>([]);
  let loading = $state(true);
  let error = $state('');
  /** 预览重载计数器（图片失效时剔除缓存重新请求 content 端点）。 */
  let bust = $state(0);

  onMount(async () => {
    try {
      const result = await listMyAttachments(fetchFn);
      // 本人附件端点语义上即 owner=self（后端裁决），投影仅需 status=ready
      // 过滤；不在此处再做 owner 匹配（端点保证）。
      attachments = result.items.filter((a) => a.status === 'ready');
      if (accept) {
        const types = accept.split(',').map((t) => t.trim().toLowerCase());
        attachments = attachments.filter((a) =>
          types.some((t) => {
            if (t.endsWith('/*')) return a.media_type.toLowerCase().startsWith(t.slice(0, -1));
            return a.media_type.toLowerCase() === t;
          })
        );
      }
    } catch {
      error = '附件列表暂不可用';
    } finally {
      loading = false;
    }
  });

  function pick(a: Attachment) {
    onSelect?.(a);
  }

  function isSelected(a: Attachment): boolean {
    return selectedId !== null && a.id === selectedId;
  }

  function refreshPreview(e: Event) {
    // 签名 URL 已过期：重打 content 端点（Cache-Control 剔除），不删除附件。
    (e.target as HTMLImageElement).style.visibility = 'hidden';
    bust = bust + 1;
  }
</script>

<div class="picker">
  {#if loading}
    <p class="input-hint" role="status">加载附件…</p>
  {:else if error}
    <p class="input-hint is-error" role="alert">{error}</p>
  {:else if attachments.length === 0}
    <p class="input-hint">还没有可用的附件，请先上传。</p>
  {:else}
    <div class="picker-list" role="radiogroup" aria-label="选择本人已就绪附件">
      {#each attachments as attachment (attachment.id)}
        {@const contentUrl = attachmentContentUrl(attachment.id)}
        <label
          class="picker-item {isSelected(attachment) ? 'is-selected' : ''}"
        >
          <input
            type="radio"
            name="attachment-pick"
            class="picker-radio"
            value={attachment.id}
            checked={isSelected(attachment)}
            onchange={() => pick(attachment)}
          />
          {#if attachment.media_type.startsWith('image/')}
            <img
              class="picker-thumb"
              src="{contentUrl}?v={bust}"
              alt=""
              loading="lazy"
              onerror={refreshPreview}
            />
          {:else}
            <span class="picker-thumb picker-thumb-file" aria-hidden="true">📄</span>
          {/if}
          <span class="picker-meta">
            <span class="picker-name">{attachment.original_name ?? '附件'}</span>
            <span class="picker-sub">{formatBytes(attachment.size_bytes)} · {attachment.media_type}</span>
          </span>
        </label>
      {/each}
    </div>
  {/if}
</div>

<style>
  .picker-list {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: var(--space-2);
    margin-top: var(--space-2);
  }
  .picker-item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2);
    border: 1px solid var(--color-border, #d0d7de);
    border-radius: var(--radius-sm, 6px);
    cursor: pointer;
  }
  .picker-item.is-selected {
    border-color: var(--color-primary, #0969da);
    box-shadow: 0 0 0 1px var(--color-primary, #0969da);
  }
  .picker-radio {
    position: absolute;
    width: 1px;
    height: 1px;
    opacity: 0;
  }
  .picker-thumb {
    width: 48px;
    height: 48px;
    object-fit: cover;
    border-radius: 4px;
    background: var(--color-bg-subtle, #f6f8fa);
  }
  .picker-thumb-file {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 20px;
  }
  .picker-meta {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .picker-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 500;
    font-size: var(--text-sm, 14px);
  }
  .picker-sub {
    font-size: var(--text-xs, 12px);
    color: var(--color-text-secondary, #666);
  }
</style>
