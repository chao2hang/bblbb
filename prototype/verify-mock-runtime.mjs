#!/usr/bin/env node
/* BBLBB 当前入口 Mock 状态验收器。
 * 覆盖 localStorage 持久化、前台关键流程与后台新增交互。
 */
import { chromium } from '../frontend/node_modules/playwright/index.mjs';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(fileURLToPath(new URL('.', import.meta.url)));
const BASE = (process.env.PROTOTYPE_BASE || 'http://127.0.0.1:8765/prototype').replace(/\/$/, '');
const OUT = join(ROOT, '.verify', 'mock-' + new Date().toISOString().replace(/[:T]/g, '-').slice(0, 19));
mkdirSync(OUT, { recursive: true });

const results = [];
const errors = [];
const add = (name, pass, detail = {}) => results.push({ name, pass: !!pass, ...detail });
const wait = (page, ms = 120) => page.waitForTimeout(ms);
const visible = (page, selector) => page.locator(selector + ':visible').first();
const state = (page) => page.evaluate(() => JSON.parse(localStorage.getItem('bblbb-prototype-state') || '{}'));

async function go(page, hash) {
  await page.waitForFunction(() => typeof window.route === 'function', null, { timeout: 5000 }).catch(() => {});
  await page.evaluate((target) => { location.hash = target; }, hash);
  await page.waitForFunction(() => {
    const heading = document.querySelector('.page:not([hidden]) h1');
    return !!heading && !!(heading.textContent || '').trim();
  }, null, { timeout: 5000 }).catch(() => {});
  await wait(page);
}

async function login(page) {
  await go(page, '#login');
  await page.locator('[data-auth-user]').fill('admin');
  await page.locator('[data-auth-password]').fill('admin123');
  await page.locator('[data-auth-login] button[type="submit"]').click();
  await page.waitForFunction(() => location.hash === '#home', null, { timeout: 3000 }).catch(() => {});
  await wait(page);
}

async function confirmModal(page, selector = '[data-confirm-publish]') {
  const button = page.locator(selector + ':visible').first();
  if (await button.count()) {
    await button.click();
    await wait(page, 650);
    return true;
  }
  return false;
}

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
page.on('pageerror', (error) => errors.push('pageerror: ' + error.message + '\n' + error.stack));
page.on('console', (message) => { if (message.type() === 'error') errors.push('console: ' + message.text()); });

