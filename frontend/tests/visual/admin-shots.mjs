// 后台视觉检测：批量截图前端 admin 路由与原型 admin-*.html 页。
// - 复用 tests/playwright/fixtures/serve.mjs 启动的真实栈（vite 4173 / 后端 8080，
//   persona 会话见 fixtures/personas.json）；
// - 原型走 prototype serve.mjs（8765）的独立页直开（page-chrome 注入共享骨架）；
// - 输出 tests/visual/shots/fe-<name>.png 与 proto-<name>.png（1280x720，fullPage）。
import { mkdirSync, rmSync, readFileSync, existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from '@playwright/test';

const __dirname = dirname(fileURLToPath(import.meta.url));
const OUT = join(__dirname, process.env.THEME === 'dark' ? 'shots-dark' : 'shots');
const BASE = 'http://localhost:4174';
const PROTO = 'http://127.0.0.1:8765';

// [输出名, 前端路径, 原型页（null = 原型无对应页）]
const PAIRS = [
  ['dashboard', '/admin', 'admin'],
  ['posts', '/admin/posts', 'admin-posts'],
  ['boards', '/admin/boards', 'admin-boards'],
  ['tags', '/admin/tags', 'admin-tags'],
  ['attachments', '/admin/attachments', 'admin-attachments'],
  ['users', '/admin/users', 'admin-users'],
  ['roles', '/admin/roles', 'admin-roles'],
  ['moderation-cases', '/admin/moderation/cases', 'admin-reports'],
  ['assignments', '/admin/assignments', null],
  ['achievements', '/admin/achievements', 'admin-achievements'],
  ['points', '/admin/points', 'admin-points'],
  ['levels', '/admin/levels', 'admin-levels'],
  ['shop', '/admin/shop', null],
  ['activity', '/admin/activity', null],
  ['marketplace', '/admin/marketplace', 'admin-marketplace'],
  ['download-billing', '/admin/download-billing', 'admin-download-billing'],
  ['themes', '/admin/themes', 'admin-themes'],
  ['plugins', '/admin/plugins', 'admin-plugins'],
  ['oauth', '/admin/oauth', 'admin-oauth'],
  ['ai', '/admin/ai', 'admin-ai'],
  ['video', '/admin/video', 'admin-video'],
  ['storage', '/admin/storage', 'admin-storage'],
  ['notifications', '/admin/notifications', 'admin-notifications'],
  ['audit', '/admin/audit', 'admin-audit'],
  ['settings', '/admin/settings', 'admin-settings'],
  ['bi', '/admin/bi', 'admin-bi']
];

rmSync(OUT, { recursive: true, force: true });
mkdirSync(OUT, { recursive: true });

const personasPath = process.env.VISUAL_PERSONAS ?? join(__dirname, 'personas.json');
if (!existsSync(personasPath)) {
  console.error(`personas.json 不存在（先跑 fixtures/serve.mjs）: ${personasPath}`);
  process.exit(1);
}
const { personas } = JSON.parse(readFileSync(personasPath, 'utf8'));
const adminSession = personas.admin?.session;
if (!adminSession) {
  console.error('admin persona 缺 session');
  process.exit(1);
}

const browser = await chromium.launch();
const context = await browser.newContext({
  viewport: { width: 1280, height: 720 },
  locale: 'zh-CN',
  timezoneId: 'Asia/Shanghai'
});
await context.addCookies([
  {
    name: '__Host-bblbb_session',
    value: adminSession,
    domain: 'localhost',
    path: '/',
    secure: true,
    sameSite: 'Lax'
  }
]);
// 主题：默认浅色；THEME=dark 输出暗色（暗色逐页目检用）。
const THEME = process.env.THEME === 'dark' ? 'dark' : 'light';
const OUT_SUFFIX = THEME === 'dark' ? '-dark' : '';
await context.addInitScript(() => {
  try {
    localStorage.setItem('bblbb-theme', 'light');
    localStorage.setItem('theme', 'light');
  } catch {}
});
// initScript 先于页面脚本执行；暗色在页面脚本后再覆写一次偏好键。
await context.addInitScript(() => {
  try {
    if (process.env.THEME === 'dark') {
      localStorage.setItem('bblbb-theme', 'dark');
      localStorage.setItem('theme', 'dark');
      document.documentElement.dataset.theme = 'dark';
    }
  } catch {}
});

async function settle(page, ms) {
  await page.waitForLoadState('networkidle', { timeout: 15000 }).catch(() => {});
  await page.waitForTimeout(ms);
}

let failed = 0;
for (const [name, fePath, protoPage] of PAIRS) {
  // 前端页
  try {
    const page = await context.newPage();
    await page.goto(BASE + fePath, { waitUntil: 'domcontentloaded', timeout: 30000 });
    await settle(page, 900);
    await page.screenshot({ path: join(OUT, `fe-${name}${OUT_SUFFIX}.png`), fullPage: true });
    await page.close();
  } catch (e) {
    failed += 1;
    console.error(`FE FAIL ${name}: ${e.message.split('\n')[0]}`);
  }
  // 原型页
  if (protoPage) {
    try {
      const page = await context.newPage();
      await page.goto(`${PROTO}/pages/${protoPage}.html`, {
        waitUntil: 'domcontentloaded',
        timeout: 30000
      });
      await settle(page, 900);
      await page.screenshot({ path: join(OUT, `proto-${name}${OUT_SUFFIX}.png`), fullPage: true });
      await page.close();
    } catch (e) {
      failed += 1;
      console.error(`PROTO FAIL ${name}: ${e.message.split('\n')[0]}`);
    }
  }
  console.log(`done ${name}`);
}

await browser.close();
console.log(failed ? `FAILED=${failed}` : 'ALL_OK');
