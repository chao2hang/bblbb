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

describe('Admin Content Page - Modal and Git Diff', () => {
  it('opens audit modal when clicking the review button on the list row', async () => {
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
    expect(dialog?.textContent).toContain('审核管理 · 测试文章：Git Diff 与弹窗审核管理');
    expect(dialog?.textContent).toContain('v1 → v2');
    expect(dialog?.textContent).toContain('+2');
    expect(dialog?.textContent).toContain('-1');

    // Shows Git diff view inside modal
    expect(dialog?.textContent).toContain('分栏');
    expect(dialog?.textContent).toContain('统一');

    // Shows audit management panel
    expect(dialog?.textContent).toContain('审核处置管理');
    expect(dialog?.textContent).toContain('通过审核（公开发布）');
    expect(dialog?.textContent).toContain('驳回修改（退回草稿）');
  });

  it('switches between approve and reject actions with quick reasons', async () => {
    const { container } = render(AdminContentPage, { props: { data: mockData, form: null } });

    const reviewBtn = container.querySelector('.review-action-btn');
    await fireEvent.click(reviewBtn!);

    const dialog = document.querySelector('.modal');
    expect(dialog).not.toBeNull();

    // Switch to reject
    const rejectBtn = dialog?.querySelector('.audit-tab-btn--reject');
    expect(rejectBtn).not.toBeNull();
    await fireEvent.click(rejectBtn!);

    // Check button label updated to reject
    expect(dialog?.textContent).toContain('确认驳回并退回');

    // Click a quick reason chip
    const quickChip = dialog?.querySelector('.audit-quick-chip');
    expect(quickChip).not.toBeNull();
    const chipText = quickChip?.textContent?.trim();
    await fireEvent.click(quickChip!);

    const input = dialog?.querySelector('#content-modal-reason') as HTMLInputElement;
    expect(input.value).toBe(chipText);
  });
});
