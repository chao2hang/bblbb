#!/usr/bin/env node
import http from 'node:http';
import WebSocket from 'ws';

// Real-browser regression for every public, account and administration route.
//
// Usage:
//   node scripts/browser-audit.mjs                 # 3 representative viewports
//   node scripts/browser-audit.mjs --breakpoints   # all layout boundary pairs
//
// Prerequisites:
//   python3 -m http.server 4173 -d prototype
//   Chrome --remote-debugging-port=9223
const routes = [
  '/', '/articles', '/boards', '/boards/rust', '/boards/rust?tab=hot', '/boards/rust?tab=essence',
  '/tags', '/tags/Rust', '/shop', '/activity', '/me/closet', '/topics/101', '/topics/203',
  '/publish', '/publish?type=article',
  '/users/Chaos', '/users/Chaos?tab=replies', '/users/Chaos?tab=favorites', '/users/Chaos?tab=points', '/users/Chaos?tab=about',
  '/notifications', '/notifications?tab=unread', '/favorites', '/favorites?tab=articles',
  '/search', '/search?q=rust', '/search?q=rust&tab=articles', '/search?q=rust&tab=users',
  '/settings', '/settings?tab=security', '/settings?tab=devices', '/settings?tab=notifications', '/settings?tab=oauth',
  '/login', '/register', '/forgot-password', '/403', '/404', '/429',
  '/admin', '/admin/users', '/admin/users?role=admin', '/admin/roles',
  '/admin/content', '/admin/content?status=pending', '/admin/posts', '/admin/boards', '/admin/tags',
  '/admin/reports', '/admin/reports?status=pending', '/admin/reports/R-1024',
  '/admin/points', '/admin/shop', '/admin/activity', '/admin/levels', '/admin/attachments',
  '/admin/download-billing', '/admin/ai', '/admin/video', '/admin/storage', '/admin/notifications',
  '/admin/themes', '/admin/plugins', '/admin/oauth', '/admin/marketplace', '/admin/audit', '/admin/settings'
];

const coreViewports = [
  { name: 'desktop', width: 1440, height: 1000 },
  { name: 'tablet-900', width: 900, height: 900 },
  { name: 'mobile-390', width: 390, height: 844 }
];

// Boundary pairs verify both sides of every layout media query that changes
// navigation, two-column shells, content grids or mobile spacing.
const breakpointViewports = [
  { name: 'desktop', width: 1440, height: 1000 },
  { name: 'content-1024', width: 1024, height: 900 },
  { name: 'content-1023', width: 1023, height: 900 },
  { name: 'nav-901', width: 901, height: 900 },
  { name: 'nav-900', width: 900, height: 900 },
  { name: 'mobile-641', width: 641, height: 860 },
  { name: 'mobile-640', width: 640, height: 860 },
  { name: 'mobile-390', width: 390, height: 844 },
  { name: 'mobile-360', width: 360, height: 800 }
];

const viewports = process.argv.includes('--breakpoints') ? breakpointViewports : coreViewports;
const themes = ['light', 'dark'];
const sleep = (ms) => new Promise(resolve => setTimeout(resolve, ms));

function getJson(path) {
  return new Promise((resolve, reject) => {
    http.get('http://127.0.0.1:9223' + path, response => {
      let body = '';
      response.on('data', chunk => { body += chunk; });
      response.on('end', () => {
        try { resolve(JSON.parse(body)); }
        catch (error) { reject(error); }
      });
    }).on('error', reject);
  });
}

const tabs = await getJson('/json/list');
const pageTarget = tabs.find(tab => tab.type === 'page' && tab.webSocketDebuggerUrl);
if (!pageTarget) throw new Error('No debuggable Chrome page target found on port 9223');

const ws = new WebSocket(pageTarget.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  ws.once('open', resolve);
  ws.once('error', reject);
});

