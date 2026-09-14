<script lang="ts">
  import { onMount, untrack, type Snippet } from 'svelte';
  import { Editor } from '@tiptap/core';
  import StarterKit from '@tiptap/starter-kit';
  import { Markdown } from 'tiptap-markdown';
  import Placeholder from '@tiptap/extension-placeholder';
  import Image from '@tiptap/extension-image';
  import { Table } from '@tiptap/extension-table';
  import { TableRow } from '@tiptap/extension-table-row';
  import { TableCell } from '@tiptap/extension-table-cell';
  import { TableHeader } from '@tiptap/extension-table-header';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { uploadEditorAttachment, formatUploadErrorMessage, type UploadResult } from './upload';

  let {
    value = $bindable(''),
    placeholder = '使用 Markdown 或富文本编写内容…',
    maxChars = 50000,
    disabled = false,
    id = 'publish-content',
    name = 'markdown',
    onchange,
    /** 视频引用面板（由父级通过 snippet 传入），锚定在工具栏正下方渲染。 */
    insertPanel,
    /** 插入视频引用：点击工具栏视频按钮时触发（未传则不渲染该按钮）。 */
    oninsertvideo,
    videoOpen = false,
    videoBadge = 0
  }: {
    value?: string;
    placeholder?: string;
    maxChars?: number;
    disabled?: boolean;
    id?: string;
    name?: string;
    onchange?: (val: string) => void;
    insertPanel?: Snippet;
    oninsertvideo?: () => void;
    videoOpen?: boolean;
    videoBadge?: number;
  } = $props();

  let mounted = $state(false);
  let mode = $state<'rich' | 'source'>('rich');
  let editorElement: HTMLDivElement | null = $state(null);
  let sourceTextarea: HTMLTextAreaElement | null = $state(null);
  let editor: Editor | null = $state(null);
  let activeStateTick = $state(0);
  let isInternalChange = false;
  // 程序化 setContent 的抑制窗口：tiptap 的 setContent 即便传 emitUpdate:false，
  // StarterKit 插件链仍会追加派发不带 preventUpdate 标记的后续事务并触发 update
  // 事件。若不抑制，onUpdate 会把序列化规范化结果（如 '#' → '# '）写回 value，
  // 与源码模式下用户正在输入的原文互相覆盖，形成无限振荡循环
  // （effect_update_depth_exceeded）。
  let suppressUpdateDepth = 0;

  /**
   * 以外部内容同步进编辑器（草稿恢复、源码模式双向同步、模式切换回填等）。
   * 抑制期间触发的 update 事件不会写回 value。
   */
  function applyExternalContent(v: string) {
    if (!editor || editor.isDestroyed) return;
    suppressUpdateDepth++;
    try {
      editor.commands.setContent(v || '', { emitUpdate: false });
    } finally {
      queueMicrotask(() => {
        suppressUpdateDepth--;
      });
    }
  }

  // 上传状态
  let isUploading = $state(false);
  let uploadStatusText = $state('');
  let imageFileInput: HTMLInputElement | null = $state(null);
  let attachmentFileInput: HTMLInputElement | null = $state(null);

  async function handleUploadAndInsert(file: File) {
    if (disabled || isUploading) return;
    const isImage = file.type.startsWith('image/') || /\.(png|jpe?g|gif|webp|svg|avif)$/i.test(file.name);
    isUploading = true;
    uploadStatusText = isImage ? `正在上传图片「${file.name}」…` : `正在上传附件「${file.name}」…`;

    try {
      const result: UploadResult = await uploadEditorAttachment(file);
      if (result.isImage) {
        if (mode === 'rich' && editor && !editor.isDestroyed) {
          editor.chain().focus().setImage({ src: result.url, alt: result.filename }).run();
        } else {
          insertTextAtSourceCursor(`![${result.filename}](${result.url})\n`);
        }
      } else {
        if (mode === 'rich' && editor && !editor.isDestroyed) {
          editor
            .chain()
            .focus()
            .insertContent([
              {
                type: 'text',
                text: `📎 附件: ${result.filename}`,
                marks: [{ type: 'link', attrs: { href: result.url } }]
              },
              { type: 'text', text: ' ' }
            ])
            .run();
        } else {
          insertTextAtSourceCursor(`[📎 附件: ${result.filename}](${result.url}) `);
        }
      }
    } catch (err: unknown) {
      const msg = formatUploadErrorMessage(err);
      alert(`文件上传失败：${msg}`);
    } finally {
      isUploading = false;
      uploadStatusText = '';
    }
  }

  function insertTextAtSourceCursor(text: string) {
    if (!sourceTextarea) {
      value = (value || '') + text;
      onchange?.(value);
      return;
    }
    const start = sourceTextarea.selectionStart ?? sourceTextarea.value.length;
    const end = sourceTextarea.selectionEnd ?? start;
    const current = sourceTextarea.value;
    value = current.slice(0, start) + text + current.slice(end);
    onchange?.(value);
    queueMicrotask(() => {
      if (sourceTextarea) {
        const nextPos = start + text.length;
        sourceTextarea.focus();
        sourceTextarea.setSelectionRange(nextPos, nextPos);
      }
    });
  }

  function handleSourcePaste(e: ClipboardEvent) {
    const items = e.clipboardData?.items;
    if (!items) return;
    for (let i = 0; i < items.length; i++) {
      const item = items[i];
      if (item && item.type.startsWith('image/')) {
        const file = item.getAsFile();
        if (file) {
          e.preventDefault();
          handleUploadAndInsert(file);
          return;
        }
      }
    }
  }

  function handleSourceDrop(e: DragEvent) {
    const files = e.dataTransfer?.files;
    if (files && files.length > 0) {
      const file = files[0];
      if (file) {
        e.preventDefault();
        handleUploadAndInsert(file);
      }
    }
  }

  onMount(() => {
    mounted = true;
  });

  // ── 编辑器实例创建 ────────────────────────────────────────────────────────
  // 关键：ProseMirror 挂载 div 在 `{:else}` 分支内，只有 `mounted=true` 触发的
  // 下一次渲染后才会绑定 editorElement。onMount 回调执行时该 div 尚不存在
  // （editorElement 为 null），因此必须在 $effect 中监听绑定完成后再创建
  // Editor——否则实例永远不会被创建（所见即所得不可编辑 / 切回无内容的根因）。
  // value/disabled/placeholder 用 untrack 读取，避免成为依赖导致编辑器被销毁重建。
  $effect(() => {
    const el = editorElement;
    if (!el) return;

    const ed = new Editor({
      element: el,
      extensions: [
        StarterKit.configure({
          heading: {
            levels: [1, 2, 3]
          },
          // Tiptap v3 StarterKit 已内置 link 扩展：直接在此配置选项，
          // 不要再单独注册 Link（避免重复扩展名）。
          link: {
            openOnClick: false,
            autolink: true,
            HTMLAttributes: {
              rel: 'noopener noreferrer',
              target: '_blank'
            }
          }
        }),
        Markdown.configure({
          html: false,
          linkify: true,
          breaks: true,
          transformPastedText: true,
          transformCopiedText: true
        }),
        Placeholder.configure({
          placeholder: untrack(() => placeholder)
        }),
        Image.configure({
          inline: false,
          allowBase64: false
        }),
        Table.configure({
          resizable: false
        }),
        TableRow,
        TableHeader,
        TableCell
      ],
      editorProps: {
        handlePaste: (_view, event) => {
          const items = event.clipboardData?.items;
          if (!items) return false;
          for (let i = 0; i < items.length; i++) {
            const item = items[i];
            if (item && item.type.startsWith('image/')) {
              const file = item.getAsFile();
              if (file) {
                event.preventDefault();
                handleUploadAndInsert(file);
                return true;
              }
            }
          }
          return false;
        },
        handleDrop: (_view, event, _slice, moved) => {
          if (moved) return false;
          const files = event.dataTransfer?.files;
          if (files && files.length > 0) {
            const file = files[0];
            if (file) {
              event.preventDefault();
              handleUploadAndInsert(file);
              return true;
            }
          }
          return false;
        }
      },
      content: untrack(() => value) || '',
      editable: untrack(() => !disabled),
      onUpdate: ({ editor: ed }) => {
        // 程序化 setContent 触发的 update（如插件追加事务）一律忽略，
        // 否则序列化规范化（如 '#' → '# '）会与用户正在输入的原文互相覆盖形成死循环。
        if (suppressUpdateDepth > 0) return;
        isInternalChange = true;
        const md = (ed.storage as any).markdown?.getMarkdown?.() ?? '';
        value = md;
        onchange?.(md);
        queueMicrotask(() => {
          isInternalChange = false;
        });
      },
      onSelectionUpdate: () => {
        untrack(() => {
          activeStateTick++;
        });
      },
      onTransaction: () => {
        untrack(() => {
          activeStateTick++;
        });
      }
    });

    editor = ed;
    const editableElement = el.querySelector<HTMLElement>('[contenteditable="true"]');
    if (editableElement) {
      editableElement.id = id;
      editableElement.setAttribute('aria-label', '正文内容');
    }

    return () => {
      ed.destroy();
      if (editor === ed) editor = null;
    };
  });

  // 外部同步（草稿恢复、AI 写作助手正文回填、源码模式双向同步等）。
  $effect(() => {
    const v = value;
    if (mounted && !isInternalChange && editor && !editor.isDestroyed) {
      const current = (editor.storage as any).markdown?.getMarkdown?.() ?? '';
      if (v !== current) {
        applyExternalContent(v);
      }
    }
  });

  $effect(() => {
    if (editor && !editor.isDestroyed) {
      // emitUpdate=false：可编辑性变化不应广播 update 事件
      editor.setEditable(!disabled && !isUploading, false);
    }
  });

  function toggleMode(target: 'rich' | 'source') {
    if (mode === target) return;
    if (target === 'source') {
      // 切换到源码模式：从 editor 导出最新 markdown
      if (editor && !editor.isDestroyed) {
        value = (editor.storage as any).markdown?.getMarkdown?.() ?? value;
      }
      mode = 'source';
    } else {
      // 切换回富文本模式：把当前 textarea 中的 markdown 灌入 editor
      mode = 'rich';
      applyExternalContent(value);
    }
  }

  function handlePromptLink() {
    if (!editor) return;
    if (editor.isActive('link')) {
      editor.chain().focus().unsetLink().run();
      return;
    }
    const previousUrl = editor.getAttributes('link').href || '';
    const url = window.prompt('请输入链接 URL（例如 https://...）：', previousUrl);
    if (url === null) return;
    if (url === '') {
      editor.chain().focus().unsetLink().run();
      return;
    }
    const normalized = url.trim();
    if (!/^https?:\/\//i.test(normalized) && !/^mailto:/i.test(normalized)) {
      alert('链接必须以 http://、https:// 或 mailto: 开头');
      return;
    }
    editor.chain().focus().extendMarkRange('link').setLink({ href: normalized }).run();
  }

  function handleInsertTable() {
    if (!editor) return;
    editor.chain().focus().insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run();
  }

  function onImageFileSelected(e: Event) {
    const input = e.target as HTMLInputElement;
    const f = input.files?.[0];
    if (f) handleUploadAndInsert(f);
    input.value = '';
  }

  function onAttachmentFileSelected(e: Event) {
    const input = e.target as HTMLInputElement;
    const f = input.files?.[0];
    if (f) handleUploadAndInsert(f);
    input.value = '';
  }
