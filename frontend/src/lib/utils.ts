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
  const ms = ts > 1e11 ? ts : ts * 1000;
  return new Date(ms).toLocaleString('zh-CN', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit'
  });
}

/** 日期展示（不含时分）：发帖时间等只关心「几月几日」的场景。
 *  当年 → 「M月D日」；跨年补年份 → 「YYYY年M月D日」避免歧义。 */
export function formatDate(ts: number | null | undefined): string {
  if (!ts) return '—';
  const ms = ts > 1e11 ? ts : ts * 1000;
  const d = new Date(ms);
  const opts: Intl.DateTimeFormatOptions = { month: 'long', day: 'numeric' };
  if (d.getFullYear() !== new Date().getFullYear()) opts.year = 'numeric';
  return d.toLocaleDateString('zh-CN', opts);
}

export function formatRelative(ts: number | null | undefined): string {
  if (!ts) return '—';
  const ms = ts > 1e11 ? ts : ts * 1000;
  const diff = (Date.now() - ms) / 1000;
  if (diff < 60) return '刚刚';
  if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
  if (diff < 86400 * 7) return `${Math.floor(diff / 86400)} 天前`;
  return new Date(ms).toLocaleDateString('zh-CN');
}

/** 微信风格会话时间展示：
 *  - 当天：HH:mm
 *  - 昨天：昨天 HH:mm
 *  - 7 天内：星期X HH:mm
 *  - 当年：M月d日 HH:mm
 *  - 跨年：YYYY年M月d日 HH:mm
 */
export function formatChatTime(ts: number | null | undefined): string {
  if (!ts) return '';
  const ms = ts > 1e11 ? ts : ts * 1000;
  const d = new Date(ms);
  const now = new Date();

  const hours = String(d.getHours()).padStart(2, '0');
  const minutes = String(d.getMinutes()).padStart(2, '0');
  const timeStr = `${hours}:${minutes}`;

  const todayStart = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const targetDayStart = new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const diffDays = Math.round((todayStart - targetDayStart) / 86400000);

  if (diffDays === 0) {
    return timeStr;
  }
  if (diffDays === 1) {
    return `昨天 ${timeStr}`;
  }
  if (diffDays > 1 && diffDays < 7) {
    const weekdays = ['星期日', '星期一', '星期二', '星期三', '星期四', '星期五', '星期六'];
    return `${weekdays[d.getDay()]} ${timeStr}`;
  }

  const month = d.getMonth() + 1;
  const day = d.getDate();
  if (d.getFullYear() === now.getFullYear()) {
    return `${month}月${day}日 ${timeStr}`;
  }

  return `${d.getFullYear()}年${month}月${day}日 ${timeStr}`;
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
