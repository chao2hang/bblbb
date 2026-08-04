import fs from 'node:fs';
import path from 'node:path';
import { JSDOM, VirtualConsole } from 'jsdom';

const ROOT = process.cwd();
const vc = new VirtualConsole();
vc.on('jsdomError', () => {});
const html = fs.readFileSync(path.join(ROOT, 'index.html'), 'utf8');
const dom = new JSDOM(html, { runScripts: 'dangerously', virtualConsole: vc, url: 'http://localhost/', pretendToBeVisual: true });
const w = dom.window;
w.scrollTo = () => {};
w.matchMedia = w.matchMedia || (() => ({ matches: false, addEventListener(){}, addListener(){} }));
for (const f of ['js/icons.js','js/mock.js','js/store.js','js/ui/atoms.js','js/ui/composites.js','js/ui/overlays.js','js/ui/bundle.js','js/pages.js','js/pages2.js','js/pages3.js','js/lazy-loader.js','js/router.js','js/app.js']) {
  w.eval(fs.readFileSync(path.join(ROOT, f), 'utf8'));
}
const routes = ['/','/articles','/boards','/boards/rust','/tags','/tags/Rust','/shop','/activity','/me/closet','/topics/101','/topics/203','/publish','/search','/search?q=rust','/notifications','/favorites','/users/Chaos','/users/Chaos?tab=points','/settings','/settings?tab=oauth','/login','/register','/forgot-password','/403','/404','/429','/admin','/admin/reports','/admin/points','/admin/shop','/admin/activity','/admin/levels','/admin/themes','/admin/plugins','/admin/oauth','/admin/marketplace','/admin/download-billing','/admin/ai','/admin/video','/admin/settings'];
const classes = new Set();
for (const r of routes) {
  w.location.hash = '#' + r;
  w.Router.render();
  w.document.getElementById('app').querySelectorAll('*').forEach(el => el.classList.forEach(c => classes.add(c)));
}
console.log([...classes].sort().join(' '));
console.error('TOTAL:', classes.size);
