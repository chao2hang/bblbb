// 全局壳·主题偏好（light / dark / system）—— app.html 防闪烁内联脚本的
// 运行时对应物。Navbar 的切换按钮复用这里的读写与解析逻辑，保证首帧
// 脚本、切换按钮、localStorage 三方对同一份约定达成一致：
//  - localStorage key：`bblbb-theme`，取值 light | dark | system（默认 system）；
//  - tokens.css 由 html.light / html.dark 显式 class 驱动（脚本总是写显式
//    class，`html:not(.light):not(.dark)` 的系统跟随分支仅供无 JS 回退）；
//  - #theme-color meta 跟随解析结果（浏览器地址栏底色，与 --color-bg-page
//    一致）。
// 仅在浏览器调用（onMount / 事件处理器）；SSR 不触碰 document。

/** 主题偏好三态。 */
export type ThemePreference = 'light' | 'dark' | 'system';

/** 解析后的实际主题（system 被折叠为 light/dark）。 */
export type ResolvedTheme = 'light' | 'dark';

export const THEME_STORAGE_KEY = 'bblbb-theme';

/** 与 tokens.css --color-bg-page 对应的地址栏底色。 */
export const THEME_COLOR: Record<ResolvedTheme, string> = {
  light: '#F7F8F8',
  dark: '#0D1014'
};

/** 读取偏好（localStorage 不可用/值非法时回退 system）。 */
export function readPreference(): ThemePreference {
  try {
    const value = localStorage.getItem(THEME_STORAGE_KEY);
    if (value === 'light' || value === 'dark' || value === 'system') return value;
  } catch {
    /* localStorage 不可用（隐私模式等）→ 跟随系统 */
  }
  return 'system';
}

/** 写入偏好（best-effort，失败不抛出）。 */
function writePreference(pref: ThemePreference): void {
  try {
    localStorage.setItem(THEME_STORAGE_KEY, pref);
  } catch {
    /* 忽略：仅失去跨会话记忆，本次切换仍生效 */
  }
}

/** 当前 OS 是否偏好暗色（matchMedia 不可用时回退亮色）。 */
export function systemPrefersDark(): boolean {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return false;
  return window.matchMedia('(prefers-color-scheme: dark)').matches;
}

/** 偏好 → 实际主题（system 折叠）。 */
export function resolveTheme(pref: ThemePreference): ResolvedTheme {
  if (pref === 'dark') return 'dark';
  if (pref === 'light') return 'light';
  return systemPrefersDark() ? 'dark' : 'light';
}

/**
 * 应用偏好到文档：显式 class（light/dark 互斥）、dataset、color-scheme、
 * #theme-color meta。返回解析后的实际主题。
 */
export function applyTheme(pref: ThemePreference): ResolvedTheme {
  const resolved = resolveTheme(pref);
  if (typeof document === 'undefined') return resolved;
  const root = document.documentElement;
  root.classList.remove('light', 'dark');
  root.classList.add(resolved);
  root.dataset.theme = resolved;
  root.dataset.themePreference = pref;
  root.style.colorScheme = resolved;
  const meta = document.querySelector('meta[name="theme-color"]');
  if (meta) meta.setAttribute('content', THEME_COLOR[resolved]);
  return resolved;
}

/** 循环切换：light → dark → system → light。 */
export function cyclePreference(pref: ThemePreference): ThemePreference {
  if (pref === 'light') return 'dark';
  if (pref === 'dark') return 'system';
  return 'light';
}

/** 切换到下一个偏好并立即应用（写 localStorage + 文档状态）。 */
export function switchToNextTheme(): { preference: ThemePreference; resolved: ResolvedTheme } {
  const next = cyclePreference(readPreference());
  writePreference(next);
  const resolved = applyTheme(next);
  return { preference: next, resolved };
}

/** 偏好中文标签（toast / title 反馈用）。 */
export function preferenceLabel(pref: ThemePreference): string {
  if (pref === 'light') return '亮色';
  if (pref === 'dark') return '暗色';
  return '跟随系统';
}
