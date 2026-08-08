// M13-THEME-08：前端主题投影安全测试（vitest dom 项目）。
//
// - 封闭 Token：未知 key/危险值（CSS/HTML/JS/SVG/远程资源）被忽略；
// - applyThemeTokens 只写 `--bb-*` 自定义属性 + data-theme；
// - 减少动效：主题数据 motion.reduced 或系统偏好 → true；
// - 隐私守卫：主题数据永不进入 localStorage/sessionStorage；
// - fallback：损坏/缺失 → 内置 default。
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import {
  applyThemeTokens,
  fallbackDefaultTheme,
  pickActiveTheme,
  prefersReducedMotion,
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

  it('封闭 schema：TOKEN_KEYS 与后端一致（14 项）', () => {
    expect(THEME_TOKEN_KEYS).toHaveLength(14);
    expect(THEME_TOKEN_KEYS).toContain('color.background');
    expect(THEME_TOKEN_KEYS).toContain('motion.reduced');
  });

  it('applyThemeTokens 只写白名单 --bb-* 变量与 data-theme，忽略危险值', () => {
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
    expect(inline).toBe('');
    // name 非白名单 → data-theme 回退 default（防御层）
    expect(el.dataset.theme).toBe('default');
    expect(el.dataset.themeRevision).toBe('2');
  });

  it('applyThemeTokens 合法值生效且写入 data-theme', () => {
    const el = document.createElement('div') as unknown as HTMLElement;
    applyThemeTokens(makeTheme(), el);
    expect(el.style.getPropertyValue('--bb-color-background')).toBe('#0f172a');
    expect(el.style.getPropertyValue('--bb-font-body')).toBe('system-ui');
    expect(el.dataset.theme).toBe('midnight');
    expect(el.dataset.themeRevision).toBe('2');
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

  it('fallbackDefaultTheme 总是可用且安全', () => {
    const fb = fallbackDefaultTheme();
    expect(fb.name).toBe('default');
    expect(fb.revision).toBe(1);
    expect(fb.tokens['color.background']).toBe('#f5f3ed');
  });
});
