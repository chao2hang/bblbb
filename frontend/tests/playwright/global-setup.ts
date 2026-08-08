// M14-A11Y-06/10：Playwright globalSetup —— 每次 `playwright test` 调用只清
// 一次 a11y artifact（axe 报告 / 性能记录 / 验收记录），避免多 worker/
// 多 project 各自的 beforeAll 相互覆盖。
import { rmSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';

export default function globalSetup(): void {
  const a11yDir = join(process.cwd(), 'tests', 'a11y');
  mkdirSync(a11yDir, { recursive: true });
  for (const file of ['axe-report.json', 'seo-perf.json', 'records.json']) {
    try {
      rmSync(join(a11yDir, file), { force: true });
    } catch {
      /* ignore */
    }
  }
}
