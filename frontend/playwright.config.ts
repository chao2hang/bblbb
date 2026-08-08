// M14-A11Y-01：Playwright 配置 —— desktop/mobile 双项目。
//
// - webServer 由 fixtures/serve.mjs 编排：真实 Rust 后端（--migrate e2e 库）
//   + persona 铸种 + vite dev（/api 代理到后端）；
// - desktop-chromium：桌面基准（viewport 1280x720）；
// - mobile-chromium：Galaxy S9 触屏模拟（392x783、touch、hasTouch、deviceScaleFactor）；
// - 全量串行（workers=1，fullyParallel=false）：persona 数据在共享后端上变更，
//   串行避免跨用例互踩；axe/键盘/无 JS 用例在同一 session 顺序执行；
// - 时钟：e2e 库由 seed 脚本铸造（created_at 固定），Playwright clock 在
//   keyboard-focus 用例中以 clock.install 冻结时间，保证 TOTP/冷却窗口稳定。
import { defineConfig, devices } from '@playwright/test';

const PORT = 4173;
const BASE_URL = `http://localhost:${PORT}`;

export default defineConfig({
  testDir: './tests/playwright',
  globalSetup: './tests/playwright/global-setup.ts',
  fullyParallel: false,
  workers: 1,
  timeout: 60_000,
  expect: { timeout: 10_000 },
  retries: process.env.CI ? 1 : 0,
  reporter: [
    ['list'],
    ['json', { outputFile: 'tests/a11y/playwright-results.json' }]
  ],
  use: {
    baseURL: BASE_URL,
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    locale: 'zh-CN',
    timezoneId: 'Asia/Shanghai'
  },
  webServer: {
    command: 'node tests/playwright/fixtures/serve.mjs',
    url: BASE_URL,
    reuseExistingServer: !process.env.CI,
    timeout: 120_000
  },
  projects: [
    {
      name: 'desktop-chromium',
      use: { ...devices['Desktop Chrome'], viewport: { width: 1280, height: 720 } }
    },
    {
      name: 'mobile-chromium',
      use: {
        ...devices['Galaxy S9'],
        // 桌面用例默认在 desktop 项目跑；mobile 项目聚焦触屏/窄屏用例。
        viewport: { width: 360, height: 740 }
      }
    }
  ]
});
