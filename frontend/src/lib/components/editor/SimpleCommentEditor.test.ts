import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import SimpleCommentEditor from './SimpleCommentEditor.svelte';
import * as uploadModule from './upload';

vi.mock('./upload', async () => {
  const actual = await vi.importActual<typeof uploadModule>('./upload');
  return {
    ...actual,
    uploadEditorAttachment: vi.fn(async (file: File) => ({
      id: 'mock-att-id',
      url: 'https://s3.example.com/mock.png',
      filename: file.name,
      isImage: true,
      size: 1024
    }))
  };
});

describe('SimpleCommentEditor', () => {
  it('渲染原生 textarea 与轻量图片/附件操作栏', () => {
    render(SimpleCommentEditor, {
      id: 'comment-input',
      value: '回复内容',
      placeholder: '写下你的回复…'
    });

    const textarea = screen.getByPlaceholderText('写下你的回复…') as HTMLTextAreaElement;
    expect(textarea).toBeInTheDocument();
    expect(textarea.id).toBe('comment-input');
    expect(textarea.value).toBe('回复内容');

    // 辅助工具栏按钮
    expect(screen.getByRole('button', { name: /图片/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /附件/ })).toBeInTheDocument();
    expect(screen.getByText('可直接截屏后按 Ctrl+V 粘贴图片')).toBeInTheDocument();
  });

  it('粘贴图片自动调用上传并在光标处插入 Markdown', async () => {
    render(SimpleCommentEditor, {
      id: 'test-reply-box',
      value: '已有文字'
    });

    const textarea = screen.getByRole('textbox') as HTMLTextAreaElement;
    const file = new File(['image-bytes'], 'screenshot.png', { type: 'image/png' });

    // 构造带 image 数据的 ClipboardEvent
    const clipboardData = {
      items: [
        {
          type: 'image/png',
          getAsFile: () => file
        }
      ]
    };

    const pasteEvent = new Event('paste', { bubbles: true, cancelable: true });
    Object.assign(pasteEvent, { clipboardData });
    textarea.dispatchEvent(pasteEvent);

    // 等待上传完成
    await vi.waitFor(() => {
      expect(uploadModule.uploadEditorAttachment).toHaveBeenCalledWith(file);
      expect(textarea.value).toContain('![screenshot.png](https://s3.example.com/mock.png)');
    });
  });

  it('disabled 状态下输入框与按钮禁用', () => {
    render(SimpleCommentEditor, {
      id: 'disabled-box',
      disabled: true
    });

    expect(screen.getByRole('textbox')).toBeDisabled();
    expect(screen.getByRole('button', { name: /图片/ })).toBeDisabled();
    expect(screen.getByRole('button', { name: /附件/ })).toBeDisabled();
  });
});
