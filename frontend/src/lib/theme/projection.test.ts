// M13-THEME-08：前端主题投影安全测试（vitest dom 项目）。
//
// - 封闭 Token：未知 key/危险值（CSS/HTML/JS/SVG/远程资源）被忽略；
// - applyThemeTokens 只写 `--bb-*` 自定义属性 + data-theme；
// - layout.mode 是结构预设：写 data-theme-layout 属性（非 CSS 变量），
//   非法/缺失值回退 classic，clear 后恢复 classic；
// - 减少动效：主题数据 motion.reduced 或系统偏好 → true；
// - 隐私守卫：主题数据永不进入 localStorage/sessionStorage；
// - fallback：损坏/缺失 → 内置 default。
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import {
  applyThemeTokens,
  clearThemeTokens,
  fallbackDefaultTheme,
  pickActiveTheme,
  prefersReducedMotion,
  resolveLayoutMode,
  THEME_TOKEN_KEYS
} from './projection';
import type { ActiveThemeView } from './projection';

function makeTheme(overrides: Partial<ActiveThemeView> = {}): ActiveThemeView {
  return {
    name: 'midnight',
    revision: 2,
    source: 'site_default',
    tokens: {
      'color.background': '#0f172a',
      'color.surface': '#1e293b',
      'color.text': '#e2e8f0',
      'color.muted': '#94a3b8',
      'color.accent': '#38bdf8',
      'color.border': '#334155',
      'font.body': 'system-ui',
      'font.mono': 'ui-monospace',
      'radius.control': '0.5rem',
      'radius.card': '0.75rem',
      'space.density': 'comfortable',
      'layout.mode': 'sidebar',
      'shadow.card': 'md',
      'motion.duration': '150ms',
      'motion.reduced': false
    },
    ...overrides
  };
}

