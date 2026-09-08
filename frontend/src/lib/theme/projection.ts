// M13-THEME-08/UI-03：前端主题 Token 应用器（无 JS 也可用：SSR 注入 data-*，
// 浏览器端仅把已校验的 Token 写到 CSS 自定义属性；不执行任意 CSS/HTML/JS）。
//
// 安全边界：
// - 只接受封闭 key 集合（与后端 THEME_TOKEN_KEYS 一致）——未知 key 忽略；
// - 值只写入 CSS 自定义属性（`--bb-*`），服务端已过封闭 schema 校验；
//   此处再次白名单，防御性拒绝异常内容；
// - v1.1 双模式：6 个 `color.*.dark` key 承载夜间（暗色）配色变体，投影为
//   `--bb-*-dark` 变量；站点 `html.dark` 时由 theme-tokens.css 优先消费；
// - 派生变量（密度/阴影/动效）只由**代码内封闭映射**生成（值来自闭集枚举，
//   绝不写入用户原始输入），供已编译 CSS 的 --space-*/--shadow-*/--duration-*
//   消费；
// - 减少动效：`motion.reduced` 为 true 或用户系统偏好时，写
//   `data-motion-reduced="true"`（theme-tokens.css 把时长强制为 0）；
// - 属性所有权：`dataset.theme`（light/dark）与 light/dark class 归日夜模式
//   系统（$lib/theme.ts）所有；数据型主题名写入独立的 `data-theme-name`，
//   绝不覆盖日夜状态；
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
  // v1.1：夜间（暗色）配色变体（可选；缺失回退日间值）
  'color.background.dark',
  'color.surface.dark',
  'color.text.dark',
  'color.muted.dark',
  'color.accent.dark',
  'color.border.dark',
  'font.body',
  'font.mono',
  'radius.control',
  'radius.card',
  'space.density',
  'layout.mode',
  'shadow.card',
  'motion.duration',
  'motion.reduced'
] as const;

/** 前端已编译的页面结构预设（与后端 LAYOUT_MODE_ALLOWLIST 一致）。 */
export const LAYOUT_MODES: readonly string[] = ['classic', 'sidebar', 'wide'] as const;

/** 布局预设中文标签（管理页选择器/徽标用）。 */
export const LAYOUT_MODE_LABELS: Record<string, string> = {
  classic: '经典顶栏',
  sidebar: '侧栏导航',
  wide: '宽幅全景'
};

/** 解析主题声明的布局预设；未声明/非法值回退 classic（防御层）。 */
export function resolveLayoutMode(tokens: Record<string, unknown> | null | undefined): string {
  const v = tokens?.['layout.mode'];
  return typeof v === 'string' && (LAYOUT_MODES as readonly string[]).includes(v) ? v : 'classic';
}

/** 前端 token key → CSS 自定义属性名。 */
const TOKEN_CSS_VAR: Record<string, string> = {
  'color.background': '--bb-color-background',
  'color.surface': '--bb-color-surface',
  'color.text': '--bb-color-text',
  'color.muted': '--bb-color-muted',
  'color.accent': '--bb-color-accent',
  'color.border': '--bb-color-border',
  // v1.1：夜间变体投影为 *-dark 变量（theme-tokens.css 在 html.dark 下优先消费）
  'color.background.dark': '--bb-color-background-dark',
  'color.surface.dark': '--bb-color-surface-dark',
  'color.text.dark': '--bb-color-text-dark',
  'color.muted.dark': '--bb-color-muted-dark',
  'color.accent.dark': '--bb-color-accent-dark',
  'color.border.dark': '--bb-color-border-dark',
  'font.body': '--bb-font-body',
  'font.mono': '--bb-font-mono',
  'radius.control': '--bb-radius-control',
  'radius.card': '--bb-radius-card',
  'motion.duration': '--bb-motion-duration'
  // 注意：space.density / shadow.card / motion.reduced 不直接写原始值，
  // 由下方封闭映射派生安全变量（见 DERIVED_* 表）；layout.mode 走 data 属性。
};

/** space.density 闭集 → 间距缩放系数（代码内映射，非用户输入）。 */
const DENSITY_SCALE: Record<string, string> = {
  compact: '0.85',
  comfortable: '1',
  relaxed: '1.2'
};