let seq = 0;
let activeBrowserErrors = null;
const pending = new Map();
ws.on('message', raw => {
  const message = JSON.parse(raw);
  if (message.id && pending.has(message.id)) {
    const { resolve, reject } = pending.get(message.id);
    pending.delete(message.id);
    message.error ? reject(new Error(message.error.message)) : resolve(message.result);
    return;
  }

  if (!activeBrowserErrors) return;
  if (message.method === 'Runtime.exceptionThrown') {
    const detail = message.params.exceptionDetails;
    activeBrowserErrors.push(`runtime: ${detail.exception?.description || detail.text || 'Uncaught exception'}`);
  }
  if (message.method === 'Runtime.consoleAPICalled' && ['error', 'assert'].includes(message.params.type)) {
    const text = message.params.args
      .map(argument => argument.value ?? argument.description ?? argument.type)
      .join(' ');
    activeBrowserErrors.push(`console: ${text}`);
  }
  if (message.method === 'Log.entryAdded' && message.params.entry.level === 'error') {
    activeBrowserErrors.push(`log: ${message.params.entry.text}`);
  }
  if (message.method === 'Network.loadingFailed' &&
      !message.params.canceled &&
      message.params.errorText !== 'net::ERR_ABORTED') {
    activeBrowserErrors.push(`network: ${message.params.errorText}`);
  }
  if (message.method === 'Network.responseReceived' && message.params.response.status >= 400) {
    activeBrowserErrors.push(`http-${message.params.response.status}: ${message.params.response.url}`);
  }
});

function call(method, params = {}) {
  return new Promise((resolve, reject) => {
    const id = ++seq;
    pending.set(id, { resolve, reject });
    ws.send(JSON.stringify({ id, method, params }));
  });
}

async function evaluate(expression) {
  const { result, exceptionDetails } = await call('Runtime.evaluate', {
    expression,
    returnByValue: true,
    awaitPromise: true
  });
  if (exceptionDetails) {
    const detail = exceptionDetails.exception?.description || exceptionDetails.exception?.value || exceptionDetails.text;
    throw new Error(detail || 'Browser evaluation failed');
  }
  return result.value;
}

async function waitForRoute(route) {
  const expectedPath = route.split('?')[0];
  for (let attempt = 0; attempt < 20; attempt++) {
    try {
      const ready = await evaluate(`(() => {
        const app = document.getElementById('app');
        return document.readyState === 'complete' &&
          !!app && !app.hasAttribute('aria-busy') &&
          (app.innerText || '').trim().length >= 20 &&
          window.Router?.getPath?.() === ${JSON.stringify(expectedPath)};
      })()`);
      if (ready) return true;
    } catch (_) {
      // Navigation can briefly replace the execution context; retry below.
    }
    await sleep(20);
  }
  return false;
}

await call('Page.enable');
await call('Runtime.enable');
await call('Log.enable');
await call('Network.enable');
await call('Network.setCacheDisabled', { cacheDisabled: true });
await call('Emulation.setEmulatedMedia', {
  media: 'screen',
  features: [{ name: 'prefers-reduced-motion', value: 'no-preference' }]
});

const findings = [];
let checked = 0;

