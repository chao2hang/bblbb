// M03-UI-08：用户面交互验收——键盘、触屏、减少动效、响应式布局。
// 复用 src/lib/testing/a11y.ts 夹具（matchMedia/按键/焦点/触屏）。
/// <reference path="./ambient-fs.d.ts" />
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { readFileSync } from 'fs';
import { cleanup, fireEvent, render } from '@testing-library/svelte';
import UserCard from '$lib/components/UserCard.svelte';
import BoardsPage from '../../routes/boards/+page.svelte';
import {
  fireTouchEnd,
  fireTouchStart,
  focusFirst,
  getFocused,
  installMatchMedia,
  setMediaMatches,
  tabOrder
} from './a11y';

const user = {
  id: 'u1',
  username: 'alice',
  display_name: '爱丽丝',
  bio: '公开简介',
  level: 7,
  avatar_attachment_id: null,
  cover_attachment_id: null,
  signature: '公开签名',
  created_at: 0
};

const boards: Array<{
  id: string;
  slug: string;
  name: string;
  description: string;
  version: number;
  created_at: number;
  updated_at: number;
}> = [
  { id: 'b1', slug: 'general', name: '综合讨论', description: '日常', version: 1, created_at: 0, updated_at: 0 },
  { id: 'b2', slug: 'tech', name: '技术分享', description: '技术', version: 1, created_at: 0, updated_at: 0 }
];

beforeEach(() => {
  installMatchMedia({ '(max-width: 640px)': false, '(prefers-reduced-motion: reduce)': false });
});

afterEach(() => {
  cleanup();
});

describe('M03-UI-08 键盘', () => {
  it('板块总览：Tab 遍历序按 DOM 顺序（面包屑→板块链接），聚焦可见', async () => {
    render(BoardsPage, { props: { data: { boards, error: null } } });
    const root = document.body;
    focusFirst(root);
    expect(getFocused()).not.toBeNull();
    const order = await tabOrder(root);
    const hrefs = order.map((el) => (el as HTMLAnchorElement).getAttribute('href')).filter(Boolean);
    expect(hrefs).toContain('/boards/general');
    expect(hrefs).toContain('/boards/tech');
    // 面包屑首页在板块链接之前（DOM 序）。
    expect(hrefs.indexOf('/')).toBeLessThan(hrefs.indexOf('/boards/general'));
  });

  it('UserCard：键盘 Focus 打开浮层后 Escape 关闭（无鼠标）', async () => {
    render(UserCard, { props: { user } });
    const trigger = document.querySelector('a.author-hover-trigger') as HTMLAnchorElement;
    await fireEvent.focus(trigger);
    expect(document.querySelector('.user-card-popover')).not.toBeNull();
    await fireEvent.keyDown(document, { key: 'Escape' });
    expect(document.querySelector('.user-card-popover')).toBeNull();
  });
});

describe('M03-UI-08 触屏', () => {
  it('窄屏：触摸点击触发底部卡（touchstart→touchend→click）', async () => {
    setMediaMatches('(max-width: 640px)', true);
    render(UserCard, { props: { user } });
    const trigger = document.querySelector('a.author-hover-trigger') as HTMLAnchorElement;
    fireTouchStart(trigger);
    fireTouchEnd(trigger);
    await fireEvent.click(trigger);
    expect(document.querySelector('.user-card-sheet')).not.toBeNull();
    expect(document.querySelector('.user-card-sheet-backdrop')).toBeNull(); // 不阻挡导航
  });

  it('桌面：触摸点击保持原导航，不弹出浮层（hover 专属）', async () => {
    render(UserCard, { props: { user } });
    const trigger = document.querySelector('a.author-hover-trigger') as HTMLAnchorElement;
    fireTouchStart(trigger);
    fireTouchEnd(trigger);
    await fireEvent.click(trigger);
    expect(document.querySelector('.user-card-popover')).toBeNull();
    expect(document.querySelector('.user-card-sheet')).toBeNull();
  });
});

describe('M03-UI-08 减少动效', () => {
  it('shipped CSS：prefers-reduced-motion 下关闭过渡/动画（结构守卫）', () => {
    // 结构守卫：已发布的样式源必须包含减少动效降级块（组件动画一律在此禁用）。
    // node:fs 仅在测试（node 环境）读取仓库内已提交的样式源。
    const componentsCss = readFileSync('src/lib/styles/components.css', 'utf8');
    const baseCss = readFileSync('src/lib/styles/base.css', 'utf8');
    expect(componentsCss).toContain('@media (prefers-reduced-motion: reduce)');
    expect(baseCss).toContain('@media (prefers-reduced-motion: reduce)');
    // 至少一处把过渡/动画关掉（transition/animation: none，或时长压到 0/0.01ms）。
    expect(componentsCss + baseCss).toMatch(
      /(transition:\s*none|animation:\s*none|duration:\s*(0|0\.01ms))/i
    );
  });

  it('减少动效偏好下组件仍正常渲染（不抛错）', () => {
    setMediaMatches('(prefers-reduced-motion: reduce)', true);
    render(UserCard, { props: { user } });
    render(BoardsPage, { props: { data: { boards, error: null } } });
    expect(document.querySelector('.author-hover-trigger')).not.toBeNull();
    expect(document.body.textContent).toContain('技术分享');
  });
});

describe('M03-UI-08 响应式布局', () => {
  it('断点切换：宽屏 hover 浮层 ↔ 窄屏底部卡', async () => {
    render(UserCard, { props: { user } });
    const trigger = document.querySelector('a.author-hover-trigger') as HTMLAnchorElement;
    // 宽屏：hover 出浮层。
    await fireEvent.mouseEnter(trigger);
    expect(document.querySelector('.user-card-popover')).not.toBeNull();
    await fireEvent.keyDown(document, { key: 'Escape' });
    expect(document.querySelector('.user-card-popover')).toBeNull();

    // 切到窄屏：点击出底部卡。
    setMediaMatches('(max-width: 640px)', true);
    await fireEvent.click(trigger);
    expect(document.querySelector('.user-card-sheet')).not.toBeNull();
  });

  it('板块总览在窄屏下仍渲染全部板块卡（网格不丢内容）', () => {
    setMediaMatches('(max-width: 640px)', true);
    render(BoardsPage, { props: { data: { boards, error: null } } });
    expect(document.body.textContent).toContain('综合讨论');
    expect(document.body.textContent).toContain('技术分享');
  });
});
