import { describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import SimpleCommentEditor from './SimpleCommentEditor.svelte';
import * as uploadModule from './upload';
import * as clientModule from '$lib/api/client';

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

const mockUsers = [
  { username: 'david_smith', display_name: '大卫', level: 3 },
  { username: 'charlie', display_name: null, level: 2 },
  { username: 'alice_wong', display_name: '爱丽丝', level: 5 },
  { username: 'bob', display_name: 'Bob Ross', level: 1 },
  { username: 'eve', display_name: null, level: 1 },
  { username: 'frank', display_name: '法兰克', level: 2 } // 第 6 个用户，用于测试最多 5 条限制
];

vi.mock('$lib/api/client', async () => {
  const actual = await vi.importActual<typeof clientModule>('$lib/api/client');
  return {
    ...actual,
    suggestMentionUsers: vi.fn(async (_fetch, q?: string, limit: number = 5) => {
      const filtered = q
        ? mockUsers.filter(
            (u) =>
              u.username.toLowerCase().includes(q.toLowerCase()) ||
              (u.display_name && u.display_name.toLowerCase().includes(q.toLowerCase()))
          )
        : mockUsers;
      return {
        items: filtered.slice(0, limit)
      };
    })
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

  it('刚输入 @ 不展示，输入第二个字符后才展示建议浮窗：最多 5 条、优先展示昵称，无昵称展示账号', async () => {
    render(SimpleCommentEditor, {
      id: 'comment-input',
      value: ''
    });

    const textarea = screen.getByRole('textbox') as HTMLTextAreaElement;

    // 刚输入 @：不展示浮窗
    textarea.value = '@';
    textarea.selectionStart = 1;
    textarea.selectionEnd = 1;
    await fireEvent.input(textarea);

    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();

    // 输入第二个字符 @a：开始展示浮窗
    textarea.value = '@a';
    textarea.selectionStart = 2;
    textarea.selectionEnd = 2;
    await fireEvent.input(textarea);

    // 等待防抖与 API 请求完成
    await waitFor(
      () => {
        expect(screen.getByRole('listbox')).toBeInTheDocument();
      },
      { timeout: 1500 }
    );

    const items = screen.getAllByRole('option');
    // 最多显示 5 条
    expect(items.length).toBeLessThanOrEqual(5);

    // 检查「优先昵称 没有昵称才是登录账号」
    // david_smith 有昵称「大卫」：优先显示昵称，副文本显示 @david_smith
    // alice_wong 有昵称「爱丽丝」：优先显示昵称，副文本显示 @alice_wong
    expect(screen.getByText('爱丽丝')).toBeInTheDocument();
    expect(screen.getByText('@alice_wong')).toBeInTheDocument();
  });

  it('输入 @ch 模糊匹配相关用户，回车或点击选中后替换为 @username 并带空格', async () => {
    render(SimpleCommentEditor, {
      id: 'comment-input',
      value: ''
    });

    const textarea = screen.getByRole('textbox') as HTMLTextAreaElement;

    // 模拟输入 @ch
    textarea.value = '@ch';
    textarea.selectionStart = 3;
    textarea.selectionEnd = 3;
    await fireEvent.input(textarea);

    await waitFor(() => {
      expect(screen.getByRole('listbox')).toBeInTheDocument();
    });

    const items = screen.getAllByRole('option');
    expect(items.length).toBe(1);
    expect(screen.getByText('charlie')).toBeInTheDocument();

    // 回车选中
    await fireEvent.keyDown(textarea, { key: 'Enter' });

    // 弹窗关闭且内容被替换为 @charlie
    await waitFor(() => {
      expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
      expect(textarea.value).toBe('@charlie ');
    });
  });

  it('鼠标点击候选用户顺利插入 mention 并关闭浮窗', async () => {
    render(SimpleCommentEditor, {
      id: 'comment-input',
      value: 'Hello '
    });

    const textarea = screen.getByRole('textbox') as HTMLTextAreaElement;

    textarea.value = 'Hello @da';
    textarea.selectionStart = 9;
    textarea.selectionEnd = 9;
    await fireEvent.input(textarea);

    await waitFor(() => {
      expect(screen.getByRole('listbox')).toBeInTheDocument();
    });

    const option = screen.getByRole('option');
    expect(option).toHaveTextContent('大卫');

    // 触发 mousedown
    await fireEvent.mouseDown(option);

    await waitFor(() => {
      expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
      expect(textarea.value).toBe('Hello @david_smith ');
    });
  });

  it('邮箱格式（如 user@example.com）不触发提及浮窗', async () => {
    render(SimpleCommentEditor, {
      id: 'comment-input',
      value: ''
    });

    const textarea = screen.getByRole('textbox') as HTMLTextAreaElement;

    textarea.value = 'user@example.com';
    textarea.selectionStart = 5;
    textarea.selectionEnd = 5;
    await fireEvent.input(textarea);

    // 保持不触发
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
  });

  it('按 Escape 键关闭浮窗', async () => {
    render(SimpleCommentEditor, {
      id: 'comment-input',
      value: ''
    });

    const textarea = screen.getByRole('textbox') as HTMLTextAreaElement;

    textarea.value = '@d';
    textarea.selectionStart = 2;
    textarea.selectionEnd = 2;
    await fireEvent.input(textarea);

    await waitFor(() => {
      expect(screen.getByRole('listbox')).toBeInTheDocument();
    });

    await fireEvent.keyDown(textarea, { key: 'Escape' });

    await waitFor(() => {
      expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
    });
  });
});
