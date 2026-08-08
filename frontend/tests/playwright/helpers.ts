// M14-A11Y-01/10：Playwright 共享夹具与记录。
//
// - personas：读取 fixtures/personas.json（seed-personas.mjs 产出）；
// - loginAs(page, persona)：注入该 persona 的 DB 会话 cookie；
// - runAxe(page, label)：@axe-core/playwright 扫描，severe/critical 违规视为
//   P0 失败（M14-A11Y-06），结果附加到 tests/a11y/axe-report.json artifact；
// - appendRecord(...)：把浏览器版本/viewport/locale/commit/报告/人工验收
//   追加到 tests/a11y/records.json（M14-A11Y-10）。
import { expect, type Page } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import { readFileSync, writeFileSync, existsSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execSync } from 'node:child_process';

const __dirname = dirname(fileURLToPath(import.meta.url));
const A11Y_DIR = join(__dirname, '..', 'a11y');
const PERSONAS_PATH = join(__dirname, 'fixtures', 'personas.json');
const AXE_REPORT = join(A11Y_DIR, 'axe-report.json');
const RECORDS_PATH = join(A11Y_DIR, 'records.json');

export interface PersonaRecord {
  username: string;
  password: string | null;
  session: string;
  persona: string;
  user_id: string;
}

export interface Personas {
  personas: Record<string, PersonaRecord>;
  backend: string;
  password: string;
}

let cached: Personas | null = null;

export function personas(): Personas {
  if (!cached) cached = JSON.parse(readFileSync(PERSONAS_PATH, 'utf8')) as Personas;
  return cached;
}

export function sessionCookie(personaName: string): string {
  const record = personas().personas[personaName];
  if (!record) throw new Error(`unknown persona ${personaName}`);
  return record.session;
}

/** 注入 persona 会话 cookie（真实 DB 会话，非 mock）。
 *  `__Host-` 前缀要求 Secure + Path=/ + 无 Domain：addCookies 必须用
 *  domain+path+secure 字段（url+secure 组合会被 Chromium 拒绝），否则
 *  该 cookie 无法注入（Invalid cookie fields）。 */
export async function loginAs(page: Page, personaName: string): Promise<void> {
  await page.context().addCookies([
    {
      name: '__Host-bblbb_session',
      value: sessionCookie(personaName),
      domain: 'localhost',
      path: '/',
      secure: true,
      sameSite: 'Lax'
    }
  ]);
}

/** 会话 Cookie 名常量（与后端一致）。 */
export const SESSION_COOKIE = '__Host-bblbb_session';

/** 读取当前 git commit（证据记录用）。 */
export function currentCommit(): string {
  try {
    return execSync('git rev-parse --short HEAD', { encoding: 'utf8' }).trim();
  } catch {
    return 'unknown';
  }
}

export interface AxeScanResult {
  url: string;
  label: string;
  violations: Array<{
    id: string;
    impact: string | null;
    description: string;
    nodes: number;
    /** 缺陷定位（target/html），供修复证据与人工验收（M14-A11Y-10）。 */
    targets?: string[];
  }>;
  passed: number;
  incomplete: number;
}

export interface AxeReport {
  generatedAt: string;
  commit: string;
  browser: string;
  scans: AxeScanResult[];
}

function loadAxeReport(): AxeReport {
  if (existsSync(AXE_REPORT)) {
    return JSON.parse(readFileSync(AXE_REPORT, 'utf8')) as AxeReport;
  }
  return { generatedAt: new Date().toISOString(), commit: currentCommit(), browser: 'chromium', scans: [] };
}

/** 清空报告文件（a11y-axe.spec.ts 的 beforeAll 调用，保证每次运行全新报告）。 */
export function resetAxeReport(): void {
  const fresh: AxeReport = { generatedAt: new Date().toISOString(), commit: currentCommit(), browser: 'chromium', scans: [] };
  mkdirSync(A11Y_DIR, { recursive: true });
  writeFileSync(AXE_REPORT, `${JSON.stringify(fresh, null, 2)}\n`);
}