describe('M13-THEME 前端投影', () => {
  beforeEach(() => {
    vi.stubGlobal('window', {
      matchMedia: vi.fn(() => ({ matches: false })),
      localStorage: { setItem: vi.fn(), getItem: vi.fn() },
      sessionStorage: { setItem: vi.fn(), getItem: vi.fn() }
    });
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('封闭 schema：TOKEN_KEYS 与后端一致（21 项，含 6 个夜间变体）', () => {
    expect(THEME_TOKEN_KEYS).toHaveLength(21);
    expect(THEME_TOKEN_KEYS).toContain('color.background');
    expect(THEME_TOKEN_KEYS).toContain('color.background.dark');
    expect(THEME_TOKEN_KEYS).toContain('color.accent.dark');
    expect(THEME_TOKEN_KEYS).toContain('motion.reduced');
    expect(THEME_TOKEN_KEYS).toContain('layout.mode');
  });

  it('applyThemeTokens 只写白名单 --bb-* 变量与 data-theme-name，忽略危险值', () => {
    const el = document.createElement('div') as unknown as HTMLElement;
    const evil = makeTheme({
      name: 'evil</style><script>',
      tokens: {
        'color.background': '</style><svg onload=alert(1)>',
        'color.text': 'red; position: fixed',
        'font.body': "url(https://evil.example/x.ttf)",
        'unknown.key': 'x',
        'color.accent': 'expression(alert(1))'
      }
    });
    applyThemeTokens(evil, el);
    // 危险值全部被忽略（未设置任何变量）
    const inline = el.getAttribute('style') ?? '';
    expect(inline).not.toContain('--bb-color-background');
    // name 非白名单 → data-theme-name 回退 default（防御层）；
    // 日夜模式的 dataset.theme 不受影响（本用例未设置 → 保持 undefined）
    expect(el.dataset.themeName).toBe('default');
    expect(el.dataset.theme).toBeUndefined();
    expect(el.dataset.themeRevision).toBe('2');
  });

  it('applyThemeTokens 合法值生效且写入 data-theme-name', () => {
    const el = document.createElement('div') as unknown as HTMLElement;
    applyThemeTokens(makeTheme(), el);
    expect(el.style.getPropertyValue('--bb-color-background')).toBe('#0f172a');
    expect(el.style.getPropertyValue('--bb-font-body')).toBe('system-ui');
    expect(el.dataset.themeName).toBe('midnight');
    expect(el.dataset.themeRevision).toBe('2');
  });

  it('夜间变体 key 投影为 --bb-*-dark 变量（v1.1 双模式）', () => {
    const el = document.createElement('div') as unknown as HTMLElement;
    const dual = makeTheme({
      tokens: {
        ...makeTheme().tokens,
        'color.background.dark': '#0a0f1e',
        'color.accent.dark': '#7dd3fc'
      }
    });
    applyThemeTokens(dual, el);
    expect(el.style.getPropertyValue('--bb-color-background-dark')).toBe('#0a0f1e');
    expect(el.style.getPropertyValue('--bb-color-accent-dark')).toBe('#7dd3fc');
    // 未声明的夜间 key 不残留变量
    expect(el.style.getPropertyValue('--bb-color-surface-dark')).toBe('');

    // 危险夜间值同样被防御层拒绝
    const evil = makeTheme({
      tokens: { ...makeTheme().tokens, 'color.background.dark': '</style><img src=x onerror=alert(1)>' }
    });
    applyThemeTokens(evil, el);
    expect(el.style.getPropertyValue('--bb-color-background-dark')).toBe('');
  });

  it('派生安全变量：密度缩放 / 三级阴影 / 动效时长（closed 映射，非原始输入）', () => {
    const el = document.createElement('div') as unknown as HTMLElement;
    applyThemeTokens(makeTheme(), el); // density=comfortable, shadow=md, duration=150ms
    expect(el.style.getPropertyValue('--bb-density-scale')).toBe('1');
    expect(el.style.getPropertyValue('--bb-shadow-control')).not.toBe('');
    expect(el.style.getPropertyValue('--bb-shadow-pop')).not.toBe('');
    expect(el.style.getPropertyValue('--bb-shadow-modal')).not.toBe('');
    expect(el.style.getPropertyValue('--bb-motion-duration')).toBe('150ms');
    // 原始枚举值绝不直接写入 CSS 变量
    expect(el.style.getPropertyValue('--bb-space-density')).toBe('');
    expect(el.style.getPropertyValue('--bb-shadow-card')).toBe('');
    expect(el.style.getPropertyValue('--bb-motion-reduced')).toBe('');

    // compact → 0.85；切换主题后派生变量不残留
    applyThemeTokens(
      makeTheme({ tokens: { ...makeTheme().tokens, 'space.density': 'compact' } }),
      el
    );
    expect(el.style.getPropertyValue('--bb-density-scale')).toBe('0.85');
    applyThemeTokens(
      makeTheme({ name: 'bare', tokens: { 'color.background': '#111111' } }),
      el
    );
    expect(el.style.getPropertyValue('--bb-density-scale')).toBe('');
    expect(el.style.getPropertyValue('--bb-shadow-control')).toBe('');
  });

  it('属性所有权：dataset.theme（light/dark）归日夜模式所有，数据主题不覆盖', () => {
    const el = document.createElement('div') as unknown as HTMLElement;
    // 日夜系统先写入（app.html 防闪烁脚本 / theme.ts）
    el.dataset.theme = 'dark';
    applyThemeTokens(makeTheme(), el);
    expect(el.dataset.theme).toBe('dark'); // 未被覆盖 → html[data-theme="dark"] 语义保持
    expect(el.dataset.themeName).toBe('midnight');
    clearThemeTokens(el);
    expect(el.dataset.theme).toBe('dark'); // clear 同样不触碰日夜状态
    expect(el.dataset.themeName).toBeUndefined();
  });

  it('减少动效：写 data-motion-reduced 属性（主题声明或系统偏好），clear 时移除', () => {
    const el = document.createElement('div') as unknown as HTMLElement;
    applyThemeTokens(makeTheme(), el);
    expect(el.dataset.motionReduced).toBeUndefined();

    applyThemeTokens(
      makeTheme({ tokens: { ...makeTheme().tokens, 'motion.reduced': true } }),
      el
    );
    expect(el.dataset.motionReduced).toBe('true');

    // 系统偏好（主题未声明）
    vi.stubGlobal('window', { matchMedia: vi.fn(() => ({ matches: true })) });
    const el2 = document.createElement('div') as unknown as HTMLElement;
    applyThemeTokens(makeTheme(), el2);
    expect(el2.dataset.motionReduced).toBe('true');

    clearThemeTokens(el);
    expect(el.dataset.motionReduced).toBeUndefined();
  });

  it('clearThemeTokens 同时清理夜间变体与派生变量', () => {
    const el = document.createElement('div') as unknown as HTMLElement;
    applyThemeTokens(
      makeTheme({
        tokens: { ...makeTheme().tokens, 'color.background.dark': '#0a0f1e' }
      }),
      el
    );
    clearThemeTokens(el);
    expect(el.style.getPropertyValue('--bb-color-background-dark')).toBe('');
    expect(el.style.getPropertyValue('--bb-density-scale')).toBe('');
    expect(el.style.getPropertyValue('--bb-shadow-pop')).toBe('');
    expect(el.style.getPropertyValue('--bb-motion-duration')).toBe('');
  });

  it('layout.mode 写 data-theme-layout（结构属性，非 CSS 变量）', () => {
    const el = document.createElement('div') as unknown as HTMLElement;
    applyThemeTokens(makeTheme(), el);
    // 结构预设走 data 属性驱动 theme-layout.css，不产生 --bb-layout-* 变量
    expect(el.dataset.themeLayout).toBe('sidebar');
    expect(el.style.getPropertyValue('--bb-layout-mode')).toBe('');

    // 换主题缺 layout.mode → 显式回退 classic（不残留上一个主题的 sidebar）
    applyThemeTokens(
      makeTheme({ name: 'legacy-theme', tokens: { 'color.background': '#111111' } }),
      el
    );
    expect(el.dataset.themeLayout).toBe('classic');

    // 非法结构值（未注册预设/注入尝试）被 safeTokenValue 拒绝 → classic
    applyThemeTokens(
      makeTheme({ tokens: { ...makeTheme().tokens, 'layout.mode': 'masonry' } }),
      el
    );
    expect(el.dataset.themeLayout).toBe('classic');
    applyThemeTokens(
      makeTheme({ tokens: { ...makeTheme().tokens, 'layout.mode': '</style><script>' } }),
      el
    );
    expect(el.dataset.themeLayout).toBe('classic');
  });

  it('resolveLayoutMode：合法预设直通，缺失/非法回退 classic', () => {
    expect(resolveLayoutMode({ 'layout.mode': 'classic' })).toBe('classic');
    expect(resolveLayoutMode({ 'layout.mode': 'sidebar' })).toBe('sidebar');
    expect(resolveLayoutMode({ 'layout.mode': 'wide' })).toBe('wide');
    expect(resolveLayoutMode({})).toBe('classic');
    expect(resolveLayoutMode(null)).toBe('classic');
    expect(resolveLayoutMode({ 'layout.mode': 'masonry' })).toBe('classic');
    expect(resolveLayoutMode({ 'layout.mode': 42 })).toBe('classic');
  });

  it('clearThemeTokens 恢复 classic 结构并清理 CSS 变量', () => {
    const el = document.createElement('div') as unknown as HTMLElement;
    applyThemeTokens(makeTheme(), el);
    expect(el.dataset.themeLayout).toBe('sidebar');
    clearThemeTokens(el);
    expect(el.dataset.themeLayout).toBe('classic');
    expect(el.style.getPropertyValue('--bb-color-background')).toBe('');
    expect(el.dataset.themeCustom).toBeUndefined();
  });

  it('减少动效：主题数据 motion.reduced=true 或系统偏好 → true', () => {
    expect(prefersReducedMotion(makeTheme({ tokens: { ...makeTheme().tokens, 'motion.reduced': true } }))).toBe(true);
    expect(prefersReducedMotion(makeTheme())).toBe(false);
    vi.stubGlobal('window', { matchMedia: vi.fn(() => ({ matches: true })) });
    expect(prefersReducedMotion(makeTheme())).toBe(true);
  });

  it('隐私守卫：主题永不写入 localStorage/sessionStorage', () => {
    const lsSet = vi.fn();
    const ssSet = vi.fn();
    vi.stubGlobal('window', {
      matchMedia: vi.fn(() => ({ matches: false })),
      localStorage: { setItem: lsSet, getItem: vi.fn() },
      sessionStorage: { setItem: ssSet, getItem: vi.fn() }
    });
    const el = document.createElement('div') as unknown as HTMLElement;
    applyThemeTokens(makeTheme(), el);
    expect(lsSet).not.toHaveBeenCalled();
    expect(ssSet).not.toHaveBeenCalled();
  });

  it('pickActiveTheme 安全投影：非法 name/null 返回 null；未知 token 不进入结果', () => {
    expect(pickActiveTheme(null)).toBeNull();
    expect(pickActiveTheme({ name: '<script>' })).toBeNull();
    const picked = pickActiveTheme({
      name: 'midnight',
      revision: 3,
      source: 'site_default',
      tokens: { 'color.background': '#000', secret: 'LEAK' }
    });
    expect(picked?.name).toBe('midnight');
    expect(picked?.revision).toBe(3);
    expect(picked?.tokens['secret']).toBeUndefined();
  });

  it('pickActiveTheme：夜间变体 key 进入安全投影', () => {
    const picked = pickActiveTheme({
      name: 'dual-theme',
      revision: 1,
      source: 'site_default',
      tokens: { 'color.background': '#fff', 'color.background.dark': '#111', secret: 'LEAK' }
    });
    expect(picked?.tokens['color.background.dark']).toBe('#111');
    expect(picked?.tokens['secret']).toBeUndefined();
  });

  it('pickActiveTheme：layout.mode 进入安全投影并驱动结构解析', () => {
    const picked = pickActiveTheme({
      name: 'wide-theme',
      revision: 1,
      source: 'site_default',
      tokens: { 'layout.mode': 'wide' }
    });
    expect(picked?.tokens['layout.mode']).toBe('wide');
    expect(resolveLayoutMode(picked?.tokens)).toBe('wide');
  });

  it('fallbackDefaultTheme 总是可用且安全（结构 classic）', () => {
    const fb = fallbackDefaultTheme();
    expect(fb.name).toBe('default');
    expect(fb.revision).toBe(1);
    expect(fb.tokens['color.background']).toBe('#f5f3ed');
    expect(fb.tokens['layout.mode']).toBe('classic');
    expect(resolveLayoutMode(fb.tokens)).toBe('classic');
  });
});
