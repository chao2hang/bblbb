// 手机端（390x844）全站对比抓取：原型 SPA（8765）vs 生产前端（vite 4183）。
//
// 用法（依赖已就绪的栈：mobile.sqlite 后端 8091 + vite 4183 + 原型 8765）:
//   node tests/visual/mobile-compare.mjs
//
// 输出（覆盖式）：
//   reports/mobile-compare/proto/<slug>.png   原型侧 fullPage 截图
//   reports/mobile-compare/app/<slug>.png     前端侧 fullPage 截图
//   reports/mobile-compare/report.json        每页的 title / 横向溢出 / 交互入口清单 / 错误
//
// 约定：
// - 两侧统一 iPhone 13 模拟（390x844, dpr3, touch, mobile UA）+ 浅色主题；
// - 原型以 localStorage `bblbb-prototype-state={"auth":true}` 注入登录态（默认 admin 身份 Chaos）；
// - 前端以 admin persona 的 __Host-bblbb_session cookie 注入登录态（mobile-personas.json）。
import { chromium } from '@playwright/test';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO = join(__dirname, '..', '..', '..');
const OUT = join(REPO, 'reports', 'mobile-compare');
const APP_BASE = process.env.MC_APP_BASE ?? 'http://127.0.0.1:4183';
const PROTO_BASE = process.env.MC_PROTO_BASE ?? 'http://127.0.0.1:8765';
const API_BASE = process.env.MC_API_BASE ?? 'http://127.0.0.1:8091';

const personas = JSON.parse(
  readFileSync(join(__dirname, process.env.MC_PERSONAS ?? 'mobile-personas.json'), 'utf8')
).personas;
const adminSession = personas.admin?.session;
if (!adminSession) {
  console.error('admin persona 缺 session（先跑 fixtures/serve.mjs 铸种）');
  process.exit(1);
}

// ---------- 从 API 解析参数化路由需要的真实 ID ----------
async function api(path) {
  const res = await fetch(`${API_BASE}${path}`, {
    headers: { Cookie: `__Host-bblbb_session=${adminSession}` },
  });
  if (!res.ok) throw new Error(`API ${path} -> ${res.status}`);
  return res.json();
}

async function resolveParams() {
  const p = {};
  try {
    const boards = await api('/api/v1/boards?limit=20');
    p.boardSlug = boards.items?.[0]?.slug ?? 'general';
  } catch (e) {
    console.error('resolve boards:', e.message);
  }
  try {
    const tags = await api('/api/v1/tags?limit=20');
    p.tagSlug = tags.items?.[0]?.slug ?? tags.items?.[0]?.name ?? 'rust';
  } catch (e) {
    console.error('resolve tags:', e.message);
  }
  try {
    const posts = await api('/api/v1/posts?limit=20');
    const pub = posts.items?.find((x) => x.status === 'published') ?? posts.items?.[0];
    p.postId = pub?.id ?? null;
    p.username = pub?.author?.username ?? 'admin';
  } catch (e) {
    console.error('resolve posts:', e.message);
  }
  try {
    const offers = await api('/api/v1/marketplace/offers?limit=20');
    p.offerId = offers.items?.[0]?.id ?? null;
  } catch (e) {
    console.error('resolve offers:', e.message);
  }
  try {
    const cases = await api('/api/v1/admin/moderation/cases?limit=20');
    p.caseId = cases.items?.[0]?.id ?? null;
  } catch (e) {
    console.error('resolve cases:', e.message);
  }
  return p;
}

