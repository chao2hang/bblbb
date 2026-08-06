// M04-MARKDOWN-09：无 JS 渲染一致性——正文/代码/链接/图片经 SafeHtml 在
// SSR（服务端渲染）输出完整可见，禁用 JavaScript 也能完整展示。
//
// 配合后端 backend/tests/markdown_consistency.rs：后端产出"无脚本/无事件/
// 无 style"的静态 HTML，前端 SafeHtml 直接注入 SSR 输出——两端共同保证
// 无 JS 环境下内容一致、可读、无注入面。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import SafeHtml from '$lib/components/SafeHtml.svelte';

describe('M04-MARKDOWN-09 无 JS 渲染一致性（SSR）', () => {
  it('纯文本/标题在 SSR 输出可见', () => {
    const { body } = render(SafeHtml, {
      props: { html: '<h2 id="hello">标题</h2><p>正文内容</p>' }
    });
    expect(body).toContain('标题');
    expect(body).toContain('正文内容');
    expect(body).toContain('id="hello"');
  });

  it('代码块在 SSR 输出完整（语言类与内容）', () => {
    const { body } = render(SafeHtml, {
      props: { html: '<pre><code class="language-rust">fn main() {}</code></pre>' }
    });
    expect(body).toContain('language-rust');
    expect(body).toContain('fn main() {}');
  });

  it('链接/图片在 SSR 输出完整（rel/target/alt）', () => {
    const { body } = render(SafeHtml, {
      props: {
        html:
          '<p><a href="https://example.com/a" rel="nofollow noopener noreferrer" target="_blank">文档</a></p>' +
          '<img src="https://example.com/i.png" alt="示意图" />'
      }
    });
    expect(body).toContain('https://example.com/a');
    expect(body).toContain('nofollow noopener noreferrer');
    expect(body).toContain('target="_blank"');
    expect(body).toContain('https://example.com/i.png');
    expect(body).toContain('alt="示意图"');
  });

  it('长文在 SSR 输出逐字节确定（无随机差异）', () => {
    const long = `<p>${'一致内容。'.repeat(500)}</p>`;
    const a = render(SafeHtml, { props: { html: long } });
    const b = render(SafeHtml, { props: { html: long } });
    expect(a.body).toBe(b.body);
    expect(a.body.length).toBeGreaterThan(1000);
  });
});
