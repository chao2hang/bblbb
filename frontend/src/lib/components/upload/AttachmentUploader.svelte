<!-- M06-UI-01..04：附件上传器
  - 两阶段上传：POST /attachments（创建 + presigned PUT 参数/本地直传）→
    浏览器直传（S3 presigned PUT 用 XHR 显示进度；本地走 create 内流式）→
    POST /attachments/{id}/complete（服务端 HEAD 校验 + 安全处理）。
  - 状态：idle → creating → uploading → completing → processing → done/error。
  - 取消：中断 XHR 并尽力 DELETE 服务端 pending 附件；重试：重建上传（签名
    URL 过期/403 时重新 create）。
  - 配额：创建响应携带 quota 摘要（字段缺失容忍），并集成 QuotaDisplay。
  - 无 JS：渲染文件输入 + 提示需启用 JS 完成直传（两阶段 S3 上传必须有 JS）。
  - 键盘：label[for] + 原生 <input type=file>，进度 role=progressbar，错误
    role=alert。
-->
<script lang="ts">
  import { onDestroy } from 'svelte';
  import {
    attachmentContentUrl,
    completeAttachment,
    createAttachment,
    deleteAttachment,
    getAttachment,
    listMyAttachments,
    newClientRequestId,
    type Attachment,
    type AttachmentQuota
  } from '$lib/api/client';
  import { problemMessage, type Problem } from '$lib/errors';
  import Button from '$lib/components/ui/Button.svelte';
  import QuotaDisplay from './QuotaDisplay.svelte';
  import { formatBytes, progressLabel } from './formatBytes';

  let {
    fetchFn = fetch,
    targetType = null,
    targetId = null,
    accept = 'image/*,.pdf,.zip,.txt',
    maxBytes = 0,
    label = '选择文件',
    waitReady = true,
    showQuota = true,
    onReady
  }: {
    fetchFn?: typeof fetch;
    targetType?: string | null;
    targetId?: string | null;
    accept?: string;
    /** 0 = 使用后端等级配额；>0 = 额外硬性限制。 */
    maxBytes?: number;
    label?: string;
    waitReady?: boolean;
    showQuota?: boolean;
    onReady?: (attachment: Attachment) => void;
  } = $props();

  type Phase = 'idle' | 'creating' | 'uploading' | 'completing' | 'processing' | 'done' | 'error';

  const inputId = `attachment-file-${Math.random().toString(36).slice(2, 9)}`;

  let phase = $state<Phase>('idle');
  /** -1 = 不确定进度（本地直传）；0-100 = 字节进度。 */
  let progress = $state(-1);
  let errorText = $state('');
  let file = $state<File | null>(null);
  let attachment: Attachment | null = $state(null);
  /** create 返回的待上传附件 id（取消时用于尽力清理服务端 pending 对象）。 */
  let pendingId = $state<string | null>(null);
  let quota = $state<AttachmentQuota | null>(null);
  let quotaLoading = $state(false);
  let quotaError = $state('');
  let uploadUrl = $state<string | null>(null);
  let uploadHeaders = $state<Record<string, string>>({});
  let xhr: XMLHttpRequest | null = null;
  let pollTimer: ReturnType<typeof setInterval> | null = null;

  const busy = $derived(phase === 'creating' || phase === 'uploading' || phase === 'completing' || phase === 'processing');

  const fileMax = $derived.by(() => {
    if (maxBytes > 0) return maxBytes;
    if (quota && typeof quota.max_file_bytes === 'number' && quota.max_file_bytes > 0) {
      return quota.max_file_bytes;
    }
    return 0;
  });

  onDestroy(() => {
    if (pollTimer) clearInterval(pollTimer);
    if (xhr) xhr.abort();
  });

  async function loadQuota() {
    quotaLoading = true;
    quotaError = '';
    try {
      const result = await listMyAttachments(fetchFn);
      quota = result.quota ?? null;
      if (!quota && result.items.length === 0) quota = null;
    } catch {
      quota = null;
      quotaError = '容量信息暂不可用';
    } finally {
      quotaLoading = false;
    }
  }

  // 需要显示容量时加载（showQuota 为 prop，仅初始化时生效）。
  $effect(() => {
    if (showQuota) loadQuota();
  });

  function validateFile(f: File): string | null {
    if (maxBytes > 0 && f.size > maxBytes) {
      return `文件超过 ${formatBytes(maxBytes)} 上限`;
    }
    if (accept) {
      const types = accept.split(',').map((t) => t.trim().toLowerCase());
      const matches = types.some((t) => {
        if (t.startsWith('.')) return f.name.toLowerCase().endsWith(t);
        if (t.endsWith('/*')) {
          const base = t.slice(0, -1);
          return f.type.toLowerCase().startsWith(base);
        }
        return f.type.toLowerCase() === t;
      });
      if (!matches) return '文件类型不被允许';
    }
    return null;
  }

  function onPick(e: Event) {
    const input = e.target as HTMLInputElement;
    const f = input.files?.[0] ?? null;
    if (!f) return;
    reset();
    const err = validateFile(f);
    if (err) {
      phase = 'error';
      errorText = err;
      return;
    }
    file = f;
  }

  function reset() {
    if (pollTimer) clearInterval(pollTimer);
    pollTimer = null;
    if (xhr) {
      xhr.abort();
      xhr = null;
    }
    phase = 'idle';
    progress = -1;
    errorText = '';
    attachment = null;
    pendingId = null;
    uploadUrl = null;
    uploadHeaders = {};
  }

  function fail(message: string) {
    phase = 'error';
    errorText = message;
  }

  function problemLabel(err: unknown): string {
    const problem = err as Problem;
    const message = problemMessage(problem);
    if (problem?.status === 413) return `文件过大：${message}`;
    if (problem?.status === 409) return `容量不足或状态冲突：${message}`;
    if (problem?.status === 429) return `操作过于频繁：${message}`;
    return message;
  }

  /** 上传主体：优先 presigned PUT（XHR 进度），否则视为本地流式直传。 */
  function uploadBytes(id: string): Promise<void> {
    if (!file) return Promise.reject(new Error('未选择文件'));
    const url = uploadUrl;
    if (!url) {
      // 本地直传：创建时后端已流式接收字节，直接进入 complete。
      progress = 100;
      return Promise.resolve();
    }
    return new Promise((resolve, reject) => {
      phase = 'uploading';
      const req = new XMLHttpRequest();
      xhr = req;
      req.open('PUT', url);
      for (const [k, v] of Object.entries(uploadHeaders)) {
        req.setRequestHeader(k, v);
      }
      req.upload.onprogress = (ev) => {
        if (ev.lengthComputable) {
          progress = Math.round((ev.loaded / ev.total) * 100);
        }
      };
      req.onload = () => {
        xhr = null;
        if (req.status >= 200 && req.status < 300) {
          progress = 100;
          resolve();
        } else if (req.status === 403 || req.status === 401) {
          // 签名 URL 过期/无效：不是附件问题，重新 create 换取新 URL。
          reject(new Error('upload_url_expired'));
        } else {
          reject(new Error(`upload_failed_${req.status}`));
        }
      };
      req.onerror = () => {
        xhr = null;
        reject(new Error('upload_network_error'));
      };
      req.onabort = () => {
        xhr = null;
        reject(new Error('upload_cancelled'));
      };
      req.send(file);
    });
  }

  /** 等待附件进入 ready（或 quarantined），最多 ~90s。 */
  function pollReady(id: string): Promise<Attachment> {
    return new Promise((resolve) => {
      let attempts = 0;
      pollTimer = setInterval(async () => {
        attempts += 1;
        let current: Attachment | null = null;
        try {
          current = await getAttachment(fetchFn, id);
        } catch {
          current = null;
        }
        if (current && (current.status === 'ready' || current.status === 'quarantined')) {
          if (pollTimer) clearInterval(pollTimer);
          pollTimer = null;
          resolve(current);
        } else if (attempts >= 45) {
          if (pollTimer) clearInterval(pollTimer);
          pollTimer = null;
          const last = current ?? attachment;
          if (last) resolve(last);
        }
      }, 2000);
    });
  }

  async function start() {
    if (!file || busy) return;
    errorText = '';
    phase = 'creating';
    try {
      const created = await createAttachment(fetchFn, {
        filename: file.name,
        size: file.size,
        declared_media_type: file.type || 'application/octet-stream',
        target_type: targetType,
        target_id: targetId
      });
      if (created.quota) quota = created.quota;
      const id = created.id;
      pendingId = id;
      const upload = created.upload ?? null;
      if (upload && upload.url) {
        uploadUrl = upload.url;
        uploadHeaders = upload.headers ?? {};
      } else {
        uploadUrl = null;
      }
      await uploadBytes(id);
      phase = 'completing';
      const completed = await completeAttachment(fetchFn, id, newClientRequestId());
      let finalAttachment = completed;
      if (waitReady && completed.status === 'processing') {
        phase = 'processing';
        finalAttachment = await pollReady(id);
      }
      if (!finalAttachment) finalAttachment = completed;
      if (finalAttachment.status === 'quarantined') {
        phase = 'error';
        errorText = '文件未通过安全校验，已被隔离';
        return;
      }
      attachment = finalAttachment;
      phase = 'done';
      onReady?.(finalAttachment);
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      if (message === 'upload_cancelled') {
        phase = 'idle';
        return;
      }
      if (message === 'upload_url_expired') {
        // 签名 URL 过期：重新 create 获取新 URL（M06-UI-04），不删除附件。
        errorText = '上传链接已过期，正在重新获取…';
        phase = 'idle'; // 清除 busy 态，允许 start() 重入
        uploadUrl = null;
        return start();
      }
      fail(problemLabel(err));
    }
  }

  async function cancel() {
    if (pollTimer) {
      clearInterval(pollTimer);
      pollTimer = null;
    }
    if (xhr) {
      xhr.abort();
      xhr = null;
    }
    const id = pendingId ?? attachment?.id ?? null;
    reset();
    if (id) {
      try {
        await deleteAttachment(fetchFn, id);
      } catch {
        // 尽力清理；失败不阻断用户。
      }
    }
    if (showQuota) loadQuota();
  }

  async function retry() {
    if (!file) return;
    // cancel 会 reset 状态但保留 file 选择；重新走完整两阶段流程。
    await cancel();
    await start();
  }

  function previewUrl(id: string): string {
    return attachmentContentUrl(id);
  }
