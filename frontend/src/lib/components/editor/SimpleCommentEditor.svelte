<script lang="ts">
  import Icon from '$lib/components/ui/Icon.svelte';
  import Avatar from '$lib/components/ui/Avatar.svelte';
  import { suggestMentionUsers, type MentionSuggestionItem } from '$lib/api/client';
  import { getCaretCoordinates } from './caret';
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

  // Mention (@提及) 状态
  let showMentionPopup = $state(false);
  let mentionList = $state<MentionSuggestionItem[]>([]);
  let mentionSelectedIndex = $state(0);
  let mentionLoading = $state(false);
  let mentionAtIndex = $state(-1);
  let mentionQuery = $state('');
  let mentionCoords = $state({ top: 0, left: 0 });
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;
  let currentFetchId = 0;

  function closeMention() {
    showMentionPopup = false;
    mentionList = [];
    mentionAtIndex = -1;
    mentionQuery = '';
    mentionSelectedIndex = 0;
    if (debounceTimer) {
      clearTimeout(debounceTimer);
      debounceTimer = null;
    }
  }

  function updateMentionCoords() {
    if (!textareaEl) return;
    try {
      const coords = getCaretCoordinates(textareaEl, mentionAtIndex >= 0 ? mentionAtIndex : textareaEl.selectionStart || 0);
      const lineHeight = coords.lineHeight || 20;

      const editorEl = textareaEl.parentElement;
      const editorWidth = editorEl ? editorEl.clientWidth : 400;

      let left = textareaEl.offsetLeft + coords.left - textareaEl.scrollLeft;
      const caretTop = textareaEl.offsetTop + coords.top - textareaEl.scrollTop;

      const popupWidth = 240;
      const popupHeight = 210;

      if (left + popupWidth > editorWidth - 12) {
        left = Math.max(12, editorWidth - popupWidth - 12);
      }
      if (left < 12) {
        left = 12;
      }

      // 检查下方空间；若下方不足且上方空间更大则翻转到上方
      const textareaRect = textareaEl.getBoundingClientRect();
      const caretViewportY = textareaRect.top + coords.top - textareaEl.scrollTop;
      const spaceBelow = window.innerHeight - (caretViewportY + lineHeight);
      const spaceAbove = caretViewportY;

      let top = caretTop + lineHeight + 4;
      if (spaceBelow < popupHeight && spaceAbove > spaceBelow) {
        top = Math.max(0, caretTop - popupHeight - 4);
      }

      mentionCoords = { top, left };
    } catch {
      mentionCoords = { top: 36, left: 12 };
    }
  }

  async function fetchMentionSuggestions(q: string) {
    if (debounceTimer) clearTimeout(debounceTimer);
    const fetchId = ++currentFetchId;

    debounceTimer = setTimeout(async () => {
      mentionLoading = true;
      try {
        const fetchFn = typeof window !== 'undefined' ? window.fetch : fetch;
        const res = await suggestMentionUsers(fetchFn, q, 5);
        if (fetchId !== currentFetchId) return;
        mentionList = (res?.items || []).slice(0, 5);
        mentionSelectedIndex = 0;
        showMentionPopup = mentionList.length > 0;
        if (showMentionPopup) {
          updateMentionCoords();
        }
      } catch {
        if (fetchId !== currentFetchId) return;
        mentionList = [];
        showMentionPopup = false;
      } finally {
        if (fetchId === currentFetchId) {
          mentionLoading = false;
        }
      }
    }, 100);
  }

  function checkMentionTrigger() {
    if (disabled || !textareaEl) {
      closeMention();
      return;
    }
    const cursor = textareaEl.selectionStart;
    if (cursor === null || cursor !== textareaEl.selectionEnd) {
      closeMention();
      return;
    }

    const text = textareaEl.value;
    const textBeforeCursor = text.slice(0, cursor);

    const lastAtIndex = textBeforeCursor.lastIndexOf('@');
    if (lastAtIndex === -1) {
      closeMention();
      return;
    }

    // 检查 @ 前驱字符（非行首时排除邮箱形态，即不能紧跟字母/数字/_/-）
    if (lastAtIndex > 0) {
      const prevChar = textBeforeCursor[lastAtIndex - 1];
      if (/[a-zA-Z0-9_\-]/.test(prevChar)) {
        closeMention();
        return;
      }
    }

    const query = textBeforeCursor.slice(lastAtIndex + 1);

    // 查询内容包含空白符或换行则视为结束
    if (/[\s\r\n]/.test(query)) {
      closeMention();
      return;
    }

    // 刚输入 @ 不展示；输入第二个字符之后（即 @ 后至少输入 1 个字符）才开始展示
    if (query.length < 1) {
      closeMention();
      return;
    }

    // 单次输入查询长度保护
    if (query.length > 30) {
      closeMention();
      return;
    }

    mentionAtIndex = lastAtIndex;
    mentionQuery = query;
    updateMentionCoords();
    fetchMentionSuggestions(query);
  }

  function selectMentionUser(user: MentionSuggestionItem) {
    if (!textareaEl) return;
    const currentText = textareaEl.value;
    const beforeAt = currentText.slice(0, mentionAtIndex >= 0 ? mentionAtIndex : 0);
    const afterCursor = currentText.slice(textareaEl.selectionStart);

    const needsSpace = !afterCursor.startsWith(' ');
    const mentionText = `@${user.username}${needsSpace ? ' ' : ''}`;

    value = beforeAt + mentionText + afterCursor;
    const nextPos = beforeAt.length + mentionText.length;
    closeMention();

    queueMicrotask(() => {
      if (textareaEl) {
        textareaEl.focus();
        textareaEl.setSelectionRange(nextPos, nextPos);
      }
    });
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (showMentionPopup && mentionList.length > 0) {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        mentionSelectedIndex = (mentionSelectedIndex + 1) % mentionList.length;
        return;
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault();
        mentionSelectedIndex = (mentionSelectedIndex - 1 + mentionList.length) % mentionList.length;
        return;
      }
      if (e.key === 'Enter' || e.key === 'Tab') {
        e.preventDefault();
        selectMentionUser(mentionList[mentionSelectedIndex]);
        return;
      }
      if (e.key === 'Escape') {
        e.preventDefault();
        closeMention();
        return;
      }
    }
  }

  $effect(() => {
    function handleDocClick(e: MouseEvent) {
      if (!showMentionPopup) return;
      const target = e.target as HTMLElement | null;
      if (target && !target.closest('.mention-popup') && target !== textareaEl) {
        closeMention();
      }
    }
    window.addEventListener('click', handleDocClick);
    return () => {
      window.removeEventListener('click', handleDocClick);
    };
  });

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
    onkeydown={handleKeyDown}
    oninput={checkMentionTrigger}
    onclick={checkMentionTrigger}
    onkeyup={(e) => {
      if (e.key === 'ArrowLeft' || e.key === 'ArrowRight' || e.key === 'Home' || e.key === 'End') {
        checkMentionTrigger();
      }
    }}
    onscroll={() => {
      if (showMentionPopup) updateMentionCoords();
    }}
  ></textarea>

  {#if showMentionPopup && mentionList.length > 0}
    <div
      class="mention-popup"
      role="listbox"
      aria-label="提及用户候选列表"
      style="top: {mentionCoords.top}px; left: {mentionCoords.left}px;"
    >
      <div class="mention-popup-header">
        <span>建议提及用户</span>
        <span class="mention-popup-hint">↑↓ 选择 · 回车确认</span>
      </div>
      <ul class="mention-popup-list" role="presentation">
        {#each mentionList as user, i (user.username)}
          {@const hasDisplayName = Boolean(user.display_name?.trim())}
          {@const primaryName = hasDisplayName ? user.display_name!.trim() : user.username}
          <li
            class="mention-item"
            class:is-active={i === mentionSelectedIndex}
            role="option"
            aria-selected={i === mentionSelectedIndex}
            onmousedown={(e) => {
              e.preventDefault();
              selectMentionUser(user);
            }}
            onmouseenter={() => {
              mentionSelectedIndex = i;
            }}
          >
            <Avatar
              name={primaryName}
              size="xs"
              seed={user.username}
            />
            <div class="mention-user-text">
              <span class="mention-primary-name">{primaryName}</span>
              {#if hasDisplayName && primaryName !== user.username}
                <span class="mention-secondary-name">@{user.username}</span>
              {/if}
            </div>
            {#if user.level !== undefined && user.level !== null}
              <span class="mention-level">Lv.{user.level}</span>
            {/if}
          </li>
        {/each}
      </ul>
    </div>
  {/if}

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
        <span class="mini-spinner" aria-hidden="true"><i></i><i></i><i></i></span>
        <span>{uploadStatusText}</span>
      </div>
    {/if}
  </div>
</div>

<style>
  .simple-comment-editor {
    position: relative;
    display: flex;
    flex-direction: column;
    border: var(--border-default, 1px solid var(--color-border, #e5e7eb));
    border-radius: var(--radius-md, 6px);
    background: var(--color-bg-card, #fff);
    overflow: visible;
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
    border-top-left-radius: var(--radius-md, 6px);
    border-top-right-radius: var(--radius-md, 6px);
    padding: var(--space-3, 12px);
    font-family: inherit;
    font-size: var(--text-sm, 14px);
    line-height: 1.6;
    color: var(--color-text-primary, #111827);
    background: transparent;
    resize: vertical;
    box-sizing: border-box;
  }

  /* 提及 (@mention) 候选浮窗 */
  .mention-popup {
    position: absolute;
    z-index: 100;
    width: 250px;
    max-width: calc(100% - 24px);
    background: var(--color-bg-card, #ffffff);
    border: 1px solid var(--color-border, #e5e7eb);
    border-radius: var(--radius-md, 6px);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.18), 0 2px 6px rgba(0, 0, 0, 0.08);
    overflow: hidden;
    animation: mention-pop 0.12s ease-out;
  }

  @keyframes mention-pop {
    from {
      opacity: 0;
      transform: translateY(-4px) scale(0.98);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  .mention-popup-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 10px;
    background: var(--color-bg-subtle, #f9fafb);
    border-bottom: 1px solid var(--color-border, #e5e7eb);
    font-size: 11px;
    font-weight: 500;
    color: var(--color-text-secondary, #6b7280);
  }

  .mention-popup-hint {
    font-size: 10px;
    color: var(--color-text-tertiary, #9ca3af);
  }

  .mention-popup-list {
    list-style: none;
    margin: 0;
    padding: 4px;
    max-height: 220px;
    overflow-y: auto;
  }

  .mention-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border-radius: var(--radius-sm, 4px);
    cursor: pointer;
    transition: background-color 0.12s ease;
  }

  .mention-item.is-active,
  .mention-item:hover {
    background: var(--color-bg-subtle, #f3f4f6);
  }

  .mention-user-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
    line-height: 1.3;
  }

  .mention-primary-name {
    font-size: var(--text-xs, 12px);
    font-weight: 600;
    color: var(--color-text-primary, #111827);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .mention-secondary-name {
    font-size: 11px;
    color: var(--color-text-tertiary, #9ca3af);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .mention-level {
    font-size: 10px;
    font-weight: 500;
    padding: 1px 4px;
    border-radius: 3px;
    background: var(--color-bg-subtle, #f3f4f6);
    border: 1px solid var(--color-border, #e5e7eb);
    color: var(--color-text-secondary, #6b7280);
    flex-shrink: 0;
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
    border-bottom-left-radius: var(--radius-md, 6px);
    border-bottom-right-radius: var(--radius-md, 6px);
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

  /* 三个变形方块（与 aui-spinner 同一视觉语言），替代旋转圆环 */
  .mini-spinner {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    height: 10px;
  }

  .mini-spinner i {
    width: 3px;
    height: 100%;
    background: currentColor;
    animation: editor-blocks-pulse 800ms ease-in-out infinite;
  }

  .mini-spinner i:nth-child(2) {
    animation-delay: 100ms;
  }

  .mini-spinner i:nth-child(3) {
    animation-delay: 200ms;
  }

  @keyframes editor-blocks-pulse {
    0%,
    100% {
      opacity: 0.35;
      transform: scaleY(0.7);
    }
    50% {
      opacity: 1;
      transform: scaleY(1);
    }
  }
</style>
