import { describe, expect, it, vi } from 'vitest';
import {
  detectLanguage,
  enhanceCodeBlock,
  enhanceCodeBlocksIn,
  highlightCodeElement,
  LANGUAGE_ALIASES,
  LANGUAGE_LABELS
} from './code-highlight';

describe('code-highlight 语法高亮与代码块增强', () => {
  it('语言别名与标签完整定义', () => {
    expect(LANGUAGE_ALIASES.ts).toBe('typescript');
    expect(LANGUAGE_ALIASES.js).toBe('javascript');
    expect(LANGUAGE_ALIASES.py).toBe('python');
    expect(LANGUAGE_ALIASES.rs).toBe('rust');
    expect(LANGUAGE_LABELS.typescript).toBe('TypeScript');
    expect(LANGUAGE_LABELS.rust).toBe('Rust');
  });

  describe('detectLanguage 启发式语法推断', () => {
    it('推断 TypeScript / JavaScript 契约代码', () => {
      const tsCode = `import type { PageServerLoad } from './$types';
export const load: PageServerLoad = async ({ fetch }) => {
  const res = await fetch('/api/v1/posts');
  return { posts: [] };
};`;
      expect(detectLanguage(tsCode)).toBe('typescript');
    });

    it('推断 Rust 函数与模块声明', () => {
      const rustCode = `pub fn render_to_html(markdown: &str) -> String {
    let parser = Parser::new(markdown);
    parser.collect()
}`;
      expect(detectLanguage(rustCode)).toBe('rust');
    });

    it('推断 JSON 文本', () => {
      const jsonCode = '{\n  "status": 200,\n  "ok": true\n}';
      expect(detectLanguage(jsonCode)).toBe('json');
    });

    it('推断 HTML 标记', () => {
      const htmlCode = '<div class="container"><p>Hello World</p></div>';
      expect(detectLanguage(htmlCode)).toBe('markup');
    });

    it('推断 SQL 语句', () => {
      const sqlCode = 'SELECT id, title FROM posts WHERE status = "published"';
      expect(detectLanguage(sqlCode)).toBe('sql');
    });

    it('对恶意超长 SQL 探测输入限长且不执行无界 SELECT/FROM 回溯', () => {
      const adversarial = `SELECT${' '.repeat(20_000)}x FROM posts`;
      expect(detectLanguage(adversarial)).toBeNull();
      expect(detectLanguage(`SELECT ${'x'.repeat(600)} FROM posts`)).toBeNull();
    });

    it('推断 Bash 命令', () => {
      const bashCode = 'npm run check && cargo test --all';
      expect(detectLanguage(bashCode)).toBe('bash');
    });

    it('空白或无特征文本返回 null', () => {
      expect(detectLanguage('')).toBeNull();
      expect(detectLanguage('   ')).toBeNull();
      expect(detectLanguage('这是一段普通中文描述没有任何代码特征。')).toBeNull();
    });
  });

  describe('highlightCodeElement DOM 渲染与高亮', () => {
    it('正确将 TypeScript 标记解析为带有 token 类的 span 节点且无 innerHTML 泄漏', () => {
      const pre = document.createElement('pre');
      const code = document.createElement('code');
      code.className = 'language-typescript';
      code.textContent = `import type { PageServerLoad } from './$types';\nexport const load: PageServerLoad = async () => {};`;
      pre.appendChild(code);
      document.body.appendChild(pre);

      const res = highlightCodeElement(code);
      expect(res).toBe(true);
      expect(code.dataset.highlighted).toBe('1');

      const keywords = code.querySelectorAll('span.token.keyword');
      expect(keywords.length).toBeGreaterThanOrEqual(3); // import, type, from, export, const, async
      const texts = Array.from(keywords).map((k) => k.textContent);
      expect(texts).toContain('import');
      expect(texts).toContain('export');
      expect(texts).toContain('const');

      const strings = code.querySelectorAll('span.token.string');
      expect(strings.length).toBeGreaterThanOrEqual(1);

      document.body.removeChild(pre);
    });

    it('未指定语言时通过内容探测自动补充 class 并高亮', () => {
      const pre = document.createElement('pre');
      const code = document.createElement('code');
      code.textContent = `fn main() {\n    println!("Hello, bblbb!");\n}`;
      pre.appendChild(code);
      document.body.appendChild(pre);

      const res = highlightCodeElement(code);
      expect(res).toBe(true);
      expect(code.classList.contains('language-rust')).toBe(true);

      const tokens = code.querySelectorAll('span.token');
      expect(tokens.length).toBeGreaterThan(0);

      document.body.removeChild(pre);
    });
  });

  describe('enhanceCodeBlock 代码块结构与交互', () => {
    it('为 pre 注入语言头部、元信息与复制按钮', async () => {
      const container = document.createElement('div');
      const pre = document.createElement('pre');
      const code = document.createElement('code');
      code.className = 'language-typescript';
      code.textContent = `const x: number = 42;`;
      pre.appendChild(code);
      container.appendChild(pre);
      document.body.appendChild(container);

      const onCopy = vi.fn();
      const wrapper = enhanceCodeBlock(pre, { onCopy });

      expect(wrapper.classList.contains('code-block-wrapper')).toBe(true);
      expect(wrapper.classList.contains('prose-pre-wrap')).toBe(true);

      const header = wrapper.querySelector('.code-block-header');
      expect(header).not.toBeNull();

      const lang = header?.querySelector('.code-block-lang');
      expect(lang?.textContent).toBe('TypeScript');

      const copyBtn = header?.querySelector('.code-copy-btn') as HTMLButtonElement;
      expect(copyBtn).not.toBeNull();
      expect(copyBtn.textContent).toContain('复制');

      // 模拟剪贴板
      Object.assign(navigator, {
        clipboard: {
          writeText: vi.fn().mockResolvedValue(undefined)
        }
      });

      copyBtn.click();
      await new Promise((r) => setTimeout(r, 10));

      expect(navigator.clipboard.writeText).toHaveBeenCalledWith('const x: number = 42;');
      expect(onCopy).toHaveBeenCalledWith('const x: number = 42;');
      expect(copyBtn.classList.contains('is-copied')).toBe(true);
      expect(copyBtn.textContent).toContain('已复制');

      document.body.removeChild(container);
    });

    it('剪贴板 API 不可用时不谎报复制成功', async () => {
      const container = document.createElement('div');
      const pre = document.createElement('pre');
      const code = document.createElement('code');
      code.textContent = 'secret text';
      pre.appendChild(code);
      container.appendChild(pre);
      document.body.appendChild(container);
      const onCopy = vi.fn();
      const wrapper = enhanceCodeBlock(pre, { onCopy });
      Object.defineProperty(navigator, 'clipboard', { configurable: true, value: undefined });

      (wrapper.querySelector('.code-copy-btn') as HTMLButtonElement).click();
      await new Promise((r) => setTimeout(r, 10));

      expect(onCopy).not.toHaveBeenCalled();
      expect(wrapper.querySelector('.code-copy-text')?.textContent).toBe('复制失败');
      document.body.removeChild(container);
    });

    it('enhanceCodeBlocksIn 批量增强容器内全部代码块且不重复挂载', () => {
      const container = document.createElement('div');
      container.innerHTML = `
        <div class="prose">
          <pre><code class="language-js">console.log(1);</code></pre>
          <pre><code class="language-py">print(2)</code></pre>
        </div>
      `;
      document.body.appendChild(container);

      enhanceCodeBlocksIn(container);

      const wrappers = container.querySelectorAll('.code-block-wrapper');
      expect(wrappers.length).toBe(2);

      // 第二次调用幂等
      enhanceCodeBlocksIn(container);
      expect(container.querySelectorAll('.code-block-wrapper').length).toBe(2);

      document.body.removeChild(container);
    });
  });
});
