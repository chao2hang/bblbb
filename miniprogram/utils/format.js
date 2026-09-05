/**
 * 展示格式化工具。
 * 后端时间戳统一为 Unix 毫秒（created_at/updated_at 等）。
 */

function pad(n) {
  return n < 10 ? `0${n}` : String(n);
}

/**
 * 相对时间：刚刚 / N 分钟前 / N 小时前 / 昨天 / MM-DD / YYYY-MM-DD
 */
function timeAgo(tsMs) {
  if (!tsMs) return '';
  const diff = Date.now() - tsMs;
  if (diff < 60 * 1000) return '刚刚';
  if (diff < 3600 * 1000) return `${Math.floor(diff / 60000)} 分钟前`;
  if (diff < 24 * 3600 * 1000) return `${Math.floor(diff / 3600000)} 小时前`;
  if (diff < 48 * 3600 * 1000) return '昨天';
  const d = new Date(tsMs);
  const now = new Date();
  if (d.getFullYear() === now.getFullYear()) {
    return `${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  }
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** 完整时间：YYYY-MM-DD HH:mm */
function fullTime(tsMs) {
  if (!tsMs) return '';
  const d = new Date(tsMs);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(
    d.getHours()
  )}:${pad(d.getMinutes())}`;
}

/** 数字缩写：1234 → 1.2k；1234567 → 1.2m */
function compactNumber(n) {
  if (n === null || n === undefined) return '0';
  const num = Number(n);
  if (!Number.isFinite(num)) return '0';
  if (num < 10000) return String(num);
  if (num < 1e8) {
    const v = num / 1000;
    return `${v >= 100 ? Math.round(v) : v.toFixed(1).replace(/\.0$/, '')}k`;
  }
  const v = num / 1e8;
  return `${v >= 10 ? Math.round(v) : v.toFixed(1).replace(/\.0$/, '')}亿`;
}

/**
 * 帖子类型展示：article → 文章；discussion/topic → 讨论
 */
function postTypeLabel(postType) {
  if (postType === 'article') return '文章';
  if (postType === 'discussion' || postType === 'topic') return '讨论';
  return postType || '帖子';
}

/** 帖子状态展示 */
function postStatusLabel(status) {
  switch (status) {
    case 'published':
      return '已发布';
    case 'draft':
      return '草稿';
    case 'hidden':
      return '已隐藏';
    case 'deleted':
      return '已删除';
    default:
      return status || '';
  }
}

module.exports = {
  timeAgo,
  fullTime,
  compactNumber,
  postTypeLabel,
  postStatusLabel,
};