// ---------- 路由表：[slug, 原型 hash 或 null, 前端路径或 null, 说明] ----------
// 前端 null = 前端无对应页（差异清单标记 frontend-missing）；原型 null 同理。
function buildRoutes(p) {
  const R = [];
  const add = (slug, proto, app, note) => R.push({ slug, proto, app, note });

  // 公开页
  add('home', '#home', '/', '首页信息流');
  add('discover', '#discover', '/discover', '发现/推荐');
  add('articles', '#articles', null, '内容列表（生产已移除）');
  add('boards', '#boards', '/boards', '板块列表');
  add('board', `#board:${p.boardSlug ?? 'rust'}`, `/boards/${p.boardSlug ?? 'general'}`, '板块详情');
  add('tags', '#tags', '/tags', '标签列表');
  add('tag', `#tag:${p.tagSlug ?? 'Rust'}`, `/tags/${p.tagSlug ?? 'rust'}`, '标签详情');
  add('topic', '#topic:rust-cli', p.postId ? `/posts/${p.postId}` : null, '帖子详情');
  add('search', '#search:Rust', '/search?q=Rust', '搜索结果');
  add('login', '#login', '/login', '登录');
  add('register', '#register', '/register', '注册');
  add('forgot-password', '#forgot-password', '/password-reset', '重置密码');
  add('user', '#user:Chaos', `/users/${p.username ?? 'admin'}`, '用户主页');
  add('notfound', '#404', '/this-route-does-not-exist-404', '404 页');
  // 登录态·个人
  add('me', '#me', '/me', '我的');
  add('publish', '#publish:article', '/editor', '发布/编辑');
  add('drafts', '#drafts', '/me/drafts', '草稿箱');
  add('favorites', '#favorites', '/favorites', '收藏');
  add('settings', '#settings:profile', '/settings', '账号设置');
  add('mfa', '#mfa', '/mfa', '两步验证（M18-MFA-01 独立页）');
  add('billing', '#billing', '/me/billing', '下载账单');
  add('appeals', '#appeals', '/moderation/appeals', '申诉中心');
  add('apikeys', '#apikeys', '/apikeys', 'API 密钥');
  add('achievements', '#achievements', '/achievements', '成就墙');
  add('messages', '#messages', '/messages', '私信');
  add('notifications', '#notifications', '/notifications', '通知');
  // 登录态·交易
  add('shop', '#shop', '/shop', '商城');
  add('market', '#market', '/marketplace', '应用市场');
  add('checkout', '#checkout', p.offerId ? `/marketplace/checkout/${p.offerId}` : null, '市场结算');
  add('purchases', '#purchases', '/marketplace/purchases', '市场购买记录');
  // 错误页
  add('err403', '#403', null, '403（前端按页内错误态呈现，无独立路由）');
  add('err429', '#429', null, '429（前端按页内错误态呈现，无独立路由）');
  add('err500', '#error', '/this-route-does-not-exist-404', '错误兜底（近似对比）');
  // 管理后台
  add('admin', '#admin', '/admin', '后台仪表盘');
  add('admin-posts', '#admin-posts', '/admin/posts', '后台·帖子');
  add('admin-boards', '#admin-boards', '/admin/boards', '后台·板块');
  add('admin-tags', '#admin-tags', '/admin/tags', '后台·标签');
  add('admin-attachments', '#admin-attachments', '/admin/attachments', '后台·附件');
  add('admin-users', '#admin-users', '/admin/users', '后台·用户');
  add('admin-roles', '#admin-roles', '/admin/roles', '后台·角色');
  add('admin-reports', '#admin-reports', '/admin/moderation/cases', '后台·举报/案件');
  add('admin-report', '#admin-report:R-1024', p.caseId ? `/admin/moderation/cases/${p.caseId}` : null, '后台·案件详情');
  add('admin-content', '#admin-article-audit', '/admin/content', '后台·内容审核');
  add('admin-points', '#admin-points', '/admin/points', '后台·积分');
  add('admin-levels', '#admin-levels', '/admin/levels', '后台·等级');
  add('admin-achievements', '#admin-achievements', '/admin/achievements', '后台·成就');
  add('admin-themes', '#admin-themes', '/admin/themes', '后台·主题');
  add('admin-plugins', '#admin-plugins', '/admin/plugins', '后台·插件');
  add('admin-oauth', '#admin-oauth', '/admin/oauth', '后台·OAuth');
  add('admin-storage', '#admin-storage', '/admin/storage', '后台·存储');
  add('admin-download-billing', '#admin-download-billing', '/admin/download-billing', '后台·下载计费');
  add('admin-ai', '#admin-ai', '/admin/ai', '后台·AI');
  add('admin-video', '#admin-video', '/admin/video', '后台·视频');
  add('admin-marketplace', '#admin-marketplace', '/admin/marketplace', '后台·市场');
  add('admin-bi', '#admin-bi', '/admin/bi', '后台·BI');
  add('admin-notifications', '#admin-notifications', '/admin/notifications', '后台·通知');
  add('admin-audit', '#admin-audit', '/admin/audit', '后台·审计');
  add('admin-settings', '#admin-settings', '/admin/settings', '后台·设置');
  return R;
}

