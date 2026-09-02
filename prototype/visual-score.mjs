#!/usr/bin/env node
import { chromium } from '/data/projects/bblbb/frontend/node_modules/playwright/index.mjs';

const BASE = (process.env.PROTOTYPE_BASE || 'http://127.0.0.1:8765').replace(/\/$/, '');
const cases = [
  ['板块', '#board:rust'], ['内容详情', '#topic:101'], ['创作', '#publish:article'],
  ['个人中心', '#user:Chaos'], ['设置', '#settings:devices'], ['消息', '#messages'],
  ['通知', '#notifications'], ['商城', '#shop'], ['管理后台', '#admin-users'], ['状态页', '#404']
];
const browser = await chromium.launch();
const results = [];
for (const [viewport, width, height] of [['desktop', 1440, 900], ['mobile', 390, 844]]) {
  const context = await browser.newContext({ viewport: { width, height } });
  const page = await context.newPage();
  await page.goto(BASE + '/', { waitUntil: 'domcontentloaded' });
  await page.evaluate(() => localStorage.setItem('bblbb-prototype-state', JSON.stringify({ auth: true, role: 'admin' })));
  for (const [name, hash] of cases) {
    await page.evaluate((target) => { location.hash = target; }, hash);
    await page.waitForTimeout(90);
    const metric = await page.evaluate(() => {
      const visible = document.querySelector('.page:not([hidden])');
      const visibleH1 = [...document.querySelectorAll('h1')].some((node) => node.getClientRects().length > 0);
      const headers = visible ? visible.querySelectorAll('.app-route-head').length : 0;
      const overflow = document.documentElement.scrollWidth > document.documentElement.clientWidth + 1;
      const badRects = [...(visible?.querySelectorAll('button,a,input,select,textarea') || [])]
        .filter((el) => el.getClientRects().length)
        .filter((el) => { const r = el.getBoundingClientRect(); const ancestor = el.closest('.app-admin-side,.app-settings-nav,.app-table-wrap'); return !ancestor && (r.right > innerWidth + 1 || r.left < -1); }).length;
      const intro = visible?.querySelector('.app-route-intro');
      const hasMeaningfulTitle = visibleH1 || !!visible?.querySelector('.topic-head-card h1,.app-profile h2,.app-state-card h1,.mobile-page-title');
      let score = 100;
      if (!visible) score -= 50;
      if (!hasMeaningfulTitle) score -= 20;
      if (headers) score -= 30;
      if (overflow) score -= 20;
      if (badRects) score -= Math.min(20, badRects * 3);
      return { score: Math.max(0, score), visible: !!visible, title: hasMeaningfulTitle, headers, overflow, badRects, intro: !!intro };
    });
    results.push({ viewport, name, hash, ...metric });
  }
  await context.close();
}
await browser.close();
console.log(JSON.stringify({ results, minimum: Math.min(...results.map((x) => x.score)), passed: results.every((x) => x.score > 90) }, null, 2));
process.exitCode = results.every((x) => x.score > 90) ? 0 : 1;
