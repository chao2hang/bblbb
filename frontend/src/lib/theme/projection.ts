// M13-THEME-08/UI-03：前端主题 Token 应用器（无 JS 也可用：SSR 注入 data-*，
// 浏览器端仅把已校验的 Token 写到 CSS 自定义属性；不执行任意 CSS/HTML/JS）。
//
// 安全边界：
// - 只接受封闭 key 集合（与后端 THEME_TOKEN_KEYS 一致）——未知 key 忽略；
// - 值只写入 CSS 自定义属性（`--bb-*`），服务端已过封闭 schema 校验；
//   此处再次白名单，防御性拒绝异常内容；
// - 减少动效：`motion.reduced` 为 true 或用户系统偏好时，动画时长强制为 0；
// - 从不把主题数据写入 localStorage/sessionStorage（M13-UI-07）。

export interface ActiveThemeView {
  name: string;
  revision: number;
  tokens: Record<string, unknown>;
  source: 'user_preference' | 'site_default' | 'builtin_default';
}

/** 前端封闭 Token key（与后端 backend/src/theme/mod.rs TOKEN_KEYS 一致）。 */
export const THEME_TOKEN_KEYS: readonly string[] = [
  'color.background',
  'color.surface',
  'color.text',
  'color.muted',
  'color.accent',
  'color.border',
  'font.body',
  'font.mono',
  'radius.control',
  'radius.card',
  'space.density',
  'shadow.card',
  'motion.duration',
  'motion.reduced'
] as const;

/** 前端 token key → CSS 自定义属性名。 */
const TOKEN_CSS_VAR: Record<string, string> = {
  'color.background': '--bb-color-background',
  'color.surface': '--bb-color-surface',
  'color.text': '--bb-color-text',
  'color.muted': '--bb-color-muted',
  'color.accent': '--bb-color-accent',
  'color.border': '--bb-color-border',
  'font.body': '--bb-font-body',
  'font.mono': '--bb-font-mono',
  'radius.control': '--bb-radius-control',
  'radius.card': '--bb-radius-card',
  'space.density': '--bb-space-density',
  'shadow.card': '--bb-shadow-card',
  'motion.duration': '--bb-motion-duration',
  'motion.reduced': '--bb-motion-reduced'
};

