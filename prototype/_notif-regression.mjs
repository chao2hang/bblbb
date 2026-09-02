import { chromium } from '../frontend/node_modules/playwright/index.mjs';
const browser = await chromium.launch({ headless: true });
const results = [];
for (const viewport of [{ name: 'desktop', width: 1440, height: 900 }, { name: 'mobile', width: 390, height: 844 }]) {
  const context = await browser.newContext({ viewport: { width: viewport.width, height: viewport.height }, isMobile: viewport.width < 768, hasTouch: viewport.width < 768 });
  const page = await context.newPage();
  const errors = [];
  page.on('console', (message) => { if (message.type() === 'error') errors.push(message.text()); });
  page.on('pageerror', (error) => errors.push('pageerror: ' + error.message));
  await page.goto('http://127.0.0.1:8765/index.html#notifications', { waitUntil: 'networkidle' });
  await page.evaluate(() => localStorage.removeItem('bblbb-notification-read'));
  await page.reload({ waitUntil: 'networkidle' });
  const initial = await page.evaluate(() => ({
    unread: document.querySelectorAll('#page-notifications .notif-item.unread').length,
    badge: document.querySelector('#bell-badge')?.textContent,
    heading: document.querySelector('#notif-unread-count')?.textContent,
    visible: [...document.querySelectorAll('#page-notifications .notif-item')].filter((item) => !item.hidden).length,
    overflow: document.documentElement.scrollWidth > innerWidth,
    brokenIcons: [...document.querySelectorAll('#page-notifications use')].filter((use) => !use.getAttribute('href')?.startsWith('#i-')).length
  }));
  await page.locator('#notif-tab-system').click();
  const system = await page.evaluate(() => ({ visible: [...document.querySelectorAll('#page-notifications .notif-item')].filter((item) => !item.hidden).map((item) => item.dataset.type), selected: document.querySelector('#notif-tab-system')?.getAttribute('aria-selected') }));
  await page.locator('#notif-tab-unread').click();
  const unread = await page.evaluate(() => ({ visible: [...document.querySelectorAll('#page-notifications .notif-item')].filter((item) => !item.hidden).length, selected: document.querySelector('#notif-tab-unread')?.getAttribute('aria-selected') }));
  await page.locator('#notif-tab-all').click();
  await page.locator('#page-notifications .notif-item').first().click();
  const afterRead = await page.evaluate(() => ({ unread: document.querySelectorAll('#page-notifications .notif-item.unread').length, badge: document.querySelector('#bell-badge')?.textContent, stored: localStorage.getItem('bblbb-notification-read') }));
  await page.reload({ waitUntil: 'networkidle' });
  const afterReload = await page.evaluate(() => ({ unread: document.querySelectorAll('#page-notifications .notif-item.unread').length, badge: document.querySelector('#bell-badge')?.textContent }));
  results.push({ viewport: viewport.name, initial, system, unread, afterRead, afterReload, errors });
  await context.close();
}
await browser.close();
console.log(JSON.stringify(results, null, 2));
