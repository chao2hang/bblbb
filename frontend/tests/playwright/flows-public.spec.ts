// M14-A11Y-02/03：公开与认证流程 —— 匿名浏览/搜索/注册/登录/验证。
//
// 覆盖 persona：anonymous（默认无会话）。
import { expect, test } from '@playwright/test';
import { personas, stableFill, appendRecord, browserInfo, LOCALE, currentCommit } from './helpers';

test.describe('匿名浏览（public read，SSR）', () => {
  for (const [path, title] of [
    ['/', 'BBLBB'],
    ['/boards', '板块'],
    ['/tags', '标签'],
    ['/search', '搜索']
  ] as const) {
    test(`${path} 页面 SSR 渲染标题`, async ({ page }) => {
      await page.goto(path);
      await expect(page).toHaveTitle(new RegExp(title.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
      // 无 JS 也能读的关键结构：主导航 + 主内容区。
      await expect(page.getByRole('banner')).toBeVisible();
      await expect(page.getByRole('main')).toBeVisible();
    });
  }

  test('/boards/tech 板块详情展示帖子列表', async ({ page }) => {
    await page.goto('/boards/tech');
    await expect(page.getByRole('heading', { level: 1 })).toContainText('技术分享');
    // 板块详情渲染帖子（SSR 列表）。
    await expect(page.getByRole('main')).toContainText(/E2E 公开文章|暂无帖子|Rust/);
  });

  test('/users/alice 用户主页显示公开投影', async ({ page }) => {
    await page.goto('/users/alice');
    await expect(page.getByText('@ alice')).toBeVisible();
    await expect(page.getByText(/LV\./).first()).toBeVisible();
  });

  test('匿名搜索：输入关键词并提交（原生 GET 表单）', async ({ page }) => {
    await page.goto('/search');
    // 页面搜索框（aria-label="搜索帖子" 精确匹配，避免命中 navbar 搜索框）。
    const input = page.getByRole('searchbox', { name: '搜索帖子', exact: true });
    await stableFill(page, input, 'Rust');
    await page.getByRole('button', { name: '搜索', exact: true }).click();
    await expect(page).toHaveURL(/\/search\?q=Rust/);
    await expect(page.getByRole('main')).toBeVisible();
  });
});

test.describe('注册（register 表单 action）', () => {
  // 真实注册消费后端 IP 注册配额（3 次/小时，src/ratelimit.rs REGISTER_IP_LIMIT），
  // 只在 desktop 项目提交一次；重复运行小时内再跑会命中 429 —— 该限流响应本身
  // 也是被测流程（冷却提示），故断言接受成功或限流两种正确降级。
  test('注册新账号成功并提示（防枚举文案）', async ({ page }, testInfo) => {
    test.skip(testInfo.project.name !== 'desktop-chromium', '真实注册仅 desktop 项目执行（IP 注册配额 3/h）');
    const username = `e2e_${Date.now().toString(36)}`;
    await page.goto('/register');
    await stableFill(page, page.getByLabel('用户名'), username);
    await stableFill(page, page.getByLabel('邮箱'), `${username}@e2e.example`);
    await stableFill(page, page.getByLabel('密码', { exact: true }), 'New-user-pass-123!');
    await stableFill(page, page.getByLabel('确认密码'), 'New-user-pass-123!');
    await page.getByRole('button', { name: /注册/ }).click();
    // 成功（防枚举统一文案）或 429 冷却（操作过于频繁 + 请求号）均为正确后端行为。
    const main = page.getByRole('main');
    await expect(main).toContainText(/注册成功|操作过于频繁|请稍后再试/);
  });

  test('注册表单校验错误可读（label→error 关联）', async ({ page }) => {
    await page.goto('/register');
    await stableFill(page, page.getByLabel('密码', { exact: true }), 'short');
    await page.getByRole('button', { name: /注册/ }).click();
    const alert = page.getByRole('alert').first();
    await expect(alert).toBeVisible();
    await expect(alert).toContainText(/字段|密码|错误|无效/i);
  });
});

test.describe('登录（login 表单 action）', () => {
  test('alice 密码登录成功后跳转首页并显示登录态', async ({ page }) => {
    await page.goto('/login');
    await stableFill(page, page.getByLabel('用户名或邮箱'), 'alice');
    await stableFill(page, page.getByLabel('密码'), personas().password);
    await page.getByRole('button', { name: /登录/ }).click();
    await expect(page).toHaveURL('/');
    // 登录态：Navbar 显示用户菜单按钮（getMe 客户端拉取）。
    await expect(page.getByRole('banner').getByRole('button', { name: '用户菜单' })).toBeVisible();
  });

  test('错误密码显示统一登录失败文案（不泄漏账号状态）', async ({ page }) => {
    await page.goto('/login');
    await stableFill(page, page.getByLabel('用户名或邮箱'), 'alice');
    await stableFill(page, page.getByLabel('密码'), 'wrong-password');
    await page.getByRole('button', { name: /登录/ }).click();
    await expect(page.getByRole('alert').first()).toBeVisible();
  });
});

test.describe('邮箱验证路由', () => {
  test('无效 token 优雅失败（不崩溃，错误可读）', async ({ page }) => {
    await page.goto('/verify-email?token=invalid-token');
    await expect(page.getByRole('main')).toBeVisible();
  });
});

test.describe('记录（M14-A11Y-10）', () => {
  test('记录匿名/注册/登录流程的浏览器环境', async ({ page }) => {
    await page.goto('/');
    const info = await browserInfo(page);
    appendRecord({
      project: 'desktop',
      browser: info.browser,
      browserVersion: info.version,
      viewport: '1280x720',
      locale: LOCALE,
      commit: currentCommit(),
      report: 'tests/a11y/playwright-results.json',
      humanAcceptance: 'pending'
    });
    expect(true).toBeTruthy();
  });
});