/** 值级安全校验（前端防御层；服务端为裁决方）。 */
function safeTokenValue(key: string, value: unknown): string | null {
  if (typeof value !== 'string' && typeof value !== 'boolean') return null;
  const s = String(value);
  if (s.length > 64) return null;
  // 拒绝 CSS/HTML/JS/SVG/远程资源特征（与后端 DANGEROUS_PATTERNS 对齐）。
  if (/[<>{};]|url\(|@import|expression\(|javascript:|data:text\/html|onerror|onload|onclick|&/.test(s)) {
    return null;
  }
  if (key.startsWith('color.') && !/^#[0-9a-fA-F]{3,8}$/.test(s)) return null;
  if (key.startsWith('font.')) {
    const allowed = [
      'system-ui', 'sans-serif', 'serif', 'monospace', 'ui-monospace', '-apple-system',
      'Segoe UI', 'PingFang SC', 'Microsoft YaHei', 'Noto Sans SC', 'Noto Serif SC',
      'Georgia', 'Times New Roman', 'Courier New'
    ];
    if (!allowed.includes(s)) return null;
  }
  if (key.startsWith('radius.')) {
    const m = /^(\d+(?:\.\d+)?)(px|rem)$/.exec(s);
    if (!m) return null;
    const n = Number(m[1]);
    if (!(n >= 0 && n <= 64)) return null;
  }
  if (key === 'space.density' && !['compact', 'comfortable', 'relaxed'].includes(s)) return null;
  if (key === 'shadow.card' && !['none', 'sm', 'md', 'lg'].includes(s)) return null;
  if (key === 'motion.duration') {
    const m = /^(\d+)(ms|s)$/.exec(s);
    if (!m) return null;
    const n = Number(m[1]);
    if (m[2] === 'ms' ? n > 2000 : n > 2) return null;
  }
  return s;
}

/**
 * 把已校验主题 Token 应用到 `document.documentElement` 的 CSS 自定义属性。
 *
 * - 未知/异常值忽略并告警（回退 CSS 默认值），不抛异常；
 * - 应用后写入 `data-theme="<name>"`（SSR 与浏览器一致，避免闪烁）；
 *   name 必须匹配白名单模式，否则回退 `default`（防御层）；
 * - 永不写入 localStorage/sessionStorage。
 */
export function applyThemeTokens(theme: ActiveThemeView, root?: HTMLElement): void {
  const el = root ?? document.documentElement;
  const tokens = theme.tokens ?? {};
  for (const key of THEME_TOKEN_KEYS) {
    const raw = tokens[key];
    const safe = safeTokenValue(key, raw);
    if (safe === null) {
      // 防御性忽略（服务端已校验；这里只防边界）
      continue;
    }
    const cssVar = TOKEN_CSS_VAR[key];
    el.style.setProperty(cssVar, safe);
  }
  el.dataset.theme = /^[a-z0-9-]{1,64}$/.test(theme.name) ? theme.name : 'default';
  el.dataset.themeRevision = String(theme.revision ?? 1);
}

/** 是否启用减少动效：用户偏好数据 `motion.reduced` 或系统 prefers-reduced-motion。 */
export function prefersReducedMotion(
  theme: ActiveThemeView,
  mediaQuery = '(prefers-reduced-motion: reduce)'
): boolean {
  if (theme.tokens?.['motion.reduced'] === true) return true;
  if (typeof window === 'undefined') return false;
  try {
    return window.matchMedia(mediaQuery).matches;
  } catch {
    return false;
  }
}

/**
 * SSR 安全的主题降级视图：后端 5xx/网络错误/解析失败 → 内置 default。
 * 返回的 theme 永不包含 Secret/任意 CSS（默认 tokens 是代码内常量）。
 */
export function fallbackDefaultTheme(): ActiveThemeView {
  return {
    name: 'default',
    revision: 1,
    source: 'builtin_default',
    tokens: {
      'color.background': '#f5f3ed',
      'color.surface': '#ffffff',
      'color.text': '#1f2937',
      'color.muted': '#6b7280',
      'color.accent': '#2563eb',
      'color.border': '#e5e7eb',
      'font.body': 'system-ui',
      'font.mono': 'ui-monospace',
      'radius.control': '0.5rem',
      'radius.card': '0.75rem',
      'space.density': 'comfortable',
      'shadow.card': 'sm',
      'motion.duration': '150ms',
      'motion.reduced': false
    }
  };
}

/**
 * 安全投影：从任意后端响应挑选允许字段（绝不信任响应里的任意 token 名/值）。
 * 返回 null 表示不可用（调用方用 fallbackDefaultTheme）。
 */
export function pickActiveTheme(data: unknown): ActiveThemeView | null {
  if (!data || typeof data !== 'object') return null;
  const d = data as Record<string, unknown>;
  const name = typeof d.name === 'string' && /^[a-z0-9-]{1,64}$/.test(d.name) ? d.name : null;
  const revision = typeof d.revision === 'number' ? d.revision : 1;
  const sourceRaw = d.source;
  const source: ActiveThemeView['source'] =
    sourceRaw === 'user_preference' || sourceRaw === 'site_default' || sourceRaw === 'builtin_default'
      ? sourceRaw
      : 'builtin_default';
  if (!name) return null;
  const tokens: Record<string, unknown> = {};
  if (d.tokens && typeof d.tokens === 'object') {
    const t = d.tokens as Record<string, unknown>;
    for (const key of THEME_TOKEN_KEYS) {
      if (key in t) tokens[key] = t[key];
    }
  }
  return { name, revision, source, tokens };
}