for (const theme of themes) {
  for (const viewport of viewports) {
    await call('Emulation.setDeviceMetricsOverride', {
      width: viewport.width,
      height: viewport.height,
      deviceScaleFactor: 1,
      mobile: false
    });

    for (const route of routes) {
      const browserErrors = [];
      activeBrowserErrors = browserErrors;
      const target = `http://127.0.0.1:4173/index.html?visual_audit=${Date.now()}#${route}`;
      await call('Page.navigate', { url: target });
      await sleep(55);
      const initialReady = await waitForRoute(route);
      await evaluate(`Store.setTheme(${JSON.stringify(theme)}); Router.refresh()`);
      await sleep(28);
      const themeReady = await waitForRoute(route);

      const value = await evaluate(`(() => {
        const root = document.documentElement;
        const body = document.body;
        const app = document.getElementById('app');
        const isVisible = (element, style, rect) =>
          style.display !== 'none' && style.visibility !== 'hidden' &&
          Number(style.opacity || 1) > 0 && rect.width > 0 && rect.height > 0;
        const isInsideHorizontalScroller = element => {
          let parent = element.parentElement;
          while (parent && parent !== body) {
            const style = getComputedStyle(parent);
            if (/auto|scroll|hidden|clip/.test(style.overflowX) && parent.scrollWidth > parent.clientWidth) return true;
            parent = parent.parentElement;
          }
          return false;
        };
        const offenders = [...document.querySelectorAll('body *')]
          .filter(element => {
            const style = getComputedStyle(element);
            const rect = element.getBoundingClientRect();
            return isVisible(element, style, rect) &&
              !isInsideHorizontalScroller(element) &&
              (rect.right > innerWidth + 2 || rect.left < -2);
          })
          .slice(0, 8)
          .map(element => {
            const rect = element.getBoundingClientRect();
            return {
              tag: element.tagName,
              cls: element.className?.toString().slice(0, 100),
              left: Math.round(rect.left),
              right: Math.round(rect.right),
              width: Math.round(rect.width)
            };
          });
        const flatCardShadows = [...document.querySelectorAll('.article-card, .board-card')]
          .filter(element => getComputedStyle(element).boxShadow !== 'none')
          .slice(0, 4)
          .map(element => element.className);
        const contentGrid = document.querySelector('.content-grid');
        const responsiveGrid = document.querySelector('.articles-grid, .articles-page-grid, .boards-grid');
        const columnCount = element => element
          ? getComputedStyle(element).gridTemplateColumns.trim().split(/\\s+/).filter(Boolean).length
          : 0;
        return {
          pageOverflow: Math.max(root.scrollWidth, body.scrollWidth) - innerWidth,
          offenders,
          bodyText: (app?.innerText || '').trim().length,
          routePath: window.Router?.getPath?.(),
          themeClass: root.classList.contains('dark') ? 'dark' : root.classList.contains('light') ? 'light' : '',
          missingIcons: document.querySelectorAll('svg[data-missing-icon]').length,
          emptyIcons: [...document.querySelectorAll('svg.icon')].filter(svg => !svg.children.length).length,
          flatCardShadows,
          navItemsDisplay: document.querySelector('.nav-items')
            ? getComputedStyle(document.querySelector('.nav-items')).display
            : '',
          mobileMenuDisplay: document.querySelector('.mobile-menu-btn')
            ? getComputedStyle(document.querySelector('.mobile-menu-btn')).display
            : '',
          contentGridColumns: columnCount(contentGrid),
          responsiveGridColumns: columnCount(responsiveGrid)
        };
      })()`);

      // Let late console/network events land before this route's error bucket is
      // evaluated, then detach it so the next navigation starts cleanly.
      await sleep(12);
      activeBrowserErrors = null;
      const uniqueBrowserErrors = [...new Set(browserErrors)];
      const expectedPath = route.split('?')[0];
      const issues = [];
      if (!initialReady || !themeReady) issues.push('route-timeout');
      if (uniqueBrowserErrors.length) issues.push('browser-error');
      if (value.pageOverflow > 2) issues.push('page-overflow');
      if (value.offenders.length) issues.push('horizontal-offender');
      if (value.bodyText < 20) issues.push('empty-page');
      if (value.routePath !== expectedPath) issues.push('wrong-route');
      if (value.themeClass !== theme) issues.push('wrong-theme');
      if (value.missingIcons) issues.push('missing-icons');
      if (value.emptyIcons) issues.push('empty-icons');
      if (value.flatCardShadows.length) issues.push('content-card-shadow');

      // Verify both sides of the exact CSS boundaries when the relevant shell
      // exists on the current route.
      if (viewport.width === 901 && value.navItemsDisplay === 'none') issues.push('nav-collapsed-too-early');
      if (viewport.width === 900 && value.mobileMenuDisplay === 'none') issues.push('mobile-nav-not-active');
      if (viewport.width === 1024 && value.contentGridColumns && value.contentGridColumns !== 2) issues.push('content-grid-1024');
      if (viewport.width === 1023 && value.contentGridColumns && value.contentGridColumns !== 1) issues.push('content-grid-1023');
      if (viewport.width === 641 && value.responsiveGridColumns && value.responsiveGridColumns < 2) issues.push('grid-collapsed-too-early');
      if (viewport.width === 640 && value.responsiveGridColumns && value.responsiveGridColumns !== 1) issues.push('grid-not-collapsed');

      if (issues.length) {
        findings.push({
          theme,
          viewport: viewport.name,
          route,
          issues,
          browserErrors: uniqueBrowserErrors.slice(0, 8),
          ...value
        });
      }
      checked++;
    }
  }
}

console.log(JSON.stringify({
  checked,
  routes: routes.length,
  viewports: viewports.length,
  themes: themes.length,
  findings
}, null, 2));
ws.close();
process.exit(findings.length ? 1 : 0);
