// 后台 25 页移动端（390x844）最新对比截图
// 前端：https://127.0.0.1:5173 (0.0.0.0:5173，真实开发栈)
// 原型：http://127.0.0.1:8765
import { chromium } from '@playwright/test';
import { mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execSync } from 'node:child_process';

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO = join(__dirname, '..', '..', '..');
const OUT_PROTO = join(REPO, 'reports', 'mobile-compare', 'admin-proto');
const OUT_APP = join(REPO, 'reports', 'mobile-compare', 'admin-app');

mkdirSync(OUT_PROTO, { recursive: true });
mkdirSync(OUT_APP, { recursive: true });

// 确保测试会话有效
execSync(`python3 -c "
import sqlite3, time
c = sqlite3.connect('${join(REPO, 'data', 'bblbb.sqlite')}')
now = int(time.time() * 1000)
c.execute('UPDATE user_sessions SET idle_expires_at = ?, last_seen_at = ?, revoked_at = NULL', (now + 86400000, now))
c.commit()
"`);

const ADMIN_PAGES = [
  ['dashboard', '/admin', 'admin', '仪表盘'],
  ['posts', '/admin/posts', 'admin-posts', '帖子与回复'],
  ['boards', '/admin/boards', 'admin-boards', '板块管理'],
  ['tags', '/admin/tags', 'admin-tags', '标签管理'],
  ['attachments', '/admin/attachments', 'admin-attachments', '附件管理'],
  ['users', '/admin/users', 'admin-users', '用户管理'],
  ['roles', '/admin/roles', 'admin-roles', '角色与权限'],
  ['reports', '/admin/moderation/cases', 'admin-reports', '举报与审核'],
  ['report-detail', '/admin/moderation/cases/R-1024', 'admin-report:R-1024', '案件详情'],
  ['content', '/admin/content', 'admin-article-audit', '内容审核'],
  ['points', '/admin/points', 'admin-points', '积分与货币'],
  ['levels', '/admin/levels', 'admin-levels', '等级管理'],
  ['achievements', '/admin/achievements', 'admin-achievements', '成就管理'],
  ['themes', '/admin/themes', 'admin-themes', '主题管理'],
  ['plugins', '/admin/plugins', 'admin-plugins', '插件管理'],
  ['oauth', '/admin/oauth', 'admin-oauth', 'OAuth 客户端'],
  ['storage', '/admin/storage', 'admin-storage', '文件存储'],
  ['download-billing', '/admin/download-billing', 'admin-download-billing', '下载计费'],
  ['ai', '/admin/ai', 'admin-ai', '大模型设置'],
  ['video', '/admin/video', 'admin-video', '视频插件'],
  ['marketplace', '/admin/marketplace', 'admin-marketplace', '市场与交易'],
  ['bi', '/admin/bi', 'admin-bi', 'BI 数据看板'],
  ['notifications', '/admin/notifications', 'admin-notifications', '通知与邮件'],
  ['audit', '/admin/audit', 'admin-audit', '审计日志'],
  ['settings', '/admin/settings', 'admin-settings', '系统设置']
];

const MOBILE = {
  viewport: { width: 390, height: 844 },
  deviceScaleFactor: 2,
  hasTouch: true,
  isMobile: true,
  locale: 'zh-CN',
  timezoneId: 'Asia/Shanghai'
};

const browser = await chromium.launch();

// 1. 原型 Context
const protoCtx = await browser.newContext(MOBILE);
await protoCtx.addInitScript(() => {
  try {
    localStorage.setItem('bblbb-theme', 'light');
    localStorage.setItem('bblbb-prototype-state', JSON.stringify({ auth: true, role: 'admin' }));
  } catch {}
});

// 2. 前端 Context (使用真实 5173 HTTPS，忽略自签证书，注入已知 admin session)
const appCtx = await browser.newContext({
  ...MOBILE,
  ignoreHTTPSErrors: true
});
await appCtx.addInitScript(() => {
  try {
    localStorage.setItem('theme', 'light');
    localStorage.setItem('bblbb-theme', 'light');
  } catch {}
});
await appCtx.addCookies([
  {
    name: '__Host-bblbb_session',
    value: 'admin_chaos_session_test_token_2026',
    url: 'https://127.0.0.1:5173/'
  }
]);

console.log('开始抓取 25 个后台页面...');

for (const [name, fePath, protoRoute, label] of ADMIN_PAGES) {
  // 原型截图
  try {
    const page = await protoCtx.newPage();
    await page.goto(`http://127.0.0.1:8765/#${protoRoute}`, { waitUntil: 'domcontentloaded', timeout: 30000 });
    await page.waitForTimeout(1000);
    await page.screenshot({ path: join(OUT_PROTO, `${name}.png`), fullPage: true });
    await page.close();
  } catch (e) {
    console.error(`PROTO FAIL [${name}]:`, e.message);
  }

  // 前端截图
  try {
    const page = await appCtx.newPage();
    await page.goto(`https://127.0.0.1:5173${fePath}`, { waitUntil: 'domcontentloaded', timeout: 30000 });
    await page.waitForLoadState('networkidle', { timeout: 10000 }).catch(() => {});
    await page.waitForTimeout(1000);
    await page.screenshot({ path: join(OUT_APP, `${name}.png`), fullPage: true });
    await page.close();
  } catch (e) {
    console.error(`APP FAIL [${name}]:`, e.message);
  }

  console.log(`done: ${name} (${label})`);
}

await browser.close();
console.log('全部 25 页抓取完毕！');