</script>

<div class="rich-editor-wrapper">
  {#if !mounted}
    <!-- SSR 与无 JS 环境降级：原生 textarea 支持表单提交与可访问性 -->
    <textarea
      class="editor-textarea"
      {id}
      {name}
      bind:value
      placeholder={placeholder}
      rows="16"
      maxlength={maxChars}
      {disabled}
    ></textarea>
  {:else}
    <!-- 客户端交互式环境 -->
    <input type="hidden" {name} value={value} />

    <!-- 隐藏的图片与文件选择控件 -->
    <input
      type="file"
      bind:this={imageFileInput}
      accept="image/*"
      style="display:none;"
      onchange={onImageFileSelected}
    />
    <input
      type="file"
      bind:this={attachmentFileInput}
      accept="*/*"
      style="display:none;"
      onchange={onAttachmentFileSelected}
    />

    <div class="rich-editor-header">
      <div class="rich-editor-toolbar" role="toolbar" aria-label="富文本格式化工具栏">
        <!-- 历史操作组 -->
        <div class="toolbar-group">
          <button
            type="button"
            class="toolbar-btn"
            title="撤销 (Ctrl+Z)"
            aria-label="撤销"
            disabled={!editor || disabled || isUploading || !editor.can().undo()}
            onclick={() => editor?.chain().focus().undo().run()}
          >
            <Icon name="undo" size={15} />
          </button>
          <button
            type="button"
            class="toolbar-btn"
            title="重做 (Ctrl+Y)"
            aria-label="重做"
            disabled={!editor || disabled || isUploading || !editor.can().redo()}
            onclick={() => editor?.chain().focus().redo().run()}
          >
            <Icon name="redo" size={15} />
          </button>
        </div>

        <span class="toolbar-sep" aria-hidden="true"></span>

        <!-- 文本样式组 -->
        <div class="toolbar-group">
          <button
            type="button"
            class="toolbar-btn"
            class:is-active={activeStateTick && editor?.isActive('bold')}
            title="加粗 (Ctrl+B)"
            aria-label="加粗"
            disabled={disabled || isUploading || mode === 'source'}
            onclick={() => editor?.chain().focus().toggleBold().run()}
          >
            <Icon name="bold" size={15} />
          </button>
          <button
            type="button"
            class="toolbar-btn"
            class:is-active={activeStateTick && editor?.isActive('italic')}
            title="斜体 (Ctrl+I)"
            aria-label="斜体"
            disabled={disabled || isUploading || mode === 'source'}
            onclick={() => editor?.chain().focus().toggleItalic().run()}
          >
            <Icon name="italic" size={15} />
          </button>
          <button
            type="button"
            class="toolbar-btn"
            class:is-active={activeStateTick && editor?.isActive('strike')}
            title="删除线"
            aria-label="删除线"
            disabled={disabled || isUploading || mode === 'source'}
            onclick={() => editor?.chain().focus().toggleStrike().run()}
          >
            <Icon name="strikethrough" size={15} />
          </button>
          <button
            type="button"
            class="toolbar-btn"
            class:is-active={activeStateTick && editor?.isActive('code')}
            title="行内代码"
            aria-label="行内代码"
            disabled={disabled || isUploading || mode === 'source'}
            onclick={() => editor?.chain().focus().toggleCode().run()}
          >
            <Icon name="code" size={15} />
          </button>
        </div>

        <span class="toolbar-sep" aria-hidden="true"></span>

        <!-- 标题组 -->
        <div class="toolbar-group">
          <button
            type="button"
            class="toolbar-btn"
            class:is-active={activeStateTick && editor?.isActive('heading', { level: 1 })}
            title="一级标题"
            aria-label="一级标题"
            disabled={disabled || isUploading || mode === 'source'}
            onclick={() => editor?.chain().focus().toggleHeading({ level: 1 }).run()}
          >
            <Icon name="heading-1" size={15} />
          </button>
          <button
            type="button"
            class="toolbar-btn"
            class:is-active={activeStateTick && editor?.isActive('heading', { level: 2 })}
            title="二级标题"
            aria-label="二级标题"
            disabled={disabled || isUploading || mode === 'source'}
            onclick={() => editor?.chain().focus().toggleHeading({ level: 2 }).run()}
          >
            <Icon name="heading-2" size={15} />
          </button>
          <button
            type="button"
            class="toolbar-btn"
            class:is-active={activeStateTick && editor?.isActive('heading', { level: 3 })}
            title="三级标题"
            aria-label="三级标题"
            disabled={disabled || isUploading || mode === 'source'}
            onclick={() => editor?.chain().focus().toggleHeading({ level: 3 }).run()}
          >
            <Icon name="heading-3" size={15} />
          </button>
        </div>

        <span class="toolbar-sep" aria-hidden="true"></span>

        <!-- 列表与引用组 -->
        <div class="toolbar-group">
          <button
            type="button"
            class="toolbar-btn"
            class:is-active={activeStateTick && editor?.isActive('bulletList')}
            title="无序列表"
            aria-label="无序列表"
            disabled={disabled || isUploading || mode === 'source'}
            onclick={() => editor?.chain().focus().toggleBulletList().run()}
          >
            <Icon name="list" size={15} />
          </button>
          <button
            type="button"
            class="toolbar-btn"
            class:is-active={activeStateTick && editor?.isActive('orderedList')}
            title="有序列表"
            aria-label="有序列表"
            disabled={disabled || isUploading || mode === 'source'}
            onclick={() => editor?.chain().focus().toggleOrderedList().run()}
          >
            <Icon name="list-ordered" size={15} />
          </button>
          <button
            type="button"
            class="toolbar-btn"
            class:is-active={activeStateTick && editor?.isActive('blockquote')}
            title="引用块"
            aria-label="引用块"
            disabled={disabled || isUploading || mode === 'source'}
            onclick={() => editor?.chain().focus().toggleBlockquote().run()}
          >
            <Icon name="quote" size={15} />
          </button>
          <button
            type="button"
            class="toolbar-btn"
            class:is-active={activeStateTick && editor?.isActive('codeBlock')}
            title="代码块"
            aria-label="代码块"
            disabled={disabled || isUploading || mode === 'source'}
            onclick={() => editor?.chain().focus().toggleCodeBlock().run()}
          >
            <span class="codeblock-icon">&#123; &#125;</span>
          </button>
        </div>

        <span class="toolbar-sep" aria-hidden="true"></span>

        <!-- 多媒体与结构化元素组 -->
        <div class="toolbar-group">
          <button
            type="button"
            class="toolbar-btn"
            class:is-active={activeStateTick && editor?.isActive('link')}
            title="插入/编辑链接"
            aria-label="链接"
            disabled={disabled || isUploading || mode === 'source'}
            onclick={handlePromptLink}
          >
            <Icon name="link" size={15} />
          </button>
          <button
            type="button"
            class="toolbar-btn"
            title="上传并插入图片（支持直接拖拽/粘贴）"
            aria-label="插入图片"
            disabled={disabled || isUploading}
            onclick={() => imageFileInput?.click()}
          >
            <Icon name="image" size={15} />
          </button>
          <button
            type="button"
            class="toolbar-btn"
            title="上传附件（文件将保存在帖子附件并在正文插入链接）"
            aria-label="上传附件"
            disabled={disabled || isUploading}
            onclick={() => attachmentFileInput?.click()}
          >
            <Icon name="paperclip" size={15} />
          </button>
          {#if oninsertvideo}
            <button
              type="button"
              class="toolbar-btn"
              class:is-active={videoOpen}
              title="插入视频引用（支持直接链接/HLS/西瓜视频页面链接）"
              aria-label="插入视频"
              aria-haspopup="dialog"
              aria-expanded={videoOpen}
              aria-controls="composer-panel-video"
              disabled={disabled || isUploading}
              onclick={oninsertvideo}
            >
              <Icon name="video" size={15} />
              {#if videoBadge > 0}
                <b class="toolbar-badge">{videoBadge > 99 ? '99+' : videoBadge}</b>
              {/if}
            </button>
          {/if}
          <button
            type="button"
            class="toolbar-btn"
            class:is-active={activeStateTick && editor?.isActive('table')}
            title="插入 3×3 表格"
            aria-label="插入表格"
            disabled={disabled || isUploading || mode === 'source'}
            onclick={handleInsertTable}
          >
            <Icon name="table" size={15} />
          </button>
          <button
            type="button"
            class="toolbar-btn"
            title="分割线"
            aria-label="分割线"
            disabled={disabled || isUploading || mode === 'source'}
            onclick={() => editor?.chain().focus().setHorizontalRule().run()}
          >
            <Icon name="minus" size={15} />
          </button>
        </div>
      </div>

      <!-- 模式切换：富文本 (WYSIWYG) ↔ Markdown 源码 -->
      <div class="rich-editor-mode-switch" role="group" aria-label="编辑视图模式">
        <button
          type="button"
          class="mode-btn"
          class:is-active={mode === 'rich'}
          onclick={() => toggleMode('rich')}
        >
          所见即所得
        </button>
        <button
          type="button"
          class="mode-btn"
          class:is-active={mode === 'source'}
          onclick={() => toggleMode('source')}
        >
          Markdown
        </button>
      </div>

      <!-- 编辑器锚定弹层（视频引用）：挂在 header 内，top:100% 定位到工具栏正下方 -->
      {#if insertPanel}
        {@render insertPanel()}
      {/if}
    </div>

    <!-- 上传中提示条 -->
    {#if isUploading}
      <div class="upload-progress-banner" role="status" aria-live="polite">
        <span class="upload-spinner" aria-hidden="true"><i></i><i></i><i></i></span>
        <span class="upload-text">{uploadStatusText}</span>
      </div>
    {/if}

    {#if activeStateTick && editor?.isActive('table') && mode === 'rich'}
      <!-- 表格操作快捷条 -->
      <div class="table-actions-bar" role="group" aria-label="表格调整工具">
        <span class="table-actions-title">表格操作:</span>
        <button type="button" class="table-btn" onclick={() => editor?.chain().focus().addRowAfter().run()}>+增行</button>
        <button type="button" class="table-btn" onclick={() => editor?.chain().focus().deleteRow().run()}>-删行</button>
        <button type="button" class="table-btn" onclick={() => editor?.chain().focus().addColumnAfter().run()}>+增列</button>
        <button type="button" class="table-btn" onclick={() => editor?.chain().focus().deleteColumn().run()}>-删列</button>
        <button type="button" class="table-btn is-danger" onclick={() => editor?.chain().focus().deleteTable().run()}>删除表格</button>
      </div>
    {/if}

    <div class="rich-editor-body">
      <!-- 富文本编辑挂载点 -->
      <div
        class="prosemirror-mount prose"
        bind:this={editorElement}
        style:display={mode === 'rich' ? 'block' : 'none'}
      ></div>

      <!-- Markdown 源码模式 -->
      {#if mode === 'source'}
        <textarea
          class="source-textarea"
          {id}
          bind:this={sourceTextarea}
          bind:value
          oninput={() => onchange?.(value)}
          onpaste={handleSourcePaste}
          ondrop={handleSourceDrop}
          placeholder="在此直接输入或编辑 Markdown 源码（支持直接拖入或粘贴图片）…"
          rows="16"
          maxlength={maxChars}
          {disabled}
        ></textarea>
      {/if}
    </div>
  {/if}
</div>

<style>
  .rich-editor-wrapper {
    display: flex;
    flex-direction: column;
    width: 100%;
    background: var(--color-bg-card, #fff);
    border-radius: var(--radius-md, 6px);
  }

  .rich-editor-header {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: var(--space-2, 8px);
    padding: var(--space-2, 8px) var(--space-3, 12px);
    border-bottom: var(--border-default, 1px solid var(--color-border, #e5e7eb));
    background: var(--color-bg-card, #fff);
  }

  .rich-editor-toolbar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 2px;
  }

  .toolbar-group {
    display: inline-flex;
    align-items: center;
    gap: 1px;
  }

  .toolbar-btn {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    padding: 0;
    font-size: var(--text-xs, 12px);
    font-family: inherit;
    color: var(--color-text-secondary, #4b5563);
    background: transparent;
    border: none;
    border-radius: var(--radius-sm, 4px);
    cursor: pointer;
    transition: background-color 0.15s ease, color 0.15s ease, transform 0.05s ease;
  }

  /* 视频引用计数徽标：待发布引用 > 0 时显示在按钮右上角 */
  .toolbar-badge {
    position: absolute;
    top: -4px;
    right: -5px;
    min-width: 14px;
    height: 14px;
    padding: 0 3px;
    color: var(--color-text-on-brand, #ffffff);
    background: var(--color-brand, #b23e2a);
    border-radius: 7px;
    font-size: 9px;
    font-weight: 700;
    font-style: normal;
    line-height: 14px;
    text-align: center;
    pointer-events: none;
  }

  .toolbar-btn:hover:not(:disabled) {
    background: var(--color-bg-subtle, #f3f4f6);
    color: var(--color-text-primary, #111827);
  }

  .toolbar-btn:active:not(:disabled) {
    transform: scale(0.96);
  }

  .toolbar-btn:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }

  .toolbar-btn.is-active {
    background: var(--color-primary-subtle, rgba(178, 62, 42, 0.12));
    color: var(--color-primary, #b23e2a);
    font-weight: 600;
  }

  .codeblock-icon {
    font-family: var(--font-family-mono, monospace);
    font-weight: 700;
    font-size: 13px;
    letter-spacing: -1px;
  }

  .toolbar-sep {
    display: inline-block;
    width: 1px;
    height: 16px;
    margin: 0 4px;
    background: var(--color-border, #e5e7eb);
  }

  .rich-editor-mode-switch {
    display: inline-flex;
    align-items: center;
    border: 1px solid var(--color-border, #e5e7eb);
    border-radius: var(--radius-md, 6px);
    padding: 2px;
    background: var(--color-bg-subtle, #f9fafb);
  }

  .mode-btn {
    padding: 3px 10px;
    font-size: var(--text-xs, 12px);
    border: none;
    background: transparent;
    color: var(--color-text-secondary, #4b5563);
    border-radius: var(--radius-sm, 4px);
    cursor: pointer;
    transition: background-color 0.15s, color 0.15s;
  }

  .mode-btn.is-active {
    background: var(--color-bg-card, #fff);
    color: var(--color-text-primary, #111827);
    font-weight: 600;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.05);
  }

  .upload-progress-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px var(--space-4, 16px);
    background: var(--color-primary-subtle, rgba(178, 62, 42, 0.08));
    border-bottom: 1px solid rgba(178, 62, 42, 0.2);
    color: var(--color-primary, #b23e2a);
    font-size: var(--text-xs, 12px);
  }

  /* 三个变形方块（与 aui-spinner 同一视觉语言），替代旋转圆环 */
  .upload-spinner {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    height: 10px;
  }

  .upload-spinner i {
    width: 3px;
    height: 100%;
    background: currentColor;
    animation: editor-blocks-pulse 800ms ease-in-out infinite;
  }

  .upload-spinner i:nth-child(2) {
    animation-delay: 100ms;
  }

  .upload-spinner i:nth-child(3) {
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

  .table-actions-bar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px var(--space-3, 12px);
    background: var(--color-bg-subtle, #f9fafb);
    border-bottom: var(--border-default, 1px solid var(--color-border, #e5e7eb));
    font-size: var(--text-xs, 12px);
  }

  .table-actions-title {
    color: var(--color-text-secondary, #6b7280);
    margin-right: 4px;
  }

  .table-btn {
    padding: 2px 8px;
    font-size: var(--text-xs, 12px);
    border: 1px solid var(--color-border, #d1d5db);
    background: var(--color-bg-card, #fff);
    border-radius: var(--radius-sm, 4px);
    cursor: pointer;
    color: var(--color-text-secondary, #374151);
  }

  .table-btn:hover {
    background: var(--color-bg-subtle, #f3f4f6);
    color: var(--color-text-primary, #111827);
  }

  .table-btn.is-danger {
    color: var(--color-danger, #dc2626);
    border-color: rgba(220, 38, 38, 0.3);
  }

  .table-btn.is-danger:hover {
    background: rgba(220, 38, 38, 0.08);
  }

  .rich-editor-body {
    position: relative;
    width: 100%;
    min-height: 320px;
  }

  .editor-textarea,
  .source-textarea {
    width: 100%;
    min-height: 320px;
    padding: var(--space-4, 16px);
    border: none;
    outline: none;
    resize: vertical;
    font-family: var(--font-family-mono, monospace);
    font-size: var(--text-sm, 14px);
    line-height: 1.7;
    color: var(--color-text-primary, #111827);
    background: var(--color-bg-card, #fff);
    box-sizing: border-box;
  }

  .prosemirror-mount {
    min-height: 320px;
    cursor: text;
  }

  :global(.prosemirror-mount .ProseMirror) {
    outline: none;
    min-height: 320px;
    padding: var(--space-4, 16px);
    color: var(--color-text-primary, inherit);
    line-height: 1.75;
    font-size: var(--text-base, 15px);
  }

  :global(.prosemirror-mount .ProseMirror p.is-editor-empty:first-child::before) {
    content: attr(data-placeholder);
    float: left;
    color: var(--color-text-tertiary, #9ca3af);
    pointer-events: none;
    height: 0;
  }

  :global(.prosemirror-mount .ProseMirror h1) {
    font-size: 1.6em;
    font-weight: 700;
    margin-top: 1em;
    margin-bottom: 0.5em;
  }

  :global(.prosemirror-mount .ProseMirror h2) {
    font-size: 1.35em;
    font-weight: 600;
    margin-top: 0.9em;
    margin-bottom: 0.4em;
  }

  :global(.prosemirror-mount .ProseMirror h3) {
    font-size: 1.15em;
    font-weight: 600;
    margin-top: 0.8em;
    margin-bottom: 0.3em;
  }

  :global(.prosemirror-mount .ProseMirror blockquote) {
    border-left: 3px solid var(--color-primary, #b23e2a);
    padding-left: var(--space-3, 12px);
    margin: 1em 0;
    color: var(--color-text-secondary, #4b5563);
    font-style: italic;
  }

  :global(.prosemirror-mount .ProseMirror pre) {
    background: var(--color-bg-subtle, #f3f4f6);
    border-radius: var(--radius-md, 6px);
    padding: var(--space-3, 12px);
    font-family: var(--font-family-mono, monospace);
    font-size: var(--text-sm, 13px);
    overflow-x: auto;
    margin: 1em 0;
  }

  :global(.prosemirror-mount .ProseMirror code) {
    background: var(--color-bg-subtle, rgba(0, 0, 0, 0.05));
    border-radius: 3px;
    padding: 0.15em 0.35em;
    font-family: var(--font-family-mono, monospace);
    font-size: 0.9em;
  }

  :global(.prosemirror-mount .ProseMirror ul),
  :global(.prosemirror-mount .ProseMirror ol) {
    padding-left: 1.5em;
    margin: 0.8em 0;
  }

  :global(.prosemirror-mount .ProseMirror li) {
    margin-bottom: 0.25em;
  }

  :global(.prosemirror-mount .ProseMirror table) {
    border-collapse: collapse;
    table-layout: fixed;
    width: 100%;
    margin: 1em 0;
    overflow: hidden;
  }

  :global(.prosemirror-mount .ProseMirror table td),
  :global(.prosemirror-mount .ProseMirror table th) {
    min-width: 1em;
    border: 1px solid var(--color-border, #d1d5db);
    padding: 6px 10px;
    vertical-align: top;
    box-sizing: border-box;
    position: relative;
  }

  :global(.prosemirror-mount .ProseMirror table th) {
    font-weight: 600;
    text-align: left;
    background-color: var(--color-bg-subtle, #f9fafb);
  }

  :global(.prosemirror-mount .ProseMirror a) {
    color: var(--color-primary, #b23e2a);
    text-decoration: underline;
  }

  :global(.prosemirror-mount .ProseMirror img) {
    max-width: 100%;
    height: auto;
    border-radius: var(--radius-md, 6px);
    margin: 0.75em 0;
    display: block;
  }

  :global(.prosemirror-mount .ProseMirror hr) {
    border: none;
    border-top: 1px solid var(--color-border, #e5e7eb);
    margin: 1.5em 0;
  }
</style>
