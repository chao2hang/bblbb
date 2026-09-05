/**
 * 服务端渲染 HTML 的小程序安全处理。
 *
 * 后端 markdown 渲染器已产出受限 HTML；本模块作为客户端纵深防御：
 * - 剥离 <script>/<style>/<iframe>/<object>/<embed>/<form> 等标签；
 * - 剥离 on* 事件属性与 javascript: 链接；
 * - 保留结构标签供 <rich-text> 渲染（rich-text 本身不执行脚本，
 *   但仍避免把脚本节点传入）。
 *
 * 注意：<rich-text> 不支持外部 CSS；样式由 app.wxss 的
 * .rich-body 作用域选择器提供。
 */

const BLOCKED_TAGS = ['script', 'style', 'iframe', 'object', 'embed', 'form', 'link', 'meta', 'base'];

/** 转义 HTML（用于把纯文本安全地插入 rich-text） */
function escapeHtml(s) {
  return String(s == null ? '' : s)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

/**
 * 净化 HTML 字符串（供 <rich-text nodes> 使用）。
 */
function sanitizeHtml(html) {
  if (!html) return '';
  let out = String(html);

  // 1) 整体移除危险标签及其内容
  BLOCKED_TAGS.forEach((tag) => {
    // 自闭合或成对；多次替换直至消失（处理嵌套）
    let prev = '';
    while (prev !== out) {
      prev = out;
      out = out.replace(
        new RegExp(`<${tag}(\\s[^>]*)?>[\\s\\S]*?<\\/${tag}>`, 'gi'),
        ''
      );
      out = out.replace(new RegExp(`<${tag}(\\s[^>]*)?\\/?>`, 'gi'), '');
    }
  });

  // 2) 剥离 on* 事件属性
  out = out.replace(/\son\w+\s*=\s*("[^"]*"|'[^']*'|[^\s>]+)/gi, '');

  // 3) 中和 javascript: / data: 链接（vbscript: 等）
  out = out.replace(
    /href\s*=\s*(["']?)\s*(javascript|data|vbscript)\s*:[^"'>\s]*\1/gi,
    'href="#"'
  );
  out = out.replace(
    /src\s*=\s*(["']?)\s*(javascript|vbscript)\s*:[^"'>\s]*\1/gi,
    'src=""'
  );

  return out;
}

/**
 * 纯文本 → 简单段落化（用于摘要展示等纯文本场景）。
 */
function textToHtml(text) {
  const escaped = escapeHtml(text).trim();
  if (!escaped) return '';
  return escaped
    .split(/\n{2,}/)
    .map((p) => `<p>${p.replace(/\n/g, '<br>')}</p>`)
    .join('');
}

/** 截取纯文本（去标签），用于列表摘要 */
function stripTags(html) {
  if (!html) return '';
  return String(html)
    .replace(/<[^>]+>/g, '')
    .replace(/&nbsp;/g, ' ')
    .replace(/&amp;/g, '&')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .trim();
}

module.exports = {
  escapeHtml,
  sanitizeHtml,
  textToHtml,
  stripTags,
};
