// M04-MARKDOWN-08：SafeHtml 组件（唯一 {@html} sink）。
import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/svelte';
import SafeHtml from './SafeHtml.svelte';

describe('SafeHtml（唯一 {@html} sink）', () => {
  it('渲染后端清洗的 HTML 内容', () => {
    const { container } = render(SafeHtml, { html: '<h2 id="hello">标题</h2><p>正文</p>' });
    const h2 = container.querySelector('h2');
    expect(h2).toBeTruthy();
    expect(h2!.getAttribute('id')).toBe('hello');
    expect(container.querySelector('p')?.textContent).toBe('正文');
  });

  it('空 HTML 不渲染元素', () => {
    const { container } = render(SafeHtml, { html: '' });
    // Svelte 会留下自身的注释锚点，但不应渲染任何元素/文本
    expect(container.querySelector('*')).toBeFalsy();
    expect(container.textContent).toBe('');
  });
});