function saveAxeReport(report: AxeReport): void {
  mkdirSync(A11Y_DIR, { recursive: true });
  writeFileSync(AXE_REPORT, `${JSON.stringify(report, null, 2)}\n`);
}

const SEVERE_IMPACTS = new Set(['serious', 'critical']);

/**
 * axe 扫描：严重/关键（serious/critical）违规 = P0 阻断（M14-A11Y-06）。
 * 扫描结果并入 tests/a11y/axe-report.json（CI artifact）。
 */
export async function runAxe(page: Page, label: string, opts: { failOnSerious?: boolean } = {}): Promise<AxeScanResult> {
  const failOnSerious = opts.failOnSerious ?? true;
  // SvelteKit hydration 期间 DOM 会被重建（图标/文本 span 短暂缺失），axe 在
  // hydration 竞态下会把过渡态误报为 link-name/link-in-text-block 等违规。
  // 先等待固定 settle 让 hydration 完成，再对「疑似过渡态」违规重扫。
  await page.waitForTimeout(1200);
  const result = await new AxeBuilder({ page })
    .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
    .analyze();

  const violations = result.violations.map((v) => ({
    id: v.id,
    impact: v.impact ?? null,
    description: v.description,
    nodes: v.nodes.length,
    targets: v.nodes.slice(0, 5).map((n) => `${n.target.join(' ')} :: ${n.html.slice(0, 160)}`)
  }));

  const scan: AxeScanResult = {
    url: page.url(),
    label,
    violations,
    passed: result.passes.length,
    incomplete: result.incomplete.length
  };

  const report = loadAxeReport();
  report.scans.push(scan);
  saveAxeReport(report);

  const blocking = violations.filter((v) => v.impact && SEVERE_IMPACTS.has(v.impact));
  if (failOnSerious && blocking.length > 0) {
    // 断言失败但报告已持久化，供后续修复追踪。
    expect
      .soft(blocking, `${label}：serious/critical 违规 = P0 阻断，见 axe-report.json`)
      .toEqual([]);
  }
  return scan;
}

export interface RecordEntry {
  at: string;
  project: string;
  browser: string;
  browserVersion: string;
  viewport: string;
  locale: string;
  commit: string;
  report: string;
  humanAcceptance: string;
}

/** M14-A11Y-10：记录浏览器版本/viewport/locale/commit/报告/人工验收。 */
export function appendRecord(entry: Omit<RecordEntry, 'at'>): void {
  const records: RecordEntry[] = existsSync(RECORDS_PATH)
    ? (JSON.parse(readFileSync(RECORDS_PATH, 'utf8')) as RecordEntry[])
    : [];
  records.push({ ...entry, at: new Date().toISOString() });
  mkdirSync(A11Y_DIR, { recursive: true });
  writeFileSync(RECORDS_PATH, `${JSON.stringify(records, null, 2)}\n`);
}

/** 从 page 提取浏览器版本信息（用于记录）。 */
export async function browserInfo(page: Page): Promise<{ browser: string; version: string }> {
  const ua = await page.evaluate(() => navigator.userAgent);
  return { browser: ua.includes('Mobile') ? 'chromium-mobile' : 'chromium-desktop', version: ua };
}

export const LOCALE = 'zh-CN';

/**
 * 表单填充助手：SvelteKit hydration 会把 hydration 完成前键入的值重置
 * （受控/bind 输入在 hydration 重渲染时回到初始 state）。因此填充后轮询
 * 校验值，被重置则重填，直到稳定（最多 6 次）。这也验证了最终值一定
 * 进入表单 —— 无 JS 场景（javaScriptEnabled=false）无 hydration，直接通过。
 */
export async function stableFill(page: Page, locator: ReturnType<Page['locator']>, value: string): Promise<void> {
  for (let attempt = 0; attempt < 6; attempt += 1) {
    await locator.fill(value);
    const current = await locator.inputValue().catch(() => '');
    if (current === value) return;
    await page.waitForTimeout(120);
  }
  await locator.fill(value);
  await expect(locator).toHaveValue(value);
}
