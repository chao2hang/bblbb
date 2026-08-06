// M04-MARKDOWN-08：Icon 图标渲染——经 SafeHtml 注入静态可信 SVG 标记，
// 必须仍是真正的 SVG 元素（SVG 命名空间）。
import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/svelte';
import Icon from './Icon.svelte';

describe('Icon（SVG 命名空间保持）', () => {
  it('check 图标渲染为 SVG path 元素', () => {
    const { container } = render(Icon, { name: 'check', size: 20 });
    const svg = container.querySelector('svg');
    expect(svg).toBeTruthy();
    const path = svg!.querySelector('path');
    expect(path).toBeTruthy();
    // SVG 命名空间检查：元素必须是 SVGPathElement（而非 HTML 元素）
    expect(path!.namespaceURI).toBe('http://www.w3.org/2000/svg');
    expect(path!.getAttribute('d')).toContain('M20 6');
  });

  it('未知图标渲染为空 svg', () => {
    const { container } = render(Icon, { name: 'no-such-icon' });
    expect(container.querySelector('path')).toBeFalsy();
  });
});
