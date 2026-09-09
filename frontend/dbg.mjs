import { chromium } from 'playwright';
const b = await chromium.launch();
const c = await b.newContext({ ignoreHTTPSErrors: true, viewport:{width:1440,height:1000} });
const p = await c.newPage();
await p.goto('https://127.0.0.1:5173/', { waitUntil:'networkidle', timeout:45000 });
const r = await p.evaluate(() => {
  const el = document.querySelector('.recommend');
  const out = [];
  for (const sheet of document.styleSheets) {
    let rules; try { rules = sheet.cssRules; } catch { continue; }
    for (const rule of rules) {
      if (!rule.selectorText) continue;
      try { if (!el.matches(rule.selectorText)) continue; } catch { continue; }
      const b = rule.style.getPropertyValue('border') || rule.style.getPropertyValue('border-top') || rule.style.getPropertyValue('border-top-width');
      if (b) out.push({ sel: rule.selectorText.slice(0,80), b, prio: rule.style.getPropertyPriority('border') });
    }
  }
  return { cls: el.className, out };
});
console.log(JSON.stringify(r, null, 2));
await b.close();
