import Prism from 'prismjs';
import 'prismjs/components/prism-markup.js';
import 'prismjs/components/prism-css.js';
import 'prismjs/components/prism-clike.js';
import 'prismjs/components/prism-javascript.js';
import 'prismjs/components/prism-typescript.js';
import 'prismjs/components/prism-jsx.js';
import 'prismjs/components/prism-tsx.js';
import 'prismjs/components/prism-rust.js';
import 'prismjs/components/prism-python.js';
import 'prismjs/components/prism-json.js';
import 'prismjs/components/prism-bash.js';
import 'prismjs/components/prism-sql.js';
import 'prismjs/components/prism-yaml.js';
import 'prismjs/components/prism-go.js';
import 'prismjs/components/prism-c.js';
import 'prismjs/components/prism-cpp.js';
import 'prismjs/components/prism-csharp.js';
import 'prismjs/components/prism-java.js';
import 'prismjs/components/prism-docker.js';
import 'prismjs/components/prism-toml.js';
import 'prismjs/components/prism-markdown.js';
import 'prismjs/components/prism-diff.js';

/** 常见语言名称规范化映射表 */
export const LANGUAGE_ALIASES: Record<string, string> = {
  js: 'javascript',
  jsx: 'jsx',
  ts: 'typescript',
  tsx: 'tsx',
  html: 'markup',
  xml: 'markup',
  svg: 'markup',
  css: 'css',
  scss: 'css',
  less: 'css',
  py: 'python',
  python: 'python',
  rs: 'rust',
  rust: 'rust',
  sh: 'bash',
  bash: 'bash',
  shell: 'bash',
  zsh: 'bash',
  json: 'json',
  yaml: 'yaml',
  yml: 'yaml',
  sql: 'sql',
  go: 'go',
  golang: 'go',
  c: 'c',
  cpp: 'cpp',
  'c++': 'cpp',
  cs: 'csharp',
  csharp: 'csharp',
  'c#': 'csharp',
  java: 'java',
  docker: 'docker',
  dockerfile: 'docker',
  toml: 'toml',
  md: 'markdown',
  markdown: 'markdown',
  diff: 'diff'
};

/** 界面展示的语言标签文案 */
export const LANGUAGE_LABELS: Record<string, string> = {
  javascript: 'JavaScript',
  typescript: 'TypeScript',
  jsx: 'JSX',
  tsx: 'TSX',
  markup: 'HTML',
  css: 'CSS',
  python: 'Python',
  rust: 'Rust',
  bash: 'Bash',
  json: 'JSON',
  yaml: 'YAML',
  sql: 'SQL',
  go: 'Go',
  c: 'C',
  cpp: 'C++',
  csharp: 'C#',
  java: 'Java',
  docker: 'Docker',
  toml: 'TOML',
  markdown: 'Markdown',
  diff: 'Diff'
};

/**
 * 未声明语言标记时的启发式语法识别
 */
const LANGUAGE_DETECTION_LIMIT = 2_000;

