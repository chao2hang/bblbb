// 一次性视觉验证：ReactionBar 弹窗配色（暗色/亮色模式）
// 用法：node scripts/verify-reaction-popup.mjs [dark|light]
// 前置：
//   1) 本地 dev 前端跑在 https://localhost:5199（vite dev --port 5199）；
//   2) /tmp/bblbb-verify-token 内为 "TOKEN=<base64url>"，对应 user_sessions 中
//      已铸发的会话（参考 tests/playwright/fixtures/seed-personas.mjs mintSession，
//      token_hash = sha256(token) hex，用完即删）。
import { chromium } from '@playwright/test';
import { readFileSync } from 'node:fs';

const TOKEN = readFileSync('/tmp/bblbb-verify-token', 'utf8').split('=')[1].trim();
const POST_ID = '01a06567-6786-76c0-a9bb-16d965d6c77d';
const BASE = 'https://localhost:5199';
const MODE = process.argv[2] === 'light' ? 'light' : 'dark';
const OUT = `/tmp/rx-verify-${MODE}`;

const browser = await chromium.launch();
const context = await browser.newContext({
  ignoreHTTPSErrors: true,
  colorScheme: MODE,
  viewport: { width: 1440, height: 900 }
});
await context.addCookies([
  {
    name: '__Host-bblbb_session',
    value: TOKEN,
    domain: 'localhost',
    path: '/',
    secure: true,
    sameSite: 'Lax'
  }
]);

const page = await context.newPage();
await page.goto(`${BASE}/posts/${POST_ID}`, { waitUntil: 'networkidle' });
await page.waitForTimeout(1200);

const theme = await page.evaluate(() => ({
  cls: document.documentElement.className,
  attr: document.documentElement.dataset.theme
}));
console.log('THEME=' + JSON.stringify(theme));

// 1) 反应明细弹窗
const pill = page.locator('.rx-pill').first();
if (await pill.count() === 0) {
  console.log('NO_PILL_FOUND');
  await page.screenshot({ path: `${OUT}-nopill.png`, fullPage: false });
} else {
  await pill.click();
  await page.waitForSelector('.reaction-detail-popover', { timeout: 5000 });
  await page.waitForTimeout(500);
  console.log('POPOVER_VISIBLE=true');
  await page.screenshot({ path: `${OUT}-detail.png` });

  const colors = await page.evaluate(() => {
    const pick = (sel, prop) => {
      const el = document.querySelector(sel);
      return el ? getComputedStyle(el)[prop] : null;
    };
    return {
      popBg: pick('.reaction-detail-popover', 'backgroundColor'),
      popBorder: pick('.reaction-detail-popover', 'borderColor'),
      activeTabBg: pick('.rx-popover-tab.is-active', 'backgroundColor'),
      activeTabColor: pick('.rx-popover-tab.is-active', 'color'),
      tabColor: pick('.rx-popover-tab:not(.is-active)', 'color'),
      pillBg: pick('.rx-pill', 'backgroundColor'),
      pillBorder: pick('.rx-pill', 'borderColor'),
      userName: pick('.rx-user-display-name', 'color'),
      userHandle: pick('.rx-user-handle', 'color'),
      pageBg: getComputedStyle(document.body).backgroundColor
    };
  });
  console.log('DETAIL_COLORS=' + JSON.stringify(colors));

  // 2) 表情选择器弹窗
  await page.keyboard.press('Escape');
  await page.waitForTimeout(400);
  const addBtn = page.locator('.reaction-add-btn').first();
  await addBtn.click();
  await page.waitForSelector('.reaction-picker-popover', { timeout: 5000 });
  await page.waitForTimeout(400);
  console.log('PICKER_VISIBLE=true');
  await page.screenshot({ path: `${OUT}-picker.png` });

  const pickerColors = await page.evaluate(() => {
    const pick = (sel, prop) => {
      const el = document.querySelector(sel);
      return el ? getComputedStyle(el)[prop] : null;
    };
    return {
      pickerBg: pick('.reaction-picker-popover', 'backgroundColor'),
      pickerBorder: pick('.reaction-picker-popover', 'borderColor'),
      itemHoverFallback: pick('.reaction-picker-item', 'backgroundColor'),
      addBtnColor: pick('.reaction-add-btn', 'color')
    };
  });
  console.log('PICKER_COLORS=' + JSON.stringify(pickerColors));
}

await browser.close();
