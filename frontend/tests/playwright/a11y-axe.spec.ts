// M14-A11Y-06：axe 页面基线 —— serious/critical 违规 = P0 阻断。
//
// 报告 artifact：tests/a11y/axe-report.json（每次运行重建，含 commit/browser/
// 每次扫描的 violations/passed/incomplete）。
// 覆盖页面：主页/板块/板块详情/标签/搜索/用户/帖子/登录/注册/商城/我的积分/
// 衣柜/通知/管理后台（admin persona）/错误页（404）。
import { expect, test } from '@playwright/test';
import { runAxe, loginAs, appendRecord, browserInfo, LOCALE, currentCommit } from './helpers';

// axe 报告 artifact 由 playwright.config.ts globalSetup 每次调用清空一次
// （避免多 worker/project 的 beforeAll 相互覆盖）。

test.describe('axe 页面扫描（severe/critical = P0）', () => {
  const publicPages: Array<[string, string]> = [
    ['/', 'home'],
    ['/boards', 'boards'],
    ['/boards/tech', 'board-detail'],
    ['/tags', 'tags'],
    ['/search', 'search'],
    ['/search?q=Rust', 'search-with-query'],
    ['/users/alice', 'user-profile'],
    ['/login', 'login'],
    ['/register', 'register'],
    ['/shop', 'shop'],
    ['/notifications', 'notifications'],
    ['/posts/does-not-exist', 'error-404']
  ];

  for (const [path, label] of publicPages) {
    test(`axe: ${label}（${path}）`, async ({ page }) => {
      await page.goto(path);
      await page.waitForLoadState('networkidle').catch(() => {});
      const scan = await runAxe(page, label);
      // serious/critical 已在 runAxe 中作为 P0 断言；这里补充完整报告写入。
      expect(scan).toBeTruthy();
    });
  }

  test('axe: 帖子详情（member 视角）', async ({ page }) => {
    await loginAs(page, 'alice');
    await page.goto('/boards/general');
    const link = page.locator('a[href^="/posts/"]').first();
    await link.waitFor({ state: 'visible', timeout: 10_000 });
    const href = await link.getAttribute('href');
    await page.goto(href!);
    await page.waitForLoadState('networkidle').catch(() => {});
    const scan = await runAxe(page, 'post-detail-member');
    expect(scan).toBeTruthy();
  });

  test('axe: 管理后台（admin persona）', async ({ page }) => {
    await loginAs(page, 'admin');
    await page.goto('/admin');
    await page.waitForLoadState('networkidle').catch(() => {});
    const scan = await runAxe(page, 'admin-dashboard');
    expect(scan).toBeTruthy();
  });

  test('axe: 管理后台用户列表（admin persona）', async ({ page }) => {
    await loginAs(page, 'admin');
    await page.goto('/admin/users');
    await page.waitForLoadState('networkidle').catch(() => {});
    const scan = await runAxe(page, 'admin-users');
    expect(scan).toBeTruthy();
  });

  test('axe: 我的积分（member）', async ({ page }) => {
    await loginAs(page, 'alice');
    await page.goto('/me/balance');
    await page.waitForLoadState('networkidle').catch(() => {});
    const scan = await runAxe(page, 'me-balance');
    expect(scan).toBeTruthy();
  });

  test('axe: 衣柜（member）', async ({ page }) => {
    await loginAs(page, 'alice');
    await page.goto('/me/wardrobe');
    await page.waitForLoadState('networkidle').catch(() => {});
    const scan = await runAxe(page, 'me-wardrobe');
    expect(scan).toBeTruthy();
  });

  test('axe: 移动端主页（mobile project 触屏语义）', async ({ page }, testInfo) => {
    await page.setViewportSize({ width: 360, height: 740 });
    await page.goto('/');
    await page.waitForLoadState('networkidle').catch(() => {});
    const scan = await runAxe(page, 'home-mobile');
    expect(scan).toBeTruthy();
  });
});

test.describe('axe 报告记录（A11Y-10）', () => {
  test('生成 axe 报告记录条目', async ({ page }, testInfo) => {
    await page.goto('/');
    const info = await browserInfo(page);
    appendRecord({
      project: testInfo.project.name,
      browser: info.browser,
      browserVersion: info.version,
      viewport: `${testInfo.project.name === 'mobile-chromium' ? '360x740' : '1280x720'}`,
      locale: LOCALE,
      commit: currentCommit(),
      report: 'tests/a11y/axe-report.json',
      humanAcceptance: 'pending'
    });
    expect(true).toBeTruthy();
  });
});
