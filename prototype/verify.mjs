#!/usr/bin/env node
/* BBLBB 单入口 Hash SPA 验收器
 * 普通模式验证路由、守卫、核心交互、响应式和错误；--shots-only 额外保存逐路由截图。
 */
import { chromium } from '../frontend/node_modules/playwright/index.mjs';
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = dirname(fileURLToPath(import.meta.url));
const BASE = (process.env.PROTOTYPE_BASE || 'http://127.0.0.1:8765').replace(/\/$/, '');
const SHOTS_ONLY = process.argv.includes('--shots-only');
const OUT_DIR = join(ROOT, '.verify', 'spa-' + new Date().toISOString().replace(/[:T]/g, '-').slice(0, 19));
mkdirSync(OUT_DIR, { recursive: true });

const ROUTES = [
  '#home', '#discover', '#articles', '#boards', '#board:rust', '#board:web-dev', '#tags', '#tag:Rust',
  '#topic:101', '#topic:201', '#topic:202', '#thread:welcome', '#publish', '#publish:article', '#publish:topic',
  '#user:Chaos', '#user:Nina', '#favorites', '#settings:profile', '#settings:security', '#settings:devices',
  '#settings:notifications', '#settings:oauth', '#settings:privacy', '#register', '#forgot-password', '#search',
  '#notifications', '#shop', '#achievements', '#billing', '#appeals', '#mfa', '#messages', '#market', '#checkout',
  '#purchases', '#apikeys', '#admin', '#admin-reports', '#admin-report:R-1024', '#admin-points', '#admin-levels',
  '#admin-achievements', '#admin-themes', '#admin-plugins', '#admin-oauth', '#admin-storage', '#admin-download-billing',
  '#admin-ai', '#admin-video', '#admin-marketplace', '#admin-bi', '#admin-users', '#admin-roles', '#admin-boards',
  '#admin-posts', '#admin-tags', '#admin-attachments', '#admin-notifications', '#admin-audit', '#admin-settings',
  '#403', '#404', '#429', '#error'
];
const PROTECTED = new Set([
  '#publish', '#publish:article', '#publish:topic', '#favorites', '#settings:profile', '#settings:security',
  '#settings:devices', '#settings:notifications', '#settings:oauth', '#settings:privacy', '#notifications', '#shop',
  '#achievements', '#billing', '#appeals', '#mfa', '#messages', '#checkout', '#purchases', '#apikeys', '#admin',
  '#admin-reports', '#admin-report:R-1024', '#admin-points', '#admin-levels', '#admin-achievements', '#admin-themes',
  '#admin-plugins', '#admin-oauth', '#admin-storage', '#admin-download-billing', '#admin-ai', '#admin-video',
  '#admin-marketplace', '#admin-bi', '#admin-users', '#admin-roles', '#admin-boards', '#admin-posts', '#admin-tags',
  '#admin-attachments', '#admin-notifications', '#admin-audit', '#admin-settings'
]);
const EXACT_HREFS = new Set(ROUTES.concat(['#login', '#home', '#discover', '#design', '#drafts', '#loading', '#main']));
const results = [];
const errors = { console: [], pageerror: [], requestfailed: [] };
const text = (value) => String(value || '').replace(/\s+/g, ' ').trim();
const add = (name, pass, detail = {}) => results.push({ name, pass: !!pass, ...detail });
const sleep = (page, ms = 100) => page.waitForTimeout(ms);