/** shadow.card 闭集 → 三级已编译阴影（控制/弹出/模态；代码内映射，非用户输入）。 */
const SHADOW_PRESETS: Record<string, { control: string; pop: string; modal: string }> = {
  none: {
    control: 'none',
    pop: '0 10px 28px rgba(2, 6, 23, 0.16)',
    modal: '0 24px 64px rgba(2, 6, 23, 0.3)'
  },
  sm: {
    control: '0 1px 2px rgba(23, 33, 31, 0.06)',
    pop: '0 14px 36px rgba(23, 33, 31, 0.11), 0 2px 8px rgba(23, 33, 31, 0.05)',
    modal: '0 28px 80px rgba(23, 33, 31, 0.22), 0 8px 24px rgba(23, 33, 31, 0.1)'
  },
  md: {
    control: '0 2px 4px rgba(23, 33, 31, 0.08)',
    pop: '0 18px 44px rgba(23, 33, 31, 0.16), 0 3px 10px rgba(23, 33, 31, 0.07)',
    modal: '0 32px 90px rgba(23, 33, 31, 0.28), 0 10px 28px rgba(23, 33, 31, 0.12)'
  },
  lg: {
    control: '0 3px 8px rgba(23, 33, 31, 0.12)',
    pop: '0 24px 56px rgba(23, 33, 31, 0.22), 0 4px 14px rgba(23, 33, 31, 0.1)',
    modal: '0 40px 100px rgba(23, 33, 31, 0.34), 0 12px 32px rgba(23, 33, 31, 0.16)'
  }
};

/** 派生安全变量名（apply 前与 clear 时都必须清理，防上一主题残留）。 */
const DERIVED_CSS_VARS = [
  '--bb-density-scale',
  '--bb-shadow-control',
  '--bb-shadow-pop',
  '--bb-shadow-modal'
] as const;

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
  if (key === 'layout.mode' && !(LAYOUT_MODES as readonly string[]).includes(s)) return null;
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
 * - `color.*.dark` 夜间变体投影为 `--bb-*-dark` 变量，由 theme-tokens.css
 *   在 `html.dark`（日夜模式的暗色态）下优先消费；缺失时回退日间值；
 * - `space.density` / `shadow.card` 通过**代码内封闭映射**派生安全变量
 *   （`--bb-density-scale` 缩放系数、`--bb-shadow-control/pop/modal` 三级
 *   已编译阴影），绝不写入用户原始输入；
 * - `motion.reduced` 或系统 prefers-reduced-motion → 写
 *   `data-motion-reduced="true"`（theme-tokens.css 将时长强制为 0）；
 * - `layout.mode` 是结构预设而非样式值：写入 `data-theme-layout` 属性
 *   （classic/sidebar/wide 闭集，驱动 theme-layout.css 的已编译结构变体），
 *   不写 CSS 自定义属性；缺失/非法回退 classic；
 * - 应用后写入 `data-theme-name="<name>"`（name 必须匹配白名单模式，
 *   否则回退 `default`，防御层）。`dataset.theme`（light/dark）归日夜模式
 *   系统（$lib/theme.ts）所有，本函数绝不覆盖——双模式主题据此在
 *   日/夜两种站点模式下分别取用日间/夜间色板；
 * - 永不写入 localStorage/sessionStorage。
 */
