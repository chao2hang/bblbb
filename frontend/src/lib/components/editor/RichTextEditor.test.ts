import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import RichTextEditor from './RichTextEditor.svelte';

/** 等待 ProseMirror 实例真正挂载（$effect 异步创建编辑器）。 */
async function waitForProseMirror(container: HTMLElement): Promise<HTMLElement> {
  let pm: HTMLElement | null = null;
  await vi.waitFor(() => {
    pm = container.querySelector('.prosemirror-mount .ProseMirror');
    expect(pm).toBeTruthy();
  });
  return pm!;
}

describe('RichTextEditor', () => {
  it('挂载后创建可编辑的 ProseMirror 实例（所见即所得可用）', async () => {
    const { container } = render(RichTextEditor, {
      value: '**加粗文字** 与普通段落',
      id: 'test-editor'
    });

    const pm = await waitForProseMirror(container);
    // 可编辑性：contenteditable=true（Bug 回归：此前实例从未创建导致无法编辑）
    expect(pm.getAttribute('contenteditable')).toBe('true');
    // 初始内容经 markdown 解析后注入
    expect(pm.textContent).toContain('加粗文字');
    expect(pm.querySelector('strong')).toBeTruthy();
  });

  it('挂载后渲染优化后的富文本工具栏与 SVG 图标', async () => {
    render(RichTextEditor, {
      value: '## 测试标题\n\n测试段落内容',
      id: 'test-editor'
    });

    // 格式化工具栏按钮
    expect(screen.getByRole('toolbar', { name: '富文本格式化工具栏' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '撤销' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '重做' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '加粗' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '斜体' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '删除线' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '行内代码' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '一级标题' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '二级标题' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '三级标题' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '无序列表' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '有序列表' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '引用块' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '代码块' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '链接' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '插入图片' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '上传附件' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '插入表格' })).toBeInTheDocument();

    // 模式切换按钮
    expect(screen.getByRole('button', { name: '所见即所得' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Markdown' })).toBeInTheDocument();
  });

  it('Markdown 源码编辑后切回所见即所得，内容正确同步', async () => {
    const { container } = render(RichTextEditor, {
      value: '**初始加粗内容**',
      id: 'test-mode-editor'
    });
    await waitForProseMirror(container);

    // 切换到 Markdown 源码模式
    await userEvent.click(screen.getByRole('button', { name: 'Markdown' }));
    const textarea = container.querySelector('textarea.source-textarea') as HTMLTextAreaElement;
    expect(textarea).toBeTruthy();
    expect(textarea.value).toContain('初始加粗内容');

    // 在源码模式编辑内容
    await userEvent.clear(textarea);
    await userEvent.type(textarea, '# 新标题内容');

    // 切换回所见即所得模式（Bug 回归：此前切回后无内容）
    await userEvent.click(screen.getByRole('button', { name: '所见即所得' }));
    const pm = await waitForProseMirror(container);
    expect(pm.textContent).toContain('新标题内容');
    expect(pm.querySelector('h1')).toBeTruthy();
  });

  it('disabled 状态下工具栏按钮正确置灰且编辑器不可编辑', async () => {
    const { container } = render(RichTextEditor, {
      value: '禁用测试',
      disabled: true
    });

    const pm = await waitForProseMirror(container);
    expect(pm.getAttribute('contenteditable')).toBe('false');

    const boldBtn = screen.getByRole('button', { name: '加粗' });
    expect(boldBtn).toBeDisabled();
    const italicBtn = screen.getByRole('button', { name: '斜体' });
    expect(italicBtn).toBeDisabled();
    const imageBtn = screen.getByRole('button', { name: '插入图片' });
    expect(imageBtn).toBeDisabled();
    const attachBtn = screen.getByRole('button', { name: '上传附件' });
    expect(attachBtn).toBeDisabled();
  });
});
