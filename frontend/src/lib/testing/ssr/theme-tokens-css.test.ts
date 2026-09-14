// M13-THEME 双模式（v1.1）：theme-tokens.css 映射层契约测试。
//
// 该文件是数据型主题 --bb-* → 语义变量的唯一映射点，必须同时满足：
// - 日间解析（区块 A）与夜间解析（区块 B，html.dark 作用域）齐全；
// - 夜间解析优先消费 --bb-*-dark 变体、缺失回退日间值再回退内置夜色板；
// - 全量语义映射覆盖颜色派生/字体/圆角/密度/阴影/动效（已编译 CSS
//   实际消费的变量名），避免“token 写入但编译样式不响应”；
// - 加载顺序：chinese-elegance.css 之后、theme-layout.css 之前。
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const STYLES_DIR = fileURLToPath(new URL('../../styles/', import.meta.url));
const SRC_DIR = fileURLToPath(new URL('../../../', import.meta.url));
const css = readFileSync(`${STYLES_DIR}theme-tokens.css`, 'utf8');
const appCss = readFileSync(`${SRC_DIR}app.css`, 'utf8');

describe('M13-THEME theme-tokens.css 契约', () => {
  it('日/夜解析区块齐全（--bb-active-* 中间变量）', () => {
    // 日间：:root[data-theme-custom] 等选择器解析 6 个活动色
    expect(css).toContain(':root[data-theme-custom="true"]');
    expect(css).toContain(':root[data-theme-preview="true"]');
    expect(css).toContain('.theme-preview-scope');
    for (const v of [
      '--bb-active-background',
      '--bb-active-surface',
      '--bb-active-text',
      '--bb-active-muted',
      '--bb-active-accent',
      '--bb-active-border'
    ]) {
      expect(css).toContain(`${v}: var(--bb-color-${v.slice('--bb-active-'.length)}`);
    }
    // 夜间：html.dark 作用域 + .dark 变体优先
    expect(css).toContain('html.dark[data-theme-custom="true"]');
    expect(css).toContain('html.dark .theme-preview-scope');
    expect(css).toContain('--bb-color-background-dark');
    expect(css).toContain('--bb-color-accent-dark');
    // 回退链：night → day → 内置官方夜色板
    expect(css).toContain('var(--bb-color-background-dark, var(--bb-color-background, #101B19))');
  });

  it('全量语义映射覆盖已编译 CSS 消费的变量', () => {
    // 颜色派生（subtle/inset/hover/tertiary/brand 派生态/边框派生/辅助色）
    for (const v of [
      '--color-bg-page',
      '--color-bg-card',
      '--color-bg-subtle',
      '--color-bg-inset',
      '--color-bg-hover',
      '--color-surface-hover',
      '--color-surface-selected',
      '--color-overlay',
      '--color-text-primary',
      '--color-text-secondary',
      '--color-text-tertiary',
      '--color-text-on-brand',
      '--color-brand',
      '--color-brand-hover',
      '--color-brand-pressed',
      '--color-brand-soft',
      '--color-border',
      '--color-border-muted',
      '--color-border-strong',
      '--color-code-bg',
      '--color-focus-ring',
      '--color-visited',
      '--color-highlight-soft'
    ]) {
      expect(css).toContain(`${v}: `);
    }
    // 字体：规范化全站字族体系（优先 Inter Tight 与 IBM Plex Mono，并映射 serif 与 AUI 变量）
    expect(css).toContain('--font-family-base: var(--bb-font-body');
    expect(css).toContain('"Inter Tight Variable"');
    expect(css).toContain('--font-family-mono: var(--bb-font-mono');
    expect(css).toContain('"IBM Plex Mono"');
    expect(css).toContain('--font-family-serif:');
    expect(css).toContain('--aui-font-ui: var(--font-family-base)');
    expect(css).toContain('--aui-font-mono: var(--font-family-mono)');
    // 圆角：control → sm/md，card → lg/xl
    expect(css).toContain('--radius-sm: var(--bb-radius-control');
    expect(css).toContain('--radius-md: var(--bb-radius-control');
    expect(css).toContain('--radius-lg: var(--bb-radius-card');
    expect(css).toContain('--radius-xl: var(--bb-radius-card');
    // 密度：--space-* 由 --bb-density-scale 缩放
    expect(css).toContain('--space-4: calc(16px * var(--bb-density-scale, 1))');
    expect(css).toContain('--space-16: calc(64px * var(--bb-density-scale, 1))');
    // 三级阴影
    expect(css).toContain('--shadow-control: var(--bb-shadow-control');
    expect(css).toContain('--shadow-pop: var(--bb-shadow-pop');
    expect(css).toContain('--shadow-modal: var(--bb-shadow-modal');
    // 动效时长
    expect(css).toContain('--duration-fast: var(--bb-motion-duration, 140ms)');
    expect(css).toContain('--duration-slow: var(--bb-motion-duration, 320ms)');
    // 减少动效 → 0 时长
    expect(css).toContain(':root[data-motion-reduced="true"]');
    expect(css).toContain('--duration-base: 0ms !important');
  });

  it('映射层位于最后颜色层（chinese-elegance 之后、theme-layout 结构层之前）', () => {
    const iElegance = appCss.indexOf("chinese-elegance.css'");
    const iTokens = appCss.indexOf("theme-tokens.css'");
    const iLayout = appCss.indexOf("theme-layout.css'");
    expect(iElegance).toBeGreaterThanOrEqual(0);
    expect(iTokens).toBeGreaterThan(iElegance);
    expect(iLayout).toBeGreaterThan(iTokens);
  });

  it('旧映射块已移除（单一映射点，防双份漂移）', () => {
    const tokensCss = readFileSync(`${STYLES_DIR}tokens.css`, 'utf8');
    const elegance = readFileSync(`${STYLES_DIR}chinese-elegance.css`, 'utf8');
    // tokens.css 不再直接映射 --bb-* → 语义变量
    expect(tokensCss).not.toMatch(/--color-bg-page:\s*var\(--bb-color-background/);
    // chinese-elegance 组件兜底消费 --bb-active-*，且废弃 html[data-theme="主题名"] 选择器
    expect(elegance).toContain('var(--bb-active-surface) !important');
    expect(elegance).not.toContain('html[data-theme="midnight"]');
    expect(elegance).not.toContain('html[data-theme="cyberpunk"]');
  });
});