export function detectLanguage(code: string): string | null {
  // Never run heuristics or JSON parsing on attacker-controlled full code blocks.
  const trimmed = code.slice(0, LANGUAGE_DETECTION_LIMIT).trim();
  if (!trimmed) return null;

  // JSON
  if ((trimmed.startsWith('{') && trimmed.endsWith('}')) || (trimmed.startsWith('[') && trimmed.endsWith(']'))) {
    try {
      JSON.parse(trimmed);
      return 'json';
    } catch {
      // ignore
    }
  }

  // HTML / XML
  if (/^<(!DOCTYPE|html|div|span|p|svg|table|body|head|form|template)\b/i.test(trimmed)) {
    return 'markup';
  }

  // Rust
  if (
    /\b(fn\s+\w+|impl\s+|pub\s+fn|pub\s+struct|pub\s+enum|let\s+mut\s+|use\s+std::|match\s+\w+\s*\{|println!|->\s*(String|usize|i32|u32|u64|i64|bool|Option|Result))\b/.test(
      trimmed
    )
  ) {
    return 'rust';
  }

  // SQL
  if (/\b(?:SELECT\b[\s\S]{0,500}?\bFROM\b|INSERT\s+INTO|UPDATE\s+\w+\s+SET|DELETE\s+FROM|CREATE\s+TABLE)\b/i.test(trimmed)) {
    return 'sql';
  }

  // Bash / Shell
  if (
    /^(#!\/bin\/(ba)?sh|npm\s+(run|i|install|ci|test)|cargo\s+(build|check|test|run)|git\s+(commit|push|pull|status|checkout|add)|docker\s+run|sudo\s+apt)/m.test(
      trimmed
    ) ||
    /^\$\s+[a-z]/m.test(trimmed)
  ) {
    return 'bash';
  }

  // Python
  if (
    /\b(def\s+\w+\s*\(|class\s+\w+(\([^)]*\))?:|import\s+[a-zA-Z_]\w*(\s+as\s+\w+)?|from\s+[a-zA-Z_]\w*\s+import|elif\s+|if\s+__name__\s*==)/.test(
      trimmed
    ) &&
    trimmed.includes(':') &&
    !trimmed.includes('{') &&
    !trimmed.includes(';')
  ) {
    return 'python';
  }

  // TypeScript / JavaScript
  if (
    /\b(interface\s+\w+|type\s+\w+\s*=|as\s+\w+|:\s*(string|number|boolean|any|void|unknown|never))\b/.test(trimmed) ||
    /\bimport\s+type\b/.test(trimmed) ||
    /\bimport\s+[\s\S]*from\s+['"][^'"]+['"]/.test(trimmed) ||
    /\b(export\s+(default\s+|const|function|class|type|interface)|const\s+\w+\s*=|let\s+\w+\s*=|function\s+\w+\s*\(|async\s+function|=>\s*\{)/.test(
      trimmed
    )
  ) {
    return 'typescript';
  }

  return null;
}

/**
 * 递归将 Prism Token 转换为纯 DOM 节点
 * 严格遵循 DOM 创建规范，杜绝 innerHTML 等注入面。
 */
function appendTokenToNode(token: string | Prism.Token, target: Node, doc: Document): void {
  if (typeof token === 'string') {
    target.appendChild(doc.createTextNode(token));
    return;
  }

  const span = doc.createElement('span');
  const typeClass = token.type;
  const aliasClass = Array.isArray(token.alias) ? token.alias.join(' ') : (token.alias ?? '');
  span.className = `token ${typeClass} ${aliasClass}`.trim();

  if (typeof token.content === 'string') {
    span.textContent = token.content;
  } else if (Array.isArray(token.content)) {
    for (const sub of token.content) {
      appendTokenToNode(sub, span, doc);
    }
  } else if (token.content) {
    appendTokenToNode(token.content as Prism.Token, span, doc);
  }

  target.appendChild(span);
}

/**
 * 为单个 <code> 元素应用语法高亮
 */
export function highlightCodeElement(codeEl: HTMLElement): boolean {
  if (codeEl.dataset.highlighted === '1') {
    return true;
  }

  // 1. 尝试从 class 提取语言（如 language-ts, lang-rust）
  const match = (codeEl.className || '').match(/(?:language|lang)-([a-zA-Z0-9_+#.-]+)/i);
  let rawLang = match ? match[1].toLowerCase() : '';

  const rawText = codeEl.textContent ?? '';
  if (!rawText.trim()) return false;

  // 2. 无语言标记时尝试自动探测
  if (!rawLang) {
    const detected = detectLanguage(rawText);
    if (detected) {
      rawLang = detected;
      codeEl.classList.add(`language-${detected}`);
    }
  }

  const resolvedLang = LANGUAGE_ALIASES[rawLang] || rawLang;
  const grammar = Prism.languages[resolvedLang];

  if (!grammar) {
    codeEl.dataset.highlighted = '1';
    return false;
  }

  const tokens = Prism.tokenize(rawText, grammar);
  const doc = codeEl.ownerDocument;
  const fragment = doc.createDocumentFragment();

  for (const token of tokens) {
    appendTokenToNode(token, fragment, doc);
  }

  codeEl.textContent = '';
  codeEl.appendChild(fragment);
  codeEl.dataset.highlighted = '1';
  return true;
}

export interface EnhanceCodeBlockOptions {
  onCopy?: (text: string) => void;
}

/**
 * 为 <pre> 代码块注入外层容器、语言头部与复制按钮
 */
export function enhanceCodeBlock(pre: HTMLElement, options?: EnhanceCodeBlockOptions): HTMLElement {
  if (pre.dataset.enhanced === '1' || pre.dataset.copyMount === '1') {
    const existingWrapper = pre.closest('.code-block-wrapper, .prose-pre-wrap');
    if (existingWrapper) return existingWrapper as HTMLElement;
  }

  pre.dataset.enhanced = '1';
  pre.dataset.copyMount = '1';

  const code = pre.querySelector('code');
  if (code) {
    highlightCodeElement(code);
  }

  // 解析展示语言标签
  const match = (code?.className || pre.className || '').match(/(?:language|lang)-([a-zA-Z0-9_+#.-]+)/i);
  const rawLang = match ? match[1].toLowerCase() : '';
  const resolvedLang = LANGUAGE_ALIASES[rawLang] || rawLang;
  const displayLang = LANGUAGE_LABELS[resolvedLang] || (rawLang ? rawLang.toUpperCase() : 'Code');

  const doc = pre.ownerDocument;
  const wrapper = doc.createElement('div');
  wrapper.className = 'prose-pre-wrap code-block-wrapper';

  const header = doc.createElement('div');
  header.className = 'code-block-header';

  const meta = doc.createElement('div');
  meta.className = 'code-block-meta';

  const dot = doc.createElement('span');
  dot.className = 'code-block-dot';
  dot.setAttribute('aria-hidden', 'true');
  meta.appendChild(dot);

  const langSpan = doc.createElement('span');
  langSpan.className = 'code-block-lang';
  langSpan.textContent = displayLang;
  meta.appendChild(langSpan);

  header.appendChild(meta);

  // 复制按钮
  const copyBtn = doc.createElement('button');
  copyBtn.type = 'button';
  copyBtn.className = 'code-copy-btn';
  copyBtn.setAttribute('aria-label', `复制 ${displayLang} 代码`);
  copyBtn.setAttribute('title', '复制代码');

  const iconSpan = doc.createElement('span');
  iconSpan.className = 'code-copy-icon';

  const svg = doc.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('width', '13');
  svg.setAttribute('height', '13');
  svg.setAttribute('viewBox', '0 0 24 24');
  svg.setAttribute('fill', 'none');
  svg.setAttribute('stroke', 'currentColor');
  svg.setAttribute('stroke-width', '2');
  svg.setAttribute('stroke-linecap', 'round');
  svg.setAttribute('stroke-linejoin', 'round');

  const rect = doc.createElementNS('http://www.w3.org/2000/svg', 'rect');
  rect.setAttribute('x', '9');
  rect.setAttribute('y', '9');
  rect.setAttribute('width', '13');
  rect.setAttribute('height', '13');
  rect.setAttribute('rx', '2');
  rect.setAttribute('ry', '2');

  const path = doc.createElementNS('http://www.w3.org/2000/svg', 'path');
  path.setAttribute('d', 'M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1');

  svg.appendChild(rect);
  svg.appendChild(path);
  iconSpan.appendChild(svg);
  copyBtn.appendChild(iconSpan);

  const labelSpan = doc.createElement('span');
  labelSpan.className = 'code-copy-text';
  labelSpan.textContent = '复制';
  copyBtn.appendChild(labelSpan);

  let resetTimer: ReturnType<typeof setTimeout> | null = null;
  copyBtn.addEventListener('click', async () => {
    const text = code?.textContent ?? pre.textContent ?? '';
    try {
      if (!navigator.clipboard?.writeText) {
        throw new Error('clipboard_unavailable');
      }
      await navigator.clipboard.writeText(text);
      labelSpan.textContent = '已复制';
      copyBtn.classList.add('is-copied');
      options?.onCopy?.(text);
    } catch {
      labelSpan.textContent = '复制失败';
    }

    if (resetTimer) clearTimeout(resetTimer);
    resetTimer = setTimeout(() => {
      labelSpan.textContent = '复制';
      copyBtn.classList.remove('is-copied');
    }, 2000);
  });

  header.appendChild(copyBtn);

  pre.replaceWith(wrapper);
  wrapper.appendChild(header);
  wrapper.appendChild(pre);

  return wrapper;
}

/**
 * 遍历容器内的全部代码块并应用增强与语法高亮
 */
export function enhanceCodeBlocksIn(container: HTMLElement, options?: EnhanceCodeBlockOptions): void {
  const blocks = container.querySelectorAll('pre > code');
  for (const code of Array.from(blocks)) {
    const pre = code.parentElement;
    if (!pre) continue;
    if (pre.dataset.enhanced === '1' || pre.dataset.copyMount === '1') continue;
    enhanceCodeBlock(pre, options);
  }
}
