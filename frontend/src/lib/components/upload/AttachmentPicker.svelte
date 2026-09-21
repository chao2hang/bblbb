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
  import Icon from '$lib/components/ui/Icon.svelte';

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
  let isTimeout = $state(false);
  /** 单个附件重试次数记录（每个附件最多重试 1 次，防雪崩）。 */
  let retryMap = $state<Record<string, number>>({});
  /** 彻底加载失败的附件 ID 集合（不再渲染 img 标签，停止重复请求）。 */
  let failedIds = $state<Record<string, boolean>>({});

  async function loadAttachments() {
    loading = true;
    error = '';
    isTimeout = false;
    let timer: ReturnType<typeof setTimeout> | null = null;
    try {
      const timeoutPromise = new Promise<never>((_, reject) => {
        timer = setTimeout(() => {
          isTimeout = true;
          reject(new Error('TIMEOUT'));
        }, 8000);
      });
      const fetchPromise = listMyAttachments(fetchFn);
      const result = await Promise.race([fetchPromise, timeoutPromise]);
      if (timer) clearTimeout(timer);
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
    } catch (err: any) {
      if (timer) clearTimeout(timer);
      if (err?.message === 'TIMEOUT' || isTimeout) {
        error = '加载附件超时，请检查网络连接或稍后重试';
      } else {
        error = '附件服务暂不可用或个人存储容量未就绪';
      }
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    loadAttachments();
  });

  function pick(a: Attachment) {
    onSelect?.(a);
  }

  function isSelected(a: Attachment): boolean {
    return selectedId !== null && a.id === selectedId;
  }

  function handleImageError(id: string) {
    const currentRetries = retryMap[id] ?? 0;
    if (currentRetries < 1) {
      // 允许对该单个图片重试 1 次（换取新签名 URL）
      retryMap[id] = currentRetries + 1;
    } else {
      // 达到重试上限，标记为失败，停止发起请求
      failedIds[id] = true;
    }
  }

  function getImageUrl(id: string): string {
    const base = attachmentContentUrl(id);
    const retryCount = retryMap[id];
    return retryCount ? `${base}?r=${retryCount}` : base;
  }
</script>

<div class="picker">
  {#if loading}
    <div style="display:flex;align-items:center;gap:8px;padding:8px 0;">
      <p class="input-hint" role="status" style="margin:0;">加载附件…</p>
    </div>
  {:else if error}
    <div class="alert alert-danger" role="alert" style="padding:10px 14px;background:var(--color-bg-subtle);border-radius:var(--radius-sm);font-size:13px;display:flex;justify-content:space-between;align-items:center;margin-bottom:10px;">
      <div>
        <p class="input-hint is-error" style="margin:0;font-weight:600;">{error}</p>
        <span class="app-muted" style="font-size:11px;">存储后端或配额限制可能导致附件列表加载异常</span>
      </div>
      <button type="button" class="btn secondary sm" onclick={loadAttachments}>重试加载</button>
    </div>
  {:else if attachments.length === 0}
    <p class="input-hint">还没有可用的附件，请先上传。</p>
  {:else}
    <div class="picker-list" role="radiogroup" aria-label="选择本人已就绪附件">
      {#each attachments as attachment (attachment.id)}
        {@const contentUrl = attachmentContentUrl(attachment.id)}
        {@const isImage = attachment.media_type.startsWith('image/')}
        {@const isFailed = Boolean(failedIds[attachment.id])}
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
          {#if isImage && !isFailed}
            <img
              class="picker-thumb"
              src={getImageUrl(attachment.id)}
              alt=""
              loading="lazy"
              onerror={() => handleImageError(attachment.id)}
            />
          {:else if isImage && isFailed}
            <span class="picker-thumb picker-thumb-file" title="预览加载失败" aria-label="预览加载失败">
              <Icon name="image" size={20} />
            </span>
          {:else}
            <span class="picker-thumb picker-thumb-file" aria-hidden="true"><Icon name="file-text" size={20} /></span>
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
