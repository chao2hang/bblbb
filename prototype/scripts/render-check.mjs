#!/usr/bin/env node
// BBLBB prototype regression harness (jsdom).
//
// Usage:
//   node scripts/render-check.mjs            # assert only (CI-friendly)
//   node scripts/render-check.mjs --golden   # write golden HTML snapshots
//
// Asserts for every route:
//   1. #app renders non-empty, with no `undefined` / `NaN` / `[object Object]`
//      / unrendered `${` templates
//   2. every class used in rendered HTML is defined in the CSS bundle
//   3. every `var(--x)` referenced is defined in tokens
//   4. (golden mode) snapshots diff — after a deliberate visual redesign,
//      regenerate with --golden once the diff is human-verified
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { JSDOM, VirtualConsole } from 'jsdom';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const GOLDEN_DIR = path.join(ROOT, 'scripts', 'golden');
const GOLDEN_MODE = process.argv.includes('--golden');

// Icon calls must resolve to local SVG paths. Keep this list aligned with the
// scripts loaded by index.html; dynamic names are covered by rendered output.
const iconSourceFiles = [
  'pages.js', 'pages2.js', 'pages3.js',
  'ui/atoms.js', 'ui/composites.js', 'ui/overlays.js', 'ui/bundle.js'
];

// ---------------------------------------------------------------------------
// CSS bundle: tokens + base + layout + components + pages (in <link> order)
// ---------------------------------------------------------------------------
const cssFiles = ['tokens.css', 'base.css', 'layout.css', 'components.css', 'pages.css', 'visual-overrides.css'];
const cssBundle = cssFiles
  .map((f) => fs.existsSync(path.join(ROOT, 'css', f)) ? fs.readFileSync(path.join(ROOT, 'css', f), 'utf8') : '')
  .join('\n');

const cssClasses = new Set((cssBundle.match(/\.[a-zA-Z][a-zA-Z0-9_-]*/g) || []).map((s) => s.slice(1)));
// Only declarations count as definitions. Matching every `--token` occurrence
// would make an undefined `var(--token)` look valid merely because it was
// referenced in the same bundle.
const cssTokenDefinitions = new Set(
  [...cssBundle.matchAll(/--([a-zA-Z0-9-]+)\s*:/g)].map((match) => match[1])
);

const iconSource = iconSourceFiles
  .map((f) => fs.existsSync(path.join(ROOT, 'js', f)) ? fs.readFileSync(path.join(ROOT, 'js', f), 'utf8') : '')
  .join('\n');