async function ensureRoot(page) {
  if (!(await page.url()).startsWith(BASE + '/')) {
    await page.goto(BASE + '/', { waitUntil: 'domcontentloaded', timeout: 10000 });
  }
}
async function settle(page) { await sleep(page, 120); await page.locator('.page:not([hidden]) h1').first().textContent({ timeout: 900 }).catch(() => ''); }
async function go(page, hash) {
  await ensureRoot(page);
  const same = await page.evaluate((target) => location.hash === target, hash);
  if (same) {
    await page.evaluate(() => { if (typeof window.route === 'function') window.route(); });
  } else {
    await page.evaluate((target) => { location.hash = target; }, hash);
  }
  await settle(page);
}
async function visibleH1(page) {
  return text(await page.locator('.page:not([hidden]) h1').first().textContent({ timeout: 3000 }).catch(() => ''));
}
function expectedPageId(hash) {
  const base = hash.slice(1).split(':')[0];
  if (base === 'thread') return 'page-topic';
  return 'page-' + base;
}
async function visiblePage(page) {
  return await page.locator('.page:not([hidden])').first().getAttribute('id').catch(() => null);
}
async function fillFirst(page, selectors, value) {
  for (const selector of selectors) {
    const locator = page.locator(selector).first();
    if (await locator.count()) { await locator.fill(value); return true; }
  }
  return false;
}
async function clearBrowserState(page) {
  await page.evaluate(() => { localStorage.clear(); sessionStorage.clear(); });
}
function safeFileName(hash) { return hash.slice(1).replace(/[^\\w-]+/g, '_') || 'home'; }
function isDynamicHash(href) {
  return /^#(?:board|tag|topic|thread|publish|user|settings|notifications|admin-report):/.test(href || '');
}
function isKnownHash(href) {
  return EXACT_HREFS.has(href) || isDynamicHash(href) || href === '#billing' || href === '#appeals' || href === '#mfa' || href === '#messages' || href === '#market' || href === '#checkout' || href === '#purchases' || href === '#apikeys';
}