export function applyThemeTokens(theme: ActiveThemeView, root?: HTMLElement): void {
  const el = root ?? document.documentElement;
  const tokens = theme.tokens ?? {};
  // 先清空旧 --bb-* 变量与派生变量：切换/预览主题时，上一主题设置而新主题
  // 缺失的 key 不得残留（否则旧颜色/字体/阴影泄漏到新主题渲染中）。
  for (const key of THEME_TOKEN_KEYS) {
    const cssVar = TOKEN_CSS_VAR[key];
    if (cssVar) el.style.removeProperty(cssVar);
  }
  for (const cssVar of DERIVED_CSS_VARS) {
    el.style.removeProperty(cssVar);
  }
  delete el.dataset.motionReduced;
  for (const key of THEME_TOKEN_KEYS) {
    const raw = tokens[key];
    const safe = safeTokenValue(key, raw);
    if (safe === null) {
      // 防御性忽略（服务端已校验；这里只防边界）
      continue;
    }
    if (key === 'layout.mode') {
      // 结构预设走 data 属性，不是 CSS 变量
      continue;
    }
    if (key === 'motion.reduced') {
      // 通过 data-motion-reduced 属性驱动（见下）
      continue;
    }
    if (key === 'space.density') {
      const scale = DENSITY_SCALE[safe];
      if (scale) el.style.setProperty('--bb-density-scale', scale);
      continue;
    }
    if (key === 'shadow.card') {
      const preset = SHADOW_PRESETS[safe];
      if (preset) {
        el.style.setProperty('--bb-shadow-control', preset.control);
        el.style.setProperty('--bb-shadow-pop', preset.pop);
        el.style.setProperty('--bb-shadow-modal', preset.modal);
      }
      continue;
    }
    const cssVar = TOKEN_CSS_VAR[key];
    if (cssVar) el.style.setProperty(cssVar, safe);
  }
  // 减少动效：主题声明或系统偏好 → 强制 0 时长（theme-tokens.css 消费）
  if (prefersReducedMotion(theme)) {
    el.dataset.motionReduced = 'true';
  }
  // 结构预设：显式解析（缺失/非法 → classic），覆盖旧值防残留
  const layout = resolveLayoutMode(tokens);
  el.dataset.themeLayout = layout;
  if (!root) {
    // 全局应用路径：同步 SSR 渲染的 .app-shell 壳属性（预览/正式切换一致）
    document.querySelector('.app-shell')?.setAttribute('data-theme-layout', layout);
  }
  // 主题名写入独立属性（防御层：白名单模式，否则 default）；
  // dataset.theme（light/dark）归日夜模式所有，不覆盖。
  el.dataset.themeName = /^[a-z0-9-]{1,64}$/.test(theme.name) ? theme.name : 'default';
  el.dataset.themeRevision = String(theme.revision ?? 1);
  el.dataset.themeCustom = 'true';
}

/** 激活主题全局预览（将 Token 注入 root 并标记 preview）。 */
export function previewThemeTokens(theme: ActiveThemeView, root?: HTMLElement): void {
  const el = root ?? (typeof document !== 'undefined' ? document.documentElement : undefined);
  if (!el) return;
  applyThemeTokens(theme, el);
  el.dataset.themePreview = 'true';
}

/** 清理已注入的 Token CSS 变量（含夜间变体与派生变量）与 preview/custom/name 标记，恢复默认。 */
export function clearThemeTokens(root?: HTMLElement): void {
  const el = root ?? (typeof document !== 'undefined' ? document.documentElement : undefined);
  if (!el) return;
  for (const key of THEME_TOKEN_KEYS) {
    const cssVar = TOKEN_CSS_VAR[key];
    if (cssVar) el.style.removeProperty(cssVar);
  }
  for (const cssVar of DERIVED_CSS_VARS) {
    el.style.removeProperty(cssVar);
  }
  delete el.dataset.themePreview;
  delete el.dataset.themeCustom;
  delete el.dataset.themeName;
  delete el.dataset.motionReduced;
  // 结构预设恢复 classic（与根布局 SSR 绑定的默认值一致）
  el.dataset.themeLayout = 'classic';
  if (!root) {
    document.querySelector('.app-shell')?.setAttribute('data-theme-layout', 'classic');
  }
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
 *
 * 与后端 `theme::default_tokens()` 保持同构：官方日/夜双模式色板
 * （夜间变体对应 tokens.css 的 html.dark 官方暗色板）。
 */
export function fallbackDefaultTheme(): ActiveThemeView {
  return {
    name: 'default',
    revision: 1,
    source: 'builtin_default',
    tokens: {
      'color.background': '#f5f3ed',
      'color.surface': '#fffefb',
      'color.text': '#17211f',
      'color.muted': '#53605b',
      'color.accent': '#b23e2a',
      'color.border': '#d9d6cc',
      'color.background.dark': '#101b19',
      'color.surface.dark': '#172522',
      'color.text.dark': '#f5f3ea',
      'color.muted.dark': '#b5c0ba',
      'color.accent.dark': '#f27759',
      'color.border.dark': '#30433e',
      'font.body': 'system-ui',
      'font.mono': 'ui-monospace',
      'radius.control': '0.375rem',
      'radius.card': '0.5rem',
      'space.density': 'comfortable',
      'layout.mode': 'classic',
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
