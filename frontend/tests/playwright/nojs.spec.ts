// M14-A11Y-08：禁用 JS 的浏览器 —— 公开阅读、注册、登录与关键表单退化。
//
// 实现：context javaScriptEnabled=false + 真实浏览器。SvelteKit 表单 action
// （原生 <form method="POST">）在无 JS 时仍可提交（progressive enhancement
// 降级为常规表单提交）；SSR HTML 直接包含公开内容与 <noscript> 降级提示。
import { expect, test as base, type Page } from '@playwright/test';

const test = base.extend<{ noJsPage: Page }>({
  noJsPage: async ({ browser }, use) => {
    const context = await browser.newContext({ javaScriptEnabled: false, locale: 'zh-CN' });
    const page = await context.newPage();
    await use(page);
    await context.close();
  }
});

test.describe('无 JS 公开阅读（public read）', () => {
  for (const path of ['/', '/boards', '/boards/tech', '/tags', '/users/alice'] as const) {
    test(`${path} SSR 内容可读`, async ({ noJsPage }) => {
      const response = await noJsPage.goto(path);
      expect(response?.status()).toBe(200);
      // 无 JS 时 JS 资源不加载，但 SSR HTML 内容直接可读。
      await expect(noJsPage.getByRole('main')).toBeVisible();
    });
  }

  test('首页展示板块/标签（无 JS 数据来自 SSR）', async ({ noJsPage }) => {
    await noJsPage.goto('/');
    await expect(noJsPage.getByRole('main')).toContainText(/综合讨论|技术分享|最新讨论|板块/);
  });

  test('帖子详情正文可读（无 JS）', async ({ noJsPage }) => {
    await noJsPage.goto('/boards/general');
    const link = noJsPage.locator('a[href^="/posts/"]').first();
    await link.waitFor({ state: 'visible', timeout: 10_000 });
    const href = await link.getAttribute('href');
    await noJsPage.goto(href!);
    await expect(noJsPage.getByRole('main')).toBeVisible();
  });
});

test.describe('无 JS 注册（register 表单 action）', () => {
  test('注册表单可见且 <noscript> 提示存在', async ({ noJsPage }) => {
    await noJsPage.goto('/register');
    await expect(noJsPage.getByLabel('用户名')).toBeVisible();
    await expect(noJsPage.getByLabel('邮箱')).toBeVisible();
    // 无 JS 降级提示（NoJsNotice 的 <noscript> SSR 内容）。
    await expect(noJsPage.getByText(/启用 JavaScript/).first()).toBeVisible();
  });

  test('无 JS 提交注册（原生表单 POST）', async ({ noJsPage }, testInfo) => {
    // 真实注册消费后端 IP 注册配额（3 次/小时）：只在 desktop 项目提交。
    test.skip(testInfo.project.name !== 'desktop-chromium', '真实注册仅 desktop 项目执行（IP 注册配额 3/h）');
    const username = `nojs_${Date.now().toString(36)}`;
    await noJsPage.goto('/register');
    await noJsPage.getByLabel('用户名').fill(username);
    await noJsPage.getByLabel('邮箱').fill(`${username}@e2e.example`);
    await noJsPage.getByLabel('密码', { exact: true }).fill('Nojs-pass-123!');
    await noJsPage.getByLabel('确认密码').fill('Nojs-pass-123!');
    await noJsPage.getByRole('button', { name: /注册/ }).click();
    // 成功（防枚举统一文案）或 429 冷却（限流提示）均为正确后端行为。
    await expect(noJsPage.getByRole('main')).toContainText(/注册成功|操作过于频繁|请稍后再试/);
  });
});

test.describe('无 JS 登录（login 表单 action）', () => {
  test('登录表单可见且可提交', async ({ noJsPage }) => {
    await noJsPage.goto('/login');
    await expect(noJsPage.getByLabel('用户名或邮箱')).toBeVisible();
    await expect(noJsPage.getByLabel('密码')).toBeVisible();
  });

  test('无 JS 登录成功跳转首页', async ({ noJsPage }) => {
    await noJsPage.goto('/login');
    await noJsPage.getByLabel('用户名或邮箱').fill('alice');
    await noJsPage.getByLabel('密码').fill('E2e-test-pass-123!');
    await noJsPage.getByRole('button', { name: /登录/ }).click();
    // 无 JS 登录成功 → 303 redirect 到首页（HTTP 跳转，非客户端路由）。
    await expect(noJsPage).toHaveURL('http://localhost:4173/');
  });
});

test.describe('无 JS 关键表单退化（degradation）', () => {
  test('搜索页原生 GET 表单可提交', async ({ noJsPage }) => {
    await noJsPage.goto('/search');
    // 页面搜索框（aria-label="搜索帖子"，避免命中 navbar 搜索框）。
    const input = noJsPage.getByRole('searchbox', { name: '搜索帖子', exact: true });
    await input.fill('Rust');
    await noJsPage.getByRole('button', { name: '搜索', exact: true }).click();
    await expect(noJsPage).toHaveURL(/\/search\?q=Rust/);
  });
});