(async () => {
  const browser = await chromium.launch();
  const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const page = await context.newPage();
  page.on('console', (message) => { if (message.type() === 'error' || message.type() === 'warning') errors.console.push(message.type() + ': ' + message.text()); });
  page.on('pageerror', (error) => errors.pageerror.push(error.message));
  page.on('requestfailed', (request) => errors.requestfailed.push(request.url() + ' — ' + (request.failure()?.errorText || 'failed')));

  try {
    await page.goto(BASE + '/', { waitUntil: 'domcontentloaded', timeout: 10000 });
    await clearBrowserState(page);
    await go(page, '#home');
    console.log('=== Hash SPA 路由验收 ===');
    for (const hash of ROUTES) {
      try {
        await go(page, hash);
        const h1 = await visibleH1(page);
        const pageId = await visiblePage(page);
        const expectedLogin = PROTECTED.has(hash);
        const expectedId = expectedLogin ? 'page-login' : expectedPageId(hash);
        const pass = !!pageId && !!h1 && pageId === expectedId;
        add('route ' + hash, pass, { hash, page: pageId, h1, expected: expectedId });
        if (SHOTS_ONLY) await page.screenshot({ path: join(OUT_DIR, safeFileName(hash) + '.png'), fullPage: true });
      } catch (error) {
        add('route ' + hash, false, { hash, error: String(error).slice(0, 240) });
      }
    }

    if (!SHOTS_ONLY) {
      console.log('=== 核心结构检查 ===');
      await clearBrowserState(page);
      await go(page, '#home');
      const links = await page.locator('a[href]:visible').evaluateAll((anchors) => anchors.map((anchor) => ({ href: anchor.getAttribute('href'), label: (anchor.textContent || '').replace(/\s+/g, ' ').trim() })));
      const badFormat = links.filter((item) => !item.href || (!item.href.startsWith('#') && !/^https?:\/\//.test(item.href) && !item.href.startsWith('mailto:')));
      const unknownHashes = [...new Set(links.map((item) => item.href).filter((href) => href?.startsWith('#') && href !== '#'))].filter((href) => !isKnownHash(href));
      add('core href format', badFormat.length === 0, { bad: badFormat.slice(0, 10) });
      add('core hash hrefs known', unknownHashes.length === 0, { unknown: unknownHashes });
      const overflow = await page.evaluate(() => ({ scrollWidth: document.documentElement.scrollWidth, clientWidth: document.documentElement.clientWidth }));
      add('no horizontal overflow at 1440', overflow.scrollWidth <= overflow.clientWidth + 1, overflow);
      const unnamed = await page.locator('button:visible').evaluateAll((buttons) => buttons.map((button) => ({ label: (button.textContent || '').replace(/\s+/g, ' ').trim(), aria: button.getAttribute('aria-label'), title: button.getAttribute('title') })).filter((item) => !item.label && !item.aria && !item.title));
      add('visible buttons accessible', unnamed.length === 0, { unnamed: unnamed.slice(0, 20) });

      console.log('=== 未登录守卫与 returnTo ===');
      for (const hash of PROTECTED) {
        await clearBrowserState(page);
        await go(page, hash);
        const finalHash = '#' + (page.url().split('#')[1] || '');
        const returnTo = await page.evaluate(() => localStorage.getItem('returnTo') || sessionStorage.getItem('returnTo') || '');
        add('auth guard ' + hash, finalHash === '#login', { expected: '#login', actual: finalHash });
        add('returnTo ' + hash, returnTo === hash, { expected: hash, actual: returnTo });
      }

      console.log('=== 核心旅程 ===');
      await clearBrowserState(page);
      await go(page, '#login');
      await fillFirst(page, ['#login-username', '[data-auth-user]'], 'admin');
      await fillFirst(page, ['#login-password', '[data-auth-password]'], 'admin123');
      const loginButton = page.locator('form:visible button[type="submit"]').first();
      if (await loginButton.count()) await loginButton.click();
      await page.waitForFunction(() => location.hash === '#home', { timeout: 3000 }).catch(() => {});
      add('login journey', page.url().endsWith('#home'), { url: page.url() });

      await clearBrowserState(page);
      await go(page, '#home');
      await go(page, '#settings:devices');
      const protectedReturn = await page.evaluate(() => localStorage.getItem('returnTo') || '');
      await fillFirst(page, ['#login-username', '[data-auth-user]'], 'admin');
      await fillFirst(page, ['#login-password', '[data-auth-password]'], 'admin123');
      const returnLoginButton = page.locator('form:visible button[type="submit"]').first();
      if (await returnLoginButton.count()) await returnLoginButton.click();
      await settle(page);
      add('login returns to protected route', page.url().endsWith('#settings:devices'), { before: protectedReturn, url: page.url() });

      await go(page, '#register');
      add('register page', /创建账号|注册/.test((await visibleH1(page)) + ' ' + text(await page.locator('body').innerText())));
      await go(page, '#forgot-password');
      add('forgot password page', /重置密码|找回/.test((await visibleH1(page)) + ' ' + text(await page.locator('body').innerText())));

      await go(page, '#publish:article');
      const publishBefore = page.url();
      const publishButton = page.locator('button:visible').filter({ hasText: /立即发布|提交审核/ }).first();
      if (await publishButton.count()) await publishButton.click();
      await settle(page);
      add('publish empty validation', page.url() === publishBefore && await page.locator('[data-publish-summary]:visible, [data-title-error]:visible, .app-error:visible').count() > 0);

      await go(page, '#board:rust');
      const boardSearch = page.locator('[data-board-search]:visible').first();
      if (await boardSearch.count()) { await boardSearch.fill('不存在的内容'); await settle(page); }
      add('board filter empty state', await page.locator('[data-board-empty]:visible').count() > 0);

      await clearBrowserState(page);
      await go(page, '#topic:201');
      add('restricted content stays out of DOM', await page.locator('[data-restricted]:visible').count() > 0 && await page.locator('[data-unlocked-content]').count() === 0);

      await go(page, '#topic:202');
      add('payment affordance', /支付|解锁|B币/.test(text(await page.locator('body').innerText())));

      await clearBrowserState(page);
      await go(page, '#login');
      await fillFirst(page, ['#login-username', '[data-auth-user]'], 'admin');
      await fillFirst(page, ['#login-password', '[data-auth-password]'], 'admin123');
      const adminLogin = page.locator('form:visible button[type="submit"]').first();
      if (await adminLogin.count()) await adminLogin.click();
      await settle(page);
      await go(page, '#admin-reports');
      add('reports page', /举报|审核/.test(text(await page.locator('body').innerText())));
      await go(page, '#admin-report:R-1024');
      add('report detail page', /R-1024|处理|处罚/.test(text(await page.locator('body').innerText())));

      await go(page, '#admin');
      const switchRole = page.locator('[data-switch-role]:visible').first();
      if (await switchRole.count()) await switchRole.click();
      await go(page, '#admin-users');
      add('non-admin reaches 403', page.url().endsWith('#403') && /访问权限|管理员/.test(text(await page.locator('body').innerText())));

      for (const hash of ['#403', '#404', '#429', '#error']) {
        await go(page, hash);
        add('state page ' + hash, !!(await visibleH1(page)));
      }
      await go(page, '#route-that-does-not-exist');
      add('unknown hash falls to 404', page.url().endsWith('#404') && /页面不存在/.test(text(await page.locator('body').innerText())));

      console.log('=== 四视口亮暗回归 ===');
      for (const colorScheme of ['light', 'dark']) {
        for (const width of [1440, 1024, 768, 390]) {
          let smallContext;
          let smallPage;
          try {
            smallContext = await browser.newContext({ viewport: { width, height: 900 }, colorScheme });
            smallPage = await smallContext.newPage();
          const localErrors = [];
          smallPage.on('pageerror', (error) => localErrors.push(error.message));
          await smallPage.goto(BASE + '/', { waitUntil: 'domcontentloaded', timeout: 10000 });
          await smallPage.evaluate((theme) => { localStorage.clear(); sessionStorage.clear(); localStorage.setItem('bblbb-prototype-state', JSON.stringify({ auth: true, role: 'admin' })); document.documentElement.dataset.theme = theme; }, colorScheme);
          await smallPage.evaluate(() => { location.hash = '#home'; });
          await sleep(smallPage, 30);
          for (const hash of ['#home', '#discover', '#articles', '#boards', '#board:rust', '#topic:201', '#topic:202', '#publish:article', '#settings:devices', '#messages', '#notifications', '#shop', '#market', '#checkout', '#purchases', '#admin-users', '#register', '#404']) {
            try {
              await go(smallPage, hash);
              const metrics = await smallPage.evaluate(() => ({ scrollWidth: document.documentElement.scrollWidth, clientWidth: document.documentElement.clientWidth, visiblePages: document.querySelectorAll('.page:not([hidden])').length }));
              if (metrics.scrollWidth > metrics.clientWidth + 1 || metrics.visiblePages !== 1) add('responsive ' + colorScheme + ' ' + width + ' ' + hash, false, metrics);
            } catch (error) {
              localErrors.push('route ' + hash + ': ' + String(error));
              add('responsive ' + colorScheme + ' ' + width + ' ' + hash, false, { error: String(error) });
              break;
            }
          }
          add('responsive ' + colorScheme + ' ' + width, localErrors.length === 0, { errors: localErrors });
          await smallContext.close().catch(() => {});
          } catch (error) {
            add('responsive ' + colorScheme + ' ' + width, false, { error: String(error) });
            if (smallContext) await smallContext.close().catch(() => {});
          }
        }
      }
    }
  } finally {
    const unique = (items) => [...new Set(items)];
    const uniqueErrors = Object.fromEntries(Object.entries(errors).map(([key, values]) => [key, unique(values)]));
    writeFileSync(join(OUT_DIR, 'report.json'), JSON.stringify({ base: BASE, routes: ROUTES, results, errors: uniqueErrors }, null, 2));
    const failed = results.filter((result) => !result.pass);
    console.log('console: ' + (uniqueErrors.console.length ? uniqueErrors.console.slice(0, 20).join(' | ') : '(无)'));
    console.log('pageerror: ' + (uniqueErrors.pageerror.length ? uniqueErrors.pageerror.slice(0, 20).join(' | ') : '(无)'));
    console.log('requestfailed: ' + (uniqueErrors.requestfailed.length ? uniqueErrors.requestfailed.slice(0, 20).join(' | ') : '(无)'));
    console.log('结果：' + (results.length - failed.length) + '/' + results.length + ' 通过，失败 ' + failed.length + '；报告：' + join(OUT_DIR, 'report.json'));
    await browser.close();
    process.exitCode = failed.length ? 1 : 0;
  }
})();
