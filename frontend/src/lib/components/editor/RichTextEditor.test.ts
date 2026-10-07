import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import RichTextEditor from './RichTextEditor.svelte';

/** 等待 ProseMirror 实例真正挂载（$effect 异步创建编辑器）。 */
async function waitForProseMirror(container: HTMLElement): Promise<HTMLElement> {
  // jsdom 缺少 Range.prototype.getClientRects，补全避免 ProseMirror scrollToSelection 报错
  if (typeof Range !== 'undefined' && !Range.prototype.getClientRects) {
    Range.prototype.getClientRects = () => [] as unknown as DOMRectList;
    Range.prototype.getBoundingClientRect = () => ({
      bottom: 0,
      height: 0,
      left: 0,
      right: 0,
      top: 0,
      width: 0,
      x: 0,
      y: 0,
      toJSON: () => {}
    });
  }
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

  it('未传 oninsertvideo 时不渲染视频按钮；传入后点击触发回调并显示待发布徽标', async () => {
    // 默认：底部工具条模式（无视频入口），工具栏不含视频按钮
    const { container, unmount } = render(RichTextEditor, {
      value: '',
      id: 'test-editor'
    });
    await waitForProseMirror(container);
    expect(screen.queryByRole('button', { name: '插入视频' })).toBeNull();
    unmount();

    // 传入入口回调：按钮出现，点击触发，且待发布数量渲染为徽标
    const oninsertvideo = vi.fn();
    const video = render(RichTextEditor, {
      value: '',
      id: 'test-editor-video',
      oninsertvideo,
      videoBadge: 2
    });
    await waitForProseMirror(video.container);

    const videoBtn = screen.getByRole('button', { name: '插入视频' });
    await userEvent.click(videoBtn);
    expect(oninsertvideo).toHaveBeenCalledTimes(1);
    expect(video.container.querySelector('.toolbar-badge')?.textContent).toBe('2');
  });

  it('点击插入回复可见按钮正确插入回复可见标记块并在所见即所得渲染卡片', async () => {
    let currentValue = '';
    const { container } = render(RichTextEditor, {
      value: '',
      id: 'test-reply-editor',
      onchange: (v: string) => {
        currentValue = v;
      }
    });
    const pm = await waitForProseMirror(container);

    const lockBtn = screen.getByRole('button', { name: '插入回复可见内容' });
    expect(lockBtn).toBeInTheDocument();
    await userEvent.click(lockBtn);

    expect(currentValue).toContain(':::reply');
    expect(currentValue).toContain('此处填写回复后可见的内容');
    expect(currentValue).toContain(':::');

    // 所见即所得 DOM 验证：渲染回复可见卡片及标题栏
    const box = pm.querySelector('.topic-restricted-editor-box');
    expect(box).toBeTruthy();
    expect(box?.textContent).toContain('🔒 回复可见内容');
    expect(box?.textContent).toContain('此处填写回复后可见的内容');
  });

  it('初始 Markdown 带有 :::reply 时所见即所得正确反序列化为回复可见卡片', async () => {
    const { container } = render(RichTextEditor, {
      value: '前文段落\n\n:::reply\n隐藏内容666\n:::\n\n后文段落',
      id: 'test-reply-initial'
    });
    const pm = await waitForProseMirror(container);

    const box = pm.querySelector('.topic-restricted-editor-box');
    expect(box).toBeTruthy();
    expect(box?.textContent).toContain('🔒 回复可见内容');
    expect(box?.textContent).toContain('隐藏内容666');
  });
});
