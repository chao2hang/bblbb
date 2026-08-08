// M14-SEO-05：公开首屏/HTML 大小/JS 预算/图片 lazy-loading/峰值 RSS 记录。
//
// 结果写入 tests/a11y/seo-perf.json（CI artifact，诚实记录测量值；基线值供
// M16-PERF 与发布门槛引用）。
import { expect, test } from '@playwright/test';
import { readFileSync, writeFileSync, existsSync, mkdirSync, readdirSync, statSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execSync } from 'node:child_process';

const __dirname = dirname(fileURLToPath(import.meta.url));
const A11Y_DIR = join(__dirname, '..', 'a11y');
const PERF_PATH = join(A11Y_DIR, 'seo-perf.json');

interface PerfRecord {
  at: string;
  commit: string;
  home: {
    htmlBytes: number;
    firstScreenMs: number; // 首屏可读（domcontentloaded 起算的 SSR 渲染完成）
    imgLazy: number;
    imgTotal: number;
  };
  jsBudget: {
    jsBytes: number;
    budgetBytes: number;
    withinBudget: boolean;
  };
  peakRssMb: number;
  node: string;
}

export function loadPerf(): PerfRecord | null {
  if (existsSync(PERF_PATH)) return JSON.parse(readFileSync(PERF_PATH, 'utf8')) as PerfRecord;
  return null;
}

test.describe('M14-SEO-05 性能记录', () => {
  test('记录公开首屏 p95/HTML 大小/JS 预算/图片 lazy/峰值 RSS', async ({ page }) => {
    // 1) HTML 大小 + 首屏时间（多次采样取 p95）。
    const samples: number[] = [];
    let htmlBytes = 0;
    let imgLazy = 0;
    let imgTotal = 0;
    for (let i = 0; i < 5; i += 1) {
      const start = Date.now();
      const response = await page.goto('/', { waitUntil: 'domcontentloaded' });
      samples.push(Date.now() - start);
      const html = await response!.text();
      htmlBytes = Buffer.byteLength(html, 'utf8');
      const counts = await page.evaluate(() => ({
        lazy: Array.from(document.images).filter((img) => img.loading === 'lazy').length,
        total: document.images.length
      }));
      imgLazy = counts.lazy;
      imgTotal = counts.total;
    }
    samples.sort((a, b) => a - b);
    const p95 = samples[Math.floor(samples.length * 0.95)] ?? samples[samples.length - 1];

    // 2) JS 预算：以生产构建产物 immutable JS 总量为口径（公开首屏实际
    //    传输的 JS，dev 模式的模块按需转换/304 不具代表性）。
    //    构建目录：frontend/build/client/_app/immutable/**/*.js。
    const buildDir = join(__dirname, '..', '..', 'build', 'client', '_app', 'immutable');
    let jsBytes = 0;
    let jsFiles = 0;
    const walkJs = (dir: string) => {
      if (!existsSync(dir)) return;
      for (const name of readdirSync(dir)) {
        const full = join(dir, name);
        const stat = statSync(full);
        if (stat.isDirectory()) walkJs(full);
        else if (name.endsWith('.js')) {
          jsBytes += stat.size;
          jsFiles += 1;
        }
      }
    };
    walkJs(buildDir);
    const budgetBytes = 512 * 1024; // 512KB 初始 JS 预算（M14-SEO-05 基线）
    if (jsFiles === 0) {
      throw new Error(
        `未找到生产构建产物（${buildDir}）：请先运行 npm run build 再执行 perf 记录（JS 预算口径 = 构建 immutable JS）`
      );
    }

    // 3) 峰值 RSS：vite dev / SSR node 进程。
    let peakRssMb = 0;
    try {
      const ps = execSync(
        "ps -axo rss=,command= | grep -E 'vite|svelte' | grep -v grep | awk '{sum+=$1} END {print sum}'",
        { encoding: 'utf8' }
      ).trim();
      peakRssMb = Math.round((Number(ps) || 0) / 1024);
    } catch {
      peakRssMb = -1;
    }

    const commit = execSync('git rev-parse --short HEAD', { encoding: 'utf8' }).trim();
    const record: PerfRecord = {
      at: new Date().toISOString(),
      commit,
      home: { htmlBytes, firstScreenMs: p95, imgLazy, imgTotal },
      jsBudget: { jsBytes, budgetBytes, withinBudget: jsBytes <= budgetBytes },
      peakRssMb,
      node: process.version
    };
    mkdirSync(A11Y_DIR, { recursive: true });
    writeFileSync(PERF_PATH, `${JSON.stringify(record, null, 2)}\n`);

    // 记录完整性断言（不设硬性性能门槛，门槛由 M16-PERF 依据本基线设定）。
    expect(record.home.htmlBytes).toBeGreaterThan(0);
    expect(record.home.firstScreenMs).toBeGreaterThan(0);
    expect(record.jsBudget.jsBytes).toBeGreaterThan(0);
    console.log(`[seo-perf] home.html=${htmlBytes}B firstScreen.p95=${p95}ms jsBudget=${(jsBytes / 1024).toFixed(0)}KB rss=${peakRssMb}MB`);
  });
});