// ---------- 采集 ----------
const MOBILE = {
  viewport: { width: 390, height: 844 },
  deviceScaleFactor: 3,
  hasTouch: true,
  isMobile: true,
  userAgent:
    'Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1',
  locale: 'zh-CN',
  timezoneId: 'Asia/Shanghai',
};

function describe(el) {
  const tag = el.tagName.toLowerCase();
  const id = el.id ? `#${el.id}` : '';
  const cls = el.className && typeof el.className === 'string'
    ? '.' + el.className.trim().split(/\s+/).slice(0, 2).join('.')
    : '';
  return tag + id + cls;
}

async function collect(page) {
  return page.evaluate(() => {
    const vw = window.innerWidth;
    const doc = document.documentElement;
    const overflowing = [];
    const seen = new Set();
    for (const el of document.querySelectorAll('body *')) {
      const r = el.getBoundingClientRect();
      if (r.width <= 0) continue;
      const over = Math.round(r.right - vw);
      if (over > 2) {
        const key = describe2(el) + over;
        if (seen.has(key)) continue;
        seen.add(key);
        overflowing.push({ el: describe2(el), overPx: over, text: (el.textContent || '').trim().slice(0, 24) });
      }
    }
    function describe2(el) {
      const tag = el.tagName.toLowerCase();
      const id = el.id ? '#' + el.id : '';
      const cls =
        el.className && typeof el.className === 'string'
          ? '.' + el.className.trim().split(/\s+/).slice(0, 2).join('.')
          : '';
      return tag + id + cls;
    }
    overflowing.sort((a, b) => b.overPx - a.overPx);
    const items = [];
    for (const el of document.querySelectorAll('a[href], button, [role="button"], input, select, textarea, summary')) {
      const st = getComputedStyle(el);
      if (st.display === 'none' || st.visibility === 'hidden') continue;
      const rect = el.getBoundingClientRect();
      if (rect.width === 0 && rect.height === 0) continue;
      const label = (el.getAttribute('aria-label') || el.textContent || el.getAttribute('placeholder') || el.getAttribute('name') || '').trim().replace(/\s+/g, ' ');
      const kind = el.tagName.toLowerCase();
      const href = el.getAttribute && el.getAttribute('href');
      items.push((kind === 'a' && href ? 'a:' + href : kind) + (label ? ' «' + label.slice(0, 36) + '»' : ''));
    }
    return {
      scrollWidth: doc.scrollWidth,
      innerWidth: vw,
      hOverflowPx: doc.scrollWidth - vw,
      overflowing: overflowing.slice(0, 15),
      inventory: items.slice(0, 400),
    };
  });
}

async function settle(page, extraMs) {
  await page.waitForLoadState('networkidle', { timeout: 10000 }).catch(() => {});
  await page.waitForTimeout(extraMs ?? 700);
}

