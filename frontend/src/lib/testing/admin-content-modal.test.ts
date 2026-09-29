import { describe, expect, it } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import AdminContentPage from '../../routes/admin/content/+page.svelte';
import type { AdminContentPageData } from '../../routes/admin/content/+page.server';

const mockPost = {
  id: 'p-100',
  title: '测试文章：Git Diff 与弹窗审核管理',
  author_username: 'developer',
  board_name: '技术讨论',
  status: 'draft',
  review_status: 'pending_review',
  created_at: 1700000000000
};

const mockData: AdminContentPageData = {
  state: 'ok',
  error: null,
  diff_error: null,
  posts: [mockPost],
  current: mockPost,
  diff: {
    from_version: 1,
    to_version: 2,
    reason: '重构并引入 Git Diff',
    before_body: '旧正文内容第一行\n旧正文第二行',
    after_body: '旧正文内容第一行（修改后）\n新增正文第三行'
  }
};

describe('Admin Content Page - Refactored Review Modal', () => {
  it('opens audit modal showing post content, poster info, and pass/reject buttons', async () => {
    const { container } = render(AdminContentPage, { props: { data: mockData, form: null } });

    // Find the review button in table
    const reviewBtn = container.querySelector('.review-action-btn');
    expect(reviewBtn).not.toBeNull();

    // Dialog should not be open initially
    expect(document.querySelector('.modal')).toBeNull();

    // Click to open modal
    await fireEvent.click(reviewBtn!);

    // Modal dialog is now opened
    const dialog = document.querySelector('.modal');
    expect(dialog).not.toBeNull();

    // 1) Shows post title & author info
    expect(dialog?.textContent).toContain('审核管理 · 测试文章：Git Diff 与弹窗审核管理');
    expect(dialog?.textContent).toContain('developer');
    expect(dialog?.textContent).toContain('发帖人');
    expect(dialog?.textContent).toContain('技术讨论');

    // 2) Shows post content
    expect(dialog?.textContent).toContain('测试文章：Git Diff 与弹窗审核管理');
    expect(dialog?.textContent).toContain('新增正文第三行');

    // 3) Shows bottom action buttons: 通过审核 & 不通过
    expect(dialog?.textContent).toContain('通过审核');
    expect(dialog?.textContent).toContain('不通过');

    // 4) Can switch to Git diff view
    const diffTab = Array.from(dialog?.querySelectorAll('.view-tab-btn') ?? []).find(
      (btn) => btn.textContent?.includes('版本比对')
    );
    expect(diffTab).not.toBeUndefined();
    await fireEvent.click(diffTab!);

    expect(dialog?.textContent).toContain('分栏');
    expect(dialog?.textContent).toContain('统一');
    expect(dialog?.textContent).toContain('+2');
    expect(dialog?.textContent).toContain('-1');
  });

  it('switches to reject reason input when clicking 不通过 and supports quick chips', async () => {
    const { container } = render(AdminContentPage, { props: { data: mockData, form: null } });

    const reviewBtn = container.querySelector('.review-action-btn');
    await fireEvent.click(reviewBtn!);

    const dialog = document.querySelector('.modal');
    expect(dialog).not.toBeNull();

    // Initially reject reason box is not displayed
    expect(dialog?.querySelector('.reject-panel')).toBeNull();

    // Click 不通过
    const rejectBtn = dialog?.querySelector('.audit-op-btn--reject');
    expect(rejectBtn).not.toBeNull();
    await fireEvent.click(rejectBtn!);

    // Reject panel is now open
    expect(dialog?.querySelector('.reject-panel')).not.toBeNull();
    expect(dialog?.textContent).toContain('请填写不通过理由');
    expect(dialog?.textContent).toContain('确认不通过并驳回');

    // Click a quick reason chip
    const quickChip = dialog?.querySelector('.quick-reason-chip');
    expect(quickChip).not.toBeNull();
    const chipText = quickChip?.textContent?.trim();
    await fireEvent.click(quickChip!);

    const textarea = dialog?.querySelector('#content-modal-reason') as HTMLTextAreaElement;
    expect(textarea.value).toBe(chipText);
  });
});