</script>

<div class="uploader">
  {#if showQuota}
    <QuotaDisplay {quota} loading={quotaLoading} error={quotaError} />
  {/if}

  <div class="uploader-body">
    {#if phase !== 'done'}
      <label class="uploader-input-label" for={inputId}>
        {#if file}
          <span class="uploader-file">{file.name}（{formatBytes(file.size)}）</span>
        {:else}
          <span>{label}</span>
        {/if}
      </label>
      <input
        id={inputId}
        type="file"
        accept={accept}
        class="uploader-input"
        onchange={onPick}
        disabled={busy}
      />
    {/if}

    {#if phase === 'creating'}
      <p class="input-hint" role="status">正在创建上传…</p>
    {:else if phase === 'uploading' || phase === 'completing' || phase === 'processing'}
      <div
        class="uploader-progress"
        role="progressbar"
        aria-label="上传进度"
        aria-valuenow={progress >= 0 ? progress : undefined}
        aria-valuemin={0}
        aria-valuemax={100}
      >
        <div class="uploader-progress-fill" style="width:{progress >= 0 ? progress : 8}%;"></div>
      </div>
      <p class="input-hint" role="status">
        {#if phase === 'uploading'}
          {progressLabel(progress)}
        {:else if phase === 'completing'}
          正在提交并校验…
        {:else}
          附件安全处理中…
        {/if}
      </p>
    {:else if phase === 'error'}
      <p class="input-hint is-error" role="alert">{errorText}</p>
      <div class="uploader-actions">
        <Button text="重试" variant="primary" size="sm" type="button" onclick={retry} disabled={!file} />
        <Button text="取消" variant="ghost" size="sm" type="button" onclick={cancel} />
      </div>
    {:else if phase === 'idle' && file}
      <div class="uploader-actions">
        <Button text="开始上传" variant="primary" size="sm" type="button" onclick={start} />
        <Button text="取消选择" variant="ghost" size="sm" type="button" onclick={reset} />
      </div>
    {/if}

    {#if phase === 'done' && attachment}
      <p class="input-hint" role="status">上传完成：{attachment.original_name ?? '附件'} 已就绪</p>
      {#if attachment.media_type.startsWith('image/')}
        <img
          class="uploader-preview"
          src={previewUrl(attachment.id)}
          alt={attachment.original_name ?? '上传预览'}
        />
      {/if}
      <div class="uploader-actions">
        <Button text="再传一个" variant="secondary" size="sm" type="button" onclick={reset} />
      </div>
    {/if}

    {#if file && busy}
      <div class="uploader-actions">
        <Button text="取消" variant="ghost" size="sm" type="button" onclick={cancel} disabled={false} />
      </div>
    {/if}
  </div>
</div>

<style>
  .uploader-input {
    position: absolute;
    width: 1px;
    height: 1px;
    opacity: 0;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
  }
  .uploader-input-label {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-border, #d0d7de);
    border-radius: var(--radius-sm, 6px);
    cursor: pointer;
    background: var(--color-surface, #fff);
    font-weight: 500;
  }
  .uploader-input-label:focus-within {
    outline: 2px solid var(--color-primary, #0969da);
    outline-offset: 1px;
  }
  .uploader-file {
    max-width: 320px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .uploader-progress {
    height: 8px;
    border-radius: 4px;
    background: var(--color-border, #d0d7de);
    overflow: hidden;
    margin: var(--space-2) 0;
  }
  .uploader-progress-fill {
    height: 100%;
    background: var(--color-primary, #0969da);
    transition: width 0.2s ease;
  }
  .uploader-actions {
    display: flex;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }
  .uploader-preview {
    max-width: 240px;
    max-height: 180px;
    margin-top: var(--space-2);
    border-radius: var(--radius-sm, 6px);
    border: 1px solid var(--color-border, #d0d7de);
  }
</style>