async function shootProto(page, hash) {
  await page.goto(`${PROTO_BASE}/#${hash.replace(/^#/, '')}`, { waitUntil: 'domcontentloaded', timeout: 30000 });
  // 等路由引擎渲染出可见页
  await page
    .waitForFunction(
      (h) => {
        const cur = location.hash || '#home';
        if (cur !== h && !cur.startsWith(h)) return false;
        const pageEl = document.querySelector('.page:not([hidden])');
        return !!pageEl && (pageEl.querySelector('h1, .post-card, .board-card, form, table, .msg-list') || pageEl.textContent.length > 40);
      },
      `#${hash.replace(/^#/, '')}`,
      { timeout: 15000 }
    )
    .catch(() => {});
  await settle(page, 600);
}

async function shoot(page, side, url, pngPath) {
  const pageErrors = [];
  const reqFailed = [];
  page.on('pageerror', (e) => pageErrors.push(String(e.message).slice(0, 200)));
  page.on('requestfailed', (r) => {
    const u = r.url();
    if (!u.startsWith('data:')) reqFailed.push(`${r.failure()?.errorText ?? 'failed'} ${u.replace(location.origin, '').slice(0, 120)}`);
  });
  const info = { url, error: null, title: null, overflow: null, inventory: null };
  try {
    if (side === 'proto') await shootProto(page, url);
    else await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30000 });
    await settle(page);
    info.title = await page.title();
    const data = await collect(page);
    info.overflow = { ...data, inventory: undefined };
    info.inventory = data.inventory;
    info.error =
      pageErrors.length || reqFailed.length
        ? { pageErrors: pageErrors.slice(0, 5), reqFailed: reqFailed.slice(0, 8) }
        : null;
    await page.screenshot({ path: pngPath, fullPage: true });
  } catch (e) {
    info.error = { goto: String(e.message).split('\n')[0], pageErrors: pageErrors.slice(0, 5) };
    try {
      await page.screenshot({ path: pngPath, fullPage: true }).catch(() => {});
    } catch {}
  }
  return info;
}

// ---------- 主流程 ----------
const routes = buildRoutes(await resolveParams());
mkdirSync(join(OUT, 'proto'), { recursive: true });
mkdirSync(join(OUT, 'app'), { recursive: true });

const browser = await chromium.launch();

const protoCtx = await browser.newContext(MOBILE);
await protoCtx.addInitScript(() => {
  try {
    localStorage.setItem('bblbb-theme', 'light');
    localStorage.setItem('bblbb-prototype-state', JSON.stringify({ auth: true }));
  } catch {}
});

const appCtx = await browser.newContext(MOBILE);
await appCtx.addInitScript(() => {
  try {
    localStorage.setItem('theme', 'light');
    localStorage.setItem('bblbb-theme', 'light');
  } catch {}
});
await appCtx.addCookies([
  {
    name: '__Host-bblbb_session',
    value: adminSession,
    domain: '127.0.0.1',
    path: '/',
    secure: true,
    sameSite: 'Lax',
  },
]);

const report = { generatedAt: new Date().toISOString(), viewport: MOBILE.viewport, pages: [] };

let failed = 0;
for (const r of routes) {
  const entry = { slug: r.slug, note: r.note };
  if (r.proto) {
    const page = await protoCtx.newPage();
    entry.proto = await shoot(page, 'proto', r.proto, join(OUT, 'proto', `${r.slug}.png`));
    await page.close();
  }
  if (r.app) {
    const page = await appCtx.newPage();
    entry.app = await shoot(page, 'app', APP_BASE + r.app, join(OUT, 'app', `${r.slug}.png`));
    await page.close();
  }
  report.pages.push(entry);
  const flag = entry.proto?.error || entry.app?.error ? '  [ERR]' : '';
  console.log(`done ${r.slug}${flag}`);
  if (entry.proto?.error?.goto || entry.app?.error?.goto) failed += 1;
}

report.params = null; // 参数已固化在 url 字段里
writeFileSync(join(OUT, 'report.json'), JSON.stringify(report, null, 2));
await browser.close();
console.log(failed ? `FAILED=${failed}` : 'ALL_OK');
console.log('OUT=' + OUT);