try {
  await page.goto(BASE + '/', { waitUntil: 'domcontentloaded', timeout: 10000 });
  await page.evaluate(() => { localStorage.clear(); sessionStorage.clear(); });

  await login(page);
  add('login persisted', (await state(page)).auth === true);

  await go(page, '#publish:article');
  await page.locator('[data-publish-title]').fill('验收发布内容');
  await page.locator('[data-publish-board]').selectOption('rust');
  await page.locator('[data-publish-tags]').fill('Rust, 验收');
  await page.locator('[data-publish-body]').fill('用于验证当前 Mock 发布、草稿和详情页流程的正文。');
  await page.locator('[data-save-publish]').click();
  add('draft saved', (await state(page)).draft?.title === '验收发布内容');
  await page.locator('[data-submit-publish]').click();
  add('publish confirmation modal', await visible(page, '[data-confirm-publish]').count() > 0);
  await confirmModal(page);
  const published = await state(page);
  const publishedId = published.publishedTopics?.[0]?.id;
  add('publish persisted and opens detail', !!publishedId && page.url().includes('#topic:' + publishedId));

  await go(page, '#topic:202');
  await page.locator('[data-paid-unlock]').click();
  await confirmModal(page, '[data-confirm-paid]');
  const paidState = await state(page);
  add('paid unlock persisted', paidState.topicUnlocked?.['202'] === true && paidState.balances?.coin === 318);

  await go(page, '#topic:201');
  await page.locator('[data-topic-reply]').fill('验证回复解锁。');
  await page.locator('[data-submit-reply]').click();
  await wait(page, 650);
  add('reply unlock persisted', (await state(page)).topicUnlocked?.['201'] === true);

  await go(page, '#notifications');
  await page.locator('[data-read-all]').click();
  add('notifications read persisted', (await state(page)).notificationsRead === true);

  await go(page, '#messages');
  await page.locator('[data-message-fail]').click();
  await page.locator('[data-chat-input]').fill('失败测试');
  await page.locator('[data-send-message]').click();
  await wait(page, 650);
  const messageState = await state(page);
  add('message failure persisted', (messageState.messages?.Lin || []).some((item) => item.failed === true));

  await go(page, '#mfa');
  await page.locator('[data-mfa-code]').fill('123456');
  await page.locator('[data-enable-mfa]').click();
  await wait(page);
  add('mfa state persisted', (await state(page)).toggles?.mfa === true);

  await go(page, '#admin-reports');
  const reportChecks = page.locator('[data-report-batch]:visible');
  const reportCount = await reportChecks.count();
  for (let i = 0; i < reportCount; i += 1) await reportChecks.nth(i).check();
  await page.locator('[data-report-batch-action="resolved"]:visible').click();
  await confirmModal(page, '[data-adm-confirm]');
  const reportState = await state(page);
  add('admin bulk report closure', reportCount > 0 && Object.values(reportState.reports || {}).filter((item) => item.status).every((item) => item.status === 'resolved'));

  await go(page, '#admin-points');
  await page.locator('[data-adjust-points]').click();
  await page.locator('[data-adjust-amount]').fill('15');
  await page.locator('[data-adjust-reason]').fill('验收调整');
  await page.locator('[data-confirm-adjust]').click();
  await confirmModal(page, '[data-final-adjust]');
  await page.locator('[data-modal-close]:visible').first().click().catch(() => {});
  add('admin points adjustment', (await state(page)).balances?.coin === 333 && (await state(page)).ledger?.some((item) => item.kind === 'admin_adjust' && item.amount === 15));

  await go(page, '#admin-ai');
  const aiBefore = (await state(page)).aiTasks || {};
  const aiTarget = page.locator('[data-adm-act="ai-retry"]:visible').first();
  if (await aiTarget.count()) {
    await aiTarget.click();
    await wait(page, 600);
  }
  const aiAfter = (await state(page)).aiTasks || {};
  add('ai task transition persisted', Object.keys(aiAfter).some((key) => aiAfter[key] === '排队中') || JSON.stringify(aiAfter) !== JSON.stringify(aiBefore));

  await go(page, '#admin-posts');
  const postMaster = page.locator('[data-batch-select-all]:visible').first();
  add('post list batch controls', await postMaster.count() > 0 && await page.locator('[data-batch-select]:visible').count() > 0);
  await postMaster.check();
  add('post batch bar count', await page.locator('[data-batch-count]:visible').innerText() === String(await page.locator('[data-batch-select]:visible').count()));
  await page.locator('[data-batch-clear]:visible').click();
  await page.locator('[data-admin-filter]:visible').fill('不存在的帖子');
  await wait(page, 350);
  add('admin filter empty state', await page.locator('[data-admin-filter-empty]:visible').count() > 0);
  await page.locator('[data-admin-filter-clear]:visible').click();

  await go(page, '#admin-users');
  add('user list batch controls', await page.locator('[data-batch-select-all]:visible').count() > 0 && await page.locator('[data-batch-select]:visible').count() > 0);
  await page.locator('[data-adm-act="us-view"]:visible').first().click();
  add('user detail drawer', await page.locator('.app-drawer-backdrop:visible').count() > 0 && await page.locator('.app-drawer:visible').innerText().then((text) => text.includes('用户详情')));
  await page.locator('[data-drawer-close]:visible').first().click();

  await go(page, '#admin-article-audit:p-201');
  add('article audit diff route', page.url().includes('#admin-article-audit:p-201') && await page.locator('.app-diff:visible').count() > 0 && await page.locator('.app-diff__grid:visible').count() > 0);

  await go(page, '#admin-bi');
  await page.locator('[data-bi-metrics]').waitFor({ state: 'visible', timeout: 5000 });
  const biRendered = await page.waitForFunction(() => !!document.querySelector('.app-admin-main [data-bi-tab="今年"]'), null, { timeout: 5000 }).then(() => true).catch(() => false);
  add('BI renderer ready', biRendered, { url: page.url(), page: await page.locator('.page:not([hidden])').first().getAttribute('id').catch(() => null), body: (await page.locator('body').innerText()).slice(-120) });
  const biBefore = await page.locator('[data-bi-metrics]').innerText();
  const biYear = page.locator('.app-admin-main [data-bi-tab="今年"]');
  let biYearClicked = false;
  for (let i = 0; i < await biYear.count(); i += 1) {
    if (await biYear.nth(i).evaluate((el) => !!(el.offsetWidth || el.offsetHeight || el.getClientRects().length))) {
      await biYear.nth(i).click({ force: true });
      biYearClicked = true;
      break;
    }
  }
  add('BI yearly tab available', biYearClicked);
  await page.locator('[data-bi-metrics]').waitFor({ state: 'visible', timeout: 5000 });
  add('BI yearly period', (await state(page)).biPeriod === '今年' && (await page.locator('[data-bi-metrics]').innerText()) !== biBefore);

  await go(page, '#admin');
  const menu = page.locator('[data-admin-menu]').first();
  await page.setViewportSize({ width: 390, height: 900 });
  add('mobile admin menu control exists', await menu.count() > 0);
  await menu.click();
  add('mobile admin menu opens', await page.locator('.app-admin-shell.admin-menu-open:visible').count() > 0);

  const report = { base: BASE, results, errors };
  writeFileSync(join(OUT, 'report.json'), JSON.stringify(report, null, 2));
  const failed = results.filter((item) => !item.pass);
  console.log('RESULT ' + (results.length - failed.length) + '/' + results.length + ' failed ' + failed.length + ' errors ' + errors.length + ' report ' + join(OUT, 'report.json'));
  if (failed.length) console.log(failed.map((item) => 'FAIL ' + item.name + ' ' + JSON.stringify(item.detail || '')).join('\n'));
  process.exitCode = failed.length || errors.length ? 1 : 0;
} finally {
  await browser.close();
}
