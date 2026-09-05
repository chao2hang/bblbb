// 项目前端：canvas 解码截图，ASCII 可视化顶部区域 + 顶栏元素几何
import { chromium } from './node_modules/playwright/index.mjs';

const BASE = process.env.FRONT_BASE || 'http://127.0.0.1:5174';
const browser = await chromium.launch();

async function look(path, width) {
  const page = await browser.newPage({ viewport: { width, height: 900 } });
  try {
    await page.goto(BASE + path, { waitUntil: 'domcontentloaded', timeout: 20000 });
    await page.waitForTimeout(1500);
    const geo = await page.evaluate(() => {
      const g = (sel) => {
        const el = document.querySelector(sel);
        if (!el) return null;
        const b = el.getBoundingClientRect();
        return sel + ':x=' + b.x.toFixed(0) + ',w=' + b.width.toFixed(0);
      };
      return ['.navbar', '.nav-container', '.nav-logo', '.nav-right', '#main-content', '#main-content .container'].map(g).filter(Boolean).join(' | ');
    });
    console.log(`\n===== ${path} @${width}  ${geo}`);
    const shot = await page.screenshot();
    const b64 = shot.toString('base64');
    const art = await page.evaluate(async (b64) => {
      const img = new Image();
      await new Promise((res, rej) => { img.onload = res; img.onerror = rej; img.src = 'data:image/png;base64,' + b64; });
      const W = img.width;
      const cv = document.createElement('canvas'); cv.width = W; cv.height = img.height;
      const ctx = cv.getContext('2d'); ctx.drawImage(img, 0, 0);
      const px = (x, y) => { const d = ctx.getImageData(x, y, 1, 1).data; return [d[0], d[1], d[2]]; };
      const quant = ([r, g, b]) => {
        if (r > 246 && g > 244 && b > 236) return '.';
        if (r > 235 && g > 232 && b > 220) return ',';
        if (b > r + 40 && b > 120) return 'B';
        if (r < 90 && g < 90 && b < 90) return '#';
        if (r > 180 && g < 120 && b < 120) return 'R';
        return '+';
      };
      const lines = [];
      for (let y = 0; y < 120; y += 3) {
        let row = '';
        for (let x = 0; x < W; x += Math.max(4, Math.round(W / 220))) row += quant(px(x, y));
        lines.push(String(y).padStart(3) + ' ' + row);
      }
      return lines.join('\n');
    }, b64);
    console.log(art);
  } catch (e) {
    console.log(`===== ${path} @${width} ERROR ${e.message.split('\n')[0]}`);
  }
  await page.close();
}

await look('/', 1920);
await look('/', 1440);
await browser.close();
