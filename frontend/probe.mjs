import { chromium } from 'playwright';

const base = process.env.BASE ?? 'https://127.0.0.1:5173';
const path = process.env.PATHNAME ?? '/';
const sel = (process.env.SELECTORS ?? '.category-card,.thread-list,.recommend').split(',');

const browser = await chromium.launch();
const ctx = await browser.newContext({ ignoreHTTPSErrors: true, viewport: { width: 1440, height: 1000 } });
const page = await ctx.newPage();
await page.goto(base + path, { waitUntil: 'networkidle', timeout: 45000 });
const out = await page.evaluate((selectors) => {
  return selectors.map((s) => {
    const el = document.querySelector(s);
    if (!el) return { sel: s, missing: true };
    const cs = getComputedStyle(el);
    return {
      sel: s,
      bg: cs.backgroundColor,
      border: cs.borderTopWidth + ' ' + cs.borderTopStyle + ' ' + cs.borderTopColor,
      radius: cs.borderTopLeftRadius,
      shadow: cs.boxShadow,
      font: cs.fontFamily.slice(0, 60)
    };
  });
}, sel);
console.log(JSON.stringify(out, null, 2));
await browser.close();
