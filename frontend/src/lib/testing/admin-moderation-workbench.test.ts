import { describe, expect, it } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import ModerationCenterPage from '../../routes/admin/moderation/+page.svelte';
import type { ModerationCenterPageData } from '../../routes/admin/moderation/+page.server';

const mockPost = {
  id: 'post-test-1',
  title: '审核中心统一测试贴',
  author_username: 'moderator_user',
  board_name: '综合讨论',
  status: 'draft',
  review_status: 'pending_review',
  created_at: 1700000000000
};

const mockCase = {
  id: 'CASE-2024-001',
  title: '垃圾广告引流案件',
  status: 'open',
  priority: 'high',
  assigned_to: null,
  created_at: 1700000000000,
  updated_at: 1700000000000
};

const mockCenterData: ModerationCenterPageData = {
  activeTab: 'content',
  content: {
    state: 'ok',
    error: null,
    diff_error: null,
    posts: [mockPost],
    current: mockPost,
    diff: {
      from_version: 1,
      to_version: 2,
      reason: '修改了标题和段落',
      before_body: '旧文章段落',
      after_body: '新修改后的正文段落'
    }
  },
  cases: {
    items: [mockCase]
  },
  stats: {
    pendingContentCount: 1,
    openCasesCount: 1,
    totalPending: 2
  }
};

describe('Admin Moderation Center (Unified Workbench)', () => {
  it('renders unified header with metrics and two main tabs in one page', () => {
    const { container } = render(ModerationCenterPage, { props: { data: mockCenterData } });

    // 页面标题与指标
    expect(container.textContent).toContain('审核治理中心');
    expect(container.textContent).toContain('待办总计');
    expect(container.textContent).toContain('待审内容');
    expect(container.textContent).toContain('待处案件');

    // 两个主 Tab
    expect(container.textContent).toContain('待发内容审核');
    expect(container.textContent).toContain('用户举报案件');

    // 默认展示待发内容审核视图
    expect(container.textContent).toContain('待审内容队列');
    expect(container.textContent).toContain('审核中心统一测试贴');
  });

  it('can switch tabs to show moderation cases in the same page', async () => {
    const { container } = render(ModerationCenterPage, { props: { data: mockCenterData } });

    // 找到案件 Tab
    const tabs = container.querySelectorAll('.main-tab');
    const casesTab = Array.from(tabs).find((t) => t.textContent?.includes('用户举报案件'));
    expect(casesTab).not.toBeNull();

    // 点击切换至举报案件 Tab
    await fireEvent.click(casesTab!);

    // 此时同页面切换至案件列表视图
    expect(container.textContent).toContain('CASE-2024-001');
    expect(container.textContent).toContain('垃圾广告引流案件');
    expect(container.textContent).toContain('高优先级');
  });

  it('opens review modal with large size (modal--xl) for comfortable diff and content inspection', async () => {
    const { container } = render(ModerationCenterPage, { props: { data: mockCenterData } });

    // 找到审核操作按钮
    const reviewBtn = container.querySelector('.review-action-btn');
    expect(reviewBtn).not.toBeNull();

    // 点击打开审核大弹窗
    await fireEvent.click(reviewBtn!);

    // 断言弹窗已打开且具有 modal--xl 尺寸规格，适合宽屏长文与对比
    const modalEl = document.querySelector('.modal');
    expect(modalEl).not.toBeNull();
    expect(modalEl?.classList.contains('modal--xl')).toBe(true);
    expect(modalEl?.textContent).toContain('审核管理 · 审核中心统一测试贴');
  });
});
