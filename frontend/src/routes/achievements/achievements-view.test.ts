import { describe, expect, it, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/svelte';
import AchievementsPage from './+page.svelte';
import type { AchievementsPageData, AchievementCard } from './+page.server';

afterEach(() => {
  cleanup();
  localStorage.clear();
});

const mockCards: AchievementCard[] = [
  {
    code: 'first_post',
    name: '首发帖',
    description: '发布第一篇帖子',
    category: 'community',
    rewardCoin: 5,
    isHidden: false,
    iconUrl: '/api/v1/achievements/first_post/icon',
    unlocked: true,
    unlockedAt: 1700000000000,
    progress: 1,
    target: 1,
    equipped: false
  },
  {
    code: 'streak7',
    name: '连续签到 7 天',
    description: '连续签到一周',
    category: 'activity',
    rewardCoin: 15,
    isHidden: false,
    iconUrl: null,
    unlocked: true,
    unlockedAt: 1700000000000,
    progress: 7,
    target: 7,
    equipped: true
  },
  {
    code: 'hundred_posts',
    name: '百帖',
    description: '发布 100 篇帖子',
    category: 'community',
    rewardCoin: 50,
    isHidden: false,
    iconUrl: null,
    unlocked: false,
    unlockedAt: null,
    progress: 30,
    target: 100,
    equipped: false
  },
  {
    code: 'secret',
    name: '???',
    description: '隐藏成就：达成条件保密，解锁后揭晓',
    category: 'secret',
    rewardCoin: 20,
    isHidden: true,
    iconUrl: '/api/v1/achievements/secret/icon',
    unlocked: false,
    unlockedAt: null,
    progress: 0,
    target: 0,
    equipped: false
  }
];

function createPageData(): AchievementsPageData {
  return {
    cards: mockCards,
    stats: { unlocked: 2, total: 4, equipped: 1, maxSlots: 3 },
    problem: null,
    error: null
  };
}

describe('成就墙 卡片展示 / 列表展示 切换与分类筛选', () => {
  it('渲染分类 Tab 栏与展示视图切换按钮', () => {
    const { container } = render(AchievementsPage, {
      props: { data: createPageData(), form: null }
    });

    const viewButtons = container.querySelectorAll('.view-mode-btn');
    expect(viewButtons.length).toBe(2);
    expect(container.textContent).toContain('卡片展示');
    expect(container.textContent).toContain('列表展示');

    const tabs = container.querySelectorAll('.tabs .tab');
    expect(tabs.length).toBe(4);
    expect(container.textContent).toContain('全部 (4)');
    expect(container.textContent).toContain('进行中 (1)');
    expect(container.textContent).toContain('已解锁 (2)');
    expect(container.textContent).toContain('隐藏 (1)');
  });

  it('默认卡片展示（grid 视图）下渲染 is-grid 容器及成就卡片信息', () => {
    const { container } = render(AchievementsPage, {
      props: { data: createPageData(), form: null }
    });

    const grid = container.querySelector('.achievement-grid');
    expect(grid).toBeTruthy();
    expect(grid?.classList.contains('is-grid')).toBe(true);
    expect(grid?.getAttribute('data-view')).toBe('grid');

    const cards = container.querySelectorAll('.achievement-card');
    expect(cards.length).toBe(4);

    expect(container.textContent).toContain('首发帖');
    expect(container.textContent).toContain('连续签到 7 天');
    expect(container.textContent).toContain('百帖');
    expect(container.textContent).toContain('???');
    expect(container.textContent).toContain('已装备');
    expect(container.textContent).toContain('已解锁');
    expect(container.textContent).toContain('隐藏');
  });

  it('点击「列表展示」切换为列表视图，且写入 localStorage', async () => {
    const { container } = render(AchievementsPage, {
      props: { data: createPageData(), form: null }
    });

    const listBtn = Array.from(container.querySelectorAll('.view-mode-btn')).find((b) =>
      b.textContent?.includes('列表展示')
    ) as HTMLButtonElement;
    expect(listBtn).toBeTruthy();

    await fireEvent.click(listBtn);

    const grid = container.querySelector('.achievement-grid');
    expect(grid?.classList.contains('is-list')).toBe(true);
    expect(grid?.getAttribute('data-view')).toBe('list');
    expect(localStorage.getItem('bblbb_achievements_view_mode')).toBe('list');

    // 再次点击「卡片展示」切换回卡片视图
    const cardBtn = Array.from(container.querySelectorAll('.view-mode-btn')).find((b) =>
      b.textContent?.includes('卡片展示')
    ) as HTMLButtonElement;
    expect(cardBtn).toBeTruthy();

    await fireEvent.click(cardBtn);
    expect(grid?.classList.contains('is-grid')).toBe(true);
    expect(grid?.getAttribute('data-view')).toBe('grid');
    expect(localStorage.getItem('bblbb_achievements_view_mode')).toBe('grid');
  });

  it('分类 tab 筛选在卡片展示模式下正常工作', async () => {
    const { container } = render(AchievementsPage, {
      props: { data: createPageData(), form: null }
    });

    // 点击「已解锁」tab
    const unlockedTab = Array.from(container.querySelectorAll('.tabs .tab')).find((t) =>
      t.textContent?.includes('已解锁')
    ) as HTMLButtonElement;
    expect(unlockedTab).toBeTruthy();

    await fireEvent.click(unlockedTab);

    const cards = container.querySelectorAll('.achievement-card');
    expect(cards.length).toBe(2);
    expect(container.textContent).toContain('首发帖');
    expect(container.textContent).toContain('连续签到 7 天');
    expect(container.textContent).not.toContain('百帖');
  });
});
