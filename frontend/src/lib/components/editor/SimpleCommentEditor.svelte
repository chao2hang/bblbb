<script lang="ts">
  import Icon from '$lib/components/ui/Icon.svelte';
  import { uploadEditorAttachment, formatUploadErrorMessage, type UploadResult } from './upload';

  let {
    value = $bindable(''),
    id = 'comment-input',
    name = 'markdown',
    placeholder = '写下你的回复…（支持直接 Ctrl+V 粘贴或拖入图片）',
    rows = 4,
    maxChars = 10000,
    disabled = false
  }: {
    value?: string;
    id?: string;
    name?: string;
    placeholder?: string;
    rows?: number;
    maxChars?: number;
    disabled?: boolean;
  } = $props();

  let textareaEl: HTMLTextAreaElement | null = $state(null);
  let imageInput: HTMLInputElement | null = $state(null);
  let fileInput: HTMLInputElement | null = $state(null);
  let isUploading = $state(false);
  let uploadStatusText = $state('');

  async function handleUpload(file: File) {
    if (disabled || isUploading) return;
    const isImage = file.type.startsWith('image/') || /\.(png|jpe?g|gif|webp|svg|avif)$/i.test(file.name);
    isUploading = true;
    uploadStatusText = isImage ? `正在上传图片…` : `正在上传附件…`;

    try {
      const result: UploadResult = await uploadEditorAttachment(file);
      const markdownToInsert = result.isImage
        ? `![${result.filename}](${result.url})\n`
        : `[📎 附件: ${result.filename}](${result.url}) `;
      insertAtCursor(markdownToInsert);
    } catch (err: unknown) {
      const msg = formatUploadErrorMessage(err);
      alert(`图片/文件上传失败：${msg}`);
    } finally {
      isUploading = false;
      uploadStatusText = '';
    }
  }

  function insertAtCursor(text: string) {
    if (!textareaEl) {
      value = (value || '') + text;
      return;
    }
    const start = textareaEl.selectionStart ?? textareaEl.value.length;
    const end = textareaEl.selectionEnd ?? start;
    const current = textareaEl.value;
    value = current.slice(0, start) + text + current.slice(end);

    queueMicrotask(() => {
      if (textareaEl) {
        const nextPos = start + text.length;
        textareaEl.focus();
        textareaEl.setSelectionRange(nextPos, nextPos);
      }
    });
  }

  function handlePaste(e: ClipboardEvent) {
    const items = e.clipboardData?.items;
    if (!items) return;
    for (let i = 0; i < items.length; i++) {
      const item = items[i];
      if (item && item.type.startsWith('image/')) {
        const file = item.getAsFile();
        if (file) {
          e.preventDefault();
          handleUpload(file);
          return;
        }
      }
    }
  }

  function handleDrop(e: DragEvent) {
    const files = e.dataTransfer?.files;
    if (files && files.length > 0) {
      const file = files[0];
      if (file) {
        e.preventDefault();
        handleUpload(file);
      }
    }
  }

  function onImageSelected(e: Event) {
    const input = e.target as HTMLInputElement;
    const f = input.files?.[0];
    if (f) handleUpload(f);
    input.value = '';
  }

  function onFileSelected(e: Event) {
    const input = e.target as HTMLInputElement;
    const f = input.files?.[0];
    if (f) handleUpload(f);
    input.value = '';
  }
</script>

<div class="simple-comment-editor">
  <input
    type="file"
    bind:this={imageInput}
    accept="image/*"
    style="display:none;"
    onchange={onImageSelected}
  />
  <input
    type="file"
    bind:this={fileInput}
    accept="*/*"
    style="display:none;"
    onchange={onFileSelected}
  />

  <textarea
    {id}
    {name}
    bind:this={textareaEl}
    bind:value
    {placeholder}
    {rows}
    maxlength={maxChars}
    disabled={disabled || isUploading}
    class="input-field editor-textarea simple-editor-textarea"
    onpaste={handlePaste}
    ondrop={handleDrop}
  ></textarea>

  <div class="simple-editor-footer">
    <div class="simple-editor-actions">
      <button
        type="button"
        class="mini-tool-btn"
        title="上传图片（支持直接 Ctrl+V 粘贴截图或拖入）"
        disabled={disabled || isUploading}
        onclick={() => imageInput?.click()}
      >
        <Icon name="image" size={14} />
        <span>图片</span>
      </button>

      <button
        type="button"
        class="mini-tool-btn"
        title="上传附件（文件将保存在附件库中并在正文插入链接）"
        disabled={disabled || isUploading}
        onclick={() => fileInput?.click()}
      >
        <Icon name="paperclip" size={14} />
        <span>附件</span>
      </button>

      <span class="paste-hint">可直接截屏后按 Ctrl+V 粘贴图片</span>
    </div>

    {#if isUploading}
      <div class="uploading-indicator" role="status" aria-live="polite">
        <span class="mini-spinner" aria-hidden="true"></span>
        <span>{uploadStatusText}</span>
      </div>
    {/if}
  </div>
</div>

<style>
  .simple-comment-editor {
    display: flex;
    flex-direction: column;
    border: var(--border-default, 1px solid var(--color-border, #e5e7eb));
    border-radius: var(--radius-md, 6px);
    background: var(--color-bg-card, #fff);
    overflow: hidden;
    transition: border-color 0.15s ease;
  }

  .simple-comment-editor:focus-within {
    border-color: var(--color-primary, #b23e2a);
    box-shadow: 0 0 0 1px var(--color-primary, #b23e2a);
  }

  .simple-editor-textarea {
    width: 100%;
    border: none !important;
    outline: none !important;
    box-shadow: none !important;
    border-radius: 0;
    padding: var(--space-3, 12px);
    font-family: inherit;
    font-size: var(--text-sm, 14px);
    line-height: 1.6;
    color: var(--color-text-primary, #111827);
    background: transparent;
    resize: vertical;
    box-sizing: border-box;
  }

  .simple-editor-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: var(--space-2, 8px);
    padding: 6px var(--space-3, 12px);
    background: var(--color-bg-subtle, #f9fafb);
    border-top: 1px solid var(--color-border, #e5e7eb);
    font-size: var(--text-xs, 12px);
  }

  .simple-editor-actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .mini-tool-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 8px;
    font-size: var(--text-xs, 12px);
    font-family: inherit;
    color: var(--color-text-secondary, #4b5563);
    background: var(--color-bg-card, #fff);
    border: 1px solid var(--color-border, #d1d5db);
    border-radius: var(--radius-sm, 4px);
    cursor: pointer;
    transition: background-color 0.15s ease, color 0.15s ease, border-color 0.15s ease;
  }

  .mini-tool-btn:hover:not(:disabled) {
    background: var(--color-bg-subtle, #f3f4f6);
    color: var(--color-text-primary, #111827);
    border-color: var(--color-text-tertiary, #9ca3af);
  }

  .mini-tool-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .paste-hint {
    color: var(--color-text-tertiary, #9ca3af);
    font-size: 11px;
    margin-left: 4px;
  }

  .uploading-indicator {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--color-primary, #b23e2a);
    font-size: var(--text-xs, 12px);
  }

  .mini-spinner {
    width: 12px;
    height: 12px;
    border: 2px solid rgba(178, 62, 42, 0.25);
    border-top-color: var(--color-primary, #b23e2a);
    border-radius: 50%;
    animation: editor-spin 0.8s linear infinite;
  }

  @keyframes editor-spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