// Some presentation tokens are intentionally supplied by rendered inline
// styles (for example `--cat-color` and `--swatch-color`). Include those
// declarations so the check validates real semantic tokens without flagging
// route data as a missing global token.
const runtimeTokenDefinitions = new Set(
  [...iconSource.matchAll(/--([a-zA-Z0-9-]+)\s*:/g)].map((match) => match[1])
);
const definedTokens = new Set([...cssTokenDefinitions, ...runtimeTokenDefinitions]);
const staticIconNames = new Set();
for (const re of [/(?:C\.)?icon\(\s*['\"]([^'\"]+)/g, /icon\s*:\s*['\"]([^'\"]+)['\"]/g]) {
  for (const match of iconSource.matchAll(re)) staticIconNames.add(match[1]);
}
const iconRegistrySource = fs.readFileSync(path.join(ROOT, 'js', 'icons.js'), 'utf8');
const registeredIconNames = new Set((iconRegistrySource.match(/^\s*"([^"]+)"\s*:/gm) || []).map((s) => s.match(/"([^"]+)"/)[1]));
const missingStaticIcons = [...staticIconNames]
  .filter((name) => /^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(name))
  .filter((name) => !registeredIconNames.has(name));

// ---------------------------------------------------------------------------
// jsdom bootstrap
// ---------------------------------------------------------------------------
const errors = [];
const vc = new VirtualConsole();
vc.on('jsdomError', (e) => { if (!/scrollTo|Not implemented/i.test(String(e.detail || e.message))) errors.push('jsdomError: ' + (e.detail || e.message)); });
vc.on('error', (...a) => errors.push('console.error: ' + a.join(' ')));

function boot() {
  const html = fs.readFileSync(path.join(ROOT, 'index.html'), 'utf8')
    .replace(/<script src="https:\/\/[^"]*"><\/script>/g, '');
  const dom = new JSDOM(html, {
    runScripts: 'dangerously', virtualConsole: vc,
    url: 'http://localhost/', pretendToBeVisual: true,
  });
  const w = dom.window;
  w.lucide = { createIcons: () => {} };
  w.scrollTo = () => {};
  // JS load order: new structure (icons + ui/ + split pages) or legacy (single components.js)
  const NEW = ['icons.js', 'mock.js', 'store.js', 'ui/atoms.js', 'ui/composites.js', 'ui/overlays.js', 'ui/bundle.js', 'pages.js', 'pages2.js', 'pages3.js', 'lazy-loader.js', 'router.js', 'app.js'];
  const LEGACY = ['icons.js', 'mock.js', 'store.js', 'components.js', 'pages.js', 'pages2.js', 'pages3.js', 'router.js', 'app.js'];
  const jsFiles = fs.existsSync(path.join(ROOT, 'js', 'ui', 'atoms.js')) ? NEW : LEGACY;
  for (const f of jsFiles) {
    const p = path.join(ROOT, 'js', f);
    if (!fs.existsSync(p)) continue;
    try { w.eval(fs.readFileSync(p, 'utf8')); }
    catch (e) { errors.push(`eval ${f}: ${e.message}`); }
  }
  return dom;
}

// Route inventory: derive from router.js patterns
function routes() {
  const src = fs.readFileSync(path.join(ROOT, 'js', 'router.js'), 'utf8');
  const patterns = [...src.matchAll(/pattern: '([^']+)'/g)].map((m) => m[1]);
  const dynamic = [
    '/boards/rust', '/boards/rust?tab=hot', '/boards/rust?tab=essence',
    '/tags/Rust', '/topics/101', '/topics/203',
    '/users/Chaos', '/users/Chaos?tab=replies', '/users/Chaos?tab=favorites', '/users/Chaos?tab=about', '/users/Chaos?tab=points',
    '/search?q=rust', '/search?q=rust&tab=articles', '/search?q=rust&tab=users',
    '/settings?tab=security', '/settings?tab=devices', '/settings?tab=notifications', '/settings?tab=oauth',
    '/notifications?tab=unread', '/favorites?tab=articles', '/shop', '/activity', '/me/closet',
    '/publish?type=article', '/forgot-password', '/403', '/429',
    '/admin/users?role=admin', '/admin/content?status=pending', '/admin/reports?status=pending'
  ];
  return [...new Set([...patterns, ...dynamic])];
}

// ---------------------------------------------------------------------------
// Run
// ---------------------------------------------------------------------------
const dom = boot();
const { window } = dom;
const results = [];
const routeList = routes();
if (missingStaticIcons.length) {
  errors.push('missing icon registrations: ' + missingStaticIcons.join(', '));
}

for (const r of routeList) {
  const before = errors.length;
  let len = 0, htmlText = '', cls = new Set(), tokensUsed = new Set();
  try {
    window.location.hash = '#' + r;
    window.Router.render();
    const app = window.document.getElementById('app');
    if (!app) throw new Error('#app missing');
    htmlText = app.innerHTML;
    len = htmlText.length;
    app.querySelectorAll('*').forEach((el) => el.classList.forEach((c) => cls.add(c)));
    for (const m of htmlText.matchAll(/var\(--([a-zA-Z0-9-]+)/g)) tokensUsed.add(m[1]);
  } catch (e) {
    errors.push(`route ${r}: ${e.message}`);
  }
  const issues = [];
  if (len < 200) issues.push('empty');
  if (/undefined|NaN|\[object Object\]/.test(htmlText)) issues.push('bad-value');
  if (/\$\{/.test(htmlText)) issues.push('unrendered-tpl');
  const missingRenderedIcons = [...window.document.querySelectorAll('#app svg[data-missing-icon]')].map((el) => el.getAttribute('title') || el.className.baseVal);
  if (missingRenderedIcons.length) issues.push('missing-rendered-icons: ' + missingRenderedIcons.slice(0, 8).join(','));
  const used = [...cls].filter((c) => !cssClasses.has(c) && !/^icon-/.test(c) && !/^language-/.test(c));
  const missingTokens = [...tokensUsed].filter((t) => !definedTokens.has(t));
  if (used.length) issues.push('class-not-in-css: ' + used.slice(0, 8).join(','));
  if (missingTokens.length) issues.push('token-undefined: ' + missingTokens.slice(0, 8).join(','));
  results.push({ route: r, len, issues, html: htmlText });
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------
const failures = results.filter((r) => r.issues.length);
console.log('routes checked:', results.length);
console.log('failures      :', failures.length);
for (const f of failures) console.log(`  [${f.route}] ${f.issues.join(' | ')}`);
console.log('js errors     :', errors.length);
for (const e of [...new Set(errors)].slice(0, 10)) console.log('  - ' + e);

if (GOLDEN_MODE) {
  fs.mkdirSync(GOLDEN_DIR, { recursive: true });
  let written = 0;
  for (const r of results) {
    const file = path.join(GOLDEN_DIR, (r.route === '/' ? 'home' : r.route.replace(/[^\w-]+/g, '_').replace(/^_+|_+$/g, '') || 'root') + '.html');
    fs.writeFileSync(file, r.html);
    written++;
  }
  fs.writeFileSync(path.join(GOLDEN_DIR, 'routes.json'), JSON.stringify(results.map((r) => ({ route: r.route, len: r.len })), null, 2));
  console.log('golden snapshots written:', written);
} else {
  const diff = [];
  if (fs.existsSync(path.join(GOLDEN_DIR, 'routes.json'))) {
    const golden = JSON.parse(fs.readFileSync(path.join(GOLDEN_DIR, 'routes.json'), 'utf8'));
    for (const g of golden) {
      const cur = results.find((r) => r.route === g.route);
      if (cur && cur.len !== g.len) diff.push(`${g.route}: golden=${g.len} now=${cur.len}`);
    }
    if (diff.length) console.log('\ngolden diff (length mismatch):\n' + diff.join('\n'));
    else console.log('golden diff: none (lengths match)');
  } else {
    console.log('no golden baseline yet — run with --golden first');
  }
}

process.exit(failures.length || errors.length ? 1 : 0);
