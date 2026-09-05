// 项目前端：量测导航栏内容左右位置（1920/1440/1200）
import { chromium } from './node_modules/playwright/index.mjs';

const BASE = process.env.FRONT_BASE || 'http://127.0.0.1:5173';
const browser = await chromium.launch();
for (const width of [1920, 1440, 1200]) {
  const page = await browser.newPage({ viewport: { width, height: 900 } });
  try {
    await page.goto(BASE + '/', { waitUntil: 'domcontentloaded', timeout: 15000 });
    await page.waitForTimeout(1200);
    const r = await page.evaluate(() => {
      const q = (sel) => {
        const el = document.querySelector(sel);
        if (!el) return null;
        const b = el.getBoundingClientRect();
        const cs = getComputedStyle(el);
        return { x: +b.x.toFixed(1), w: +b.width.toFixed(1), pad: cs.paddingInline || cs.padding, maxW: cs.maxWidth, disp: cs.display };
      };
      return {
        navbar: q('.navbar'),
        navContainer: q('.nav-container'),
        logo: q('.nav-logo'),
        main: q('main .container, main.container, #main-content'),
        vw: innerWidth,
      };
    });
    console.log(`--- @${width}`, JSON.stringify(r));
  } catch (e) {
    console.log(`--- @${width} ERROR ${e.message.split('\n')[0]}`);
  }
  await page.close();
}
await browser.close();
