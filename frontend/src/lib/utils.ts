// BBLBB 通用工具（与原型 Atoms 对齐）

export function escapeHtml(text: unknown): string {
  if (text === null || text === undefined) return '';
  return String(text).replace(/[&<>"']/g, (c) => {
    return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]!;
  });
}

/** Unicode 字符数（代理对按 1 计；与后端 PostContent/CommentContent 一致）。 */
export function charCount(text: string): number {
  return [...text].length;
}

export function formatCount(n: number | null | undefined): string {
  if (typeof n !== 'number' || Number.isNaN(n)) return String(n ?? 0);
  if (n >= 100000) return `${(n / 1000).toFixed(0)}k`;
  if (n >= 1000) {
    const v = n / 1000;
    return `${v >= 10 ? Math.round(v) : v.toFixed(1).replace(/\.0$/, '')}k`;
  }
  return String(n);
}

export function formatTime(ts: number | null | undefined): string {
  if (!ts) return '—';
  return new Date(ts * 1000).toLocaleString('zh-CN', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit'
  });
}

export function formatRelative(ts: number | null | undefined): string {
  if (!ts) return '—';
  const diff = Date.now() / 1000 - ts;
  if (diff < 60) return '刚刚';
  if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
  if (diff < 86400 * 7) return `${Math.floor(diff / 86400)} 天前`;
  return new Date(ts * 1000).toLocaleDateString('zh-CN');
}

/** 安全 Markdown 渲染：先整体转义 HTML，再转换语法；
 *  链接只允许 http/https/mailto，杜绝 javascript:/data: 与属性逃逸。 */
export function renderSafeMarkdown(text: string): string {
  let html = escapeHtml(text);
  html = html
    .replace(/^### (.+)$/gm, '<h3>$1</h3>')
    .replace(/^## (.+)$/gm, '<h2>$1</h2>')
    .replace(/^# (.+)$/gm, '<h1>$1</h1>')
    .replace(/^&gt; (.+)$/gm, '<blockquote>$1</blockquote>')
    .replace(/```([\s\S]*?)```/g, (_m, code: string) => `<pre><code>${code}</code></pre>`)
    .replace(/`([^`\n]+)`/g, '<code>$1</code>')
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    .replace(/\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)/g, '<a href="$2" rel="noopener noreferrer" target="_blank">$1</a>')
    .replace(/\[([^\]]+)\]\((mailto:[^)\s]+)\)/g, '<a href="$2">$1</a>')
    .replace(/^- (.+)$/gm, '<li>$1</li>')
    .replace(/\n{2,}/g, '<br /><br />')
    .replace(/\n/g, '<br />');
  return html;
}
