import { chromium } from 'playwright';

const base = process.env.BASE ?? 'https://127.0.0.1:5173';
const targets = (process.env.SHOTS ?? '/:home').split(',');
const tag = process.env.TAG ?? 'now';
const dark = process.env.DARK === '1';

const browser = await chromium.launch();
const ctx = await browser.newContext({
  ignoreHTTPSErrors: true,
  viewport: { width: 1440, height: 1000 },
  deviceScaleFactor: 1,
  colorScheme: dark ? 'dark' : 'light'
});
const page = await ctx.newPage();
for (const t of targets) {
  const idx = t.lastIndexOf(':');
  const path = t.slice(0, idx);
  const name = t.slice(idx + 1);
  try {
    await page.goto(base + path, { waitUntil: 'networkidle', timeout: 45000 });
  } catch {
    await page.goto(base + path, { waitUntil: 'domcontentloaded', timeout: 45000 });
  }
  await page.waitForTimeout(1200);
  const out = `/tmp/shots/${tag}-${name}.png`;
  await page.screenshot({ path: out, fullPage: process.env.FULL === '1' });
  console.log(out);
}
await browser.close();
