// M08-UI-01：公开搜索查询校验与分页参数规范化。
//
// 后端契约（openapi searchPublicContent）：q 必填、minLength 1、maxLength 200；
// limit 1–100（后端当前实现 clamp 到 1–50，默认 20）；after 为稳定 cursor。
// 本模块是 SSR load、表单提交与浏览器 client 共用的唯一校验/归一化入口，
// 任何边界值改动先改这里。
import type { SearchResultView, SearchPageView } from '$lib/api/types';

export const SEARCH_QUERY_MAX = 200;
export const SEARCH_QUERY_MIN = 1;
export const SEARCH_LIMIT_MIN = 1;
export const SEARCH_LIMIT_MAX = 50;
export const SEARCH_LIMIT_DEFAULT = 20;
/** cursor 长度上限：防深链接/滥用构造超长 after（服务端同样裁决）。 */
export const SEARCH_CURSOR_MAX = 200;

export interface SearchQueryInput {
  q?: string | null;
  limit?: string | number | null;
  after?: string | null;
}

export interface NormalizedSearchQuery {
  q: string;
  limit: number;
  /** 稳定 cursor；非法/超长时置 null 并给出 invalid（回退到第一页）。 */
  after: string | null;
  /** 校验错误中文提示；null = 无错误。 */
  invalid: string | null;
}

/** 规范化/校验搜索参数。非法输入返回安全默认值并附带用户可读提示。 */
export function normalizeSearchQuery(input: SearchQueryInput): NormalizedSearchQuery {
  const q = String(input.q ?? '').trim();
  if (q.length > SEARCH_QUERY_MAX) {
    return {
      q: q.slice(0, SEARCH_QUERY_MAX),
      limit: normalizeLimit(input.limit),
      after: null,
      invalid: `搜索关键词过长（最多 ${SEARCH_QUERY_MAX} 字符）`
    };
  }
  if (q.length < SEARCH_QUERY_MIN) {
    return {
      q: '',
      limit: normalizeLimit(input.limit),
      after: null,
      invalid: null // 空查询不是错误，页面显示引导态
    };
  }
  const after = normalizeCursor(input.after);
  return { q, limit: normalizeLimit(input.limit), after: after.cursor, invalid: after.invalid };
}

function normalizeLimit(raw: string | number | null | undefined): number {
  if (raw === null || raw === undefined || raw === '') return SEARCH_LIMIT_DEFAULT;
  const n = Number(raw);
  if (!Number.isInteger(n)) return SEARCH_LIMIT_DEFAULT;
  if (n < SEARCH_LIMIT_MIN) return SEARCH_LIMIT_MIN;
  if (n > SEARCH_LIMIT_MAX) return SEARCH_LIMIT_MAX;
  return n;
}

/** cursor 仅接受短字符串；超长视为非法（回退第一页并提示）。 */
export function normalizeCursor(raw: string | null | undefined): {
  cursor: string | null;
  invalid: string | null;
} {
  if (!raw) return { cursor: null, invalid: null };
  const trimmed = raw.trim();
  if (trimmed.length > SEARCH_CURSOR_MAX) {
    return { cursor: null, invalid: '分页标记无效，已回到第一页' };
  }
  return { cursor: trimmed, invalid: null };
}

/** 构建搜索页 URL（供分页链接/表单回填共用）。 */
export function searchUrl(q: string, opts: { limit?: number; after?: string | null } = {}): string {
  const params = new URLSearchParams();
  params.set('q', q);
  if (opts.limit && opts.limit !== SEARCH_LIMIT_DEFAULT) params.set('limit', String(opts.limit));
  if (opts.after) params.set('after', opts.after);
  return `/search?${params.toString()}`;
}

// ── 搜索行/分页归一化（SSR 与浏览器 client 共用；M08-UI-02） ────────────────
//
// 契约 SearchPage：{items: SearchResult[], page: {next_cursor, has_more}}；
// 后端当前实现（backend/src/routes/search.rs）返回平面
// {items, query, next_cursor, has_more}，items 为平面 post 行（无 excerpt）。
// 归一化目标：只保留后端安全投影字段（title/url/excerpt/highlight + 平面行
// 展示字段），绝不拼接/推导正文内容。隐藏正文永远不进输出。

/** 归一化单条搜索结果行。 */
export function normalizeSearchItem(raw: unknown): SearchResultView {
  if (!raw || typeof raw !== 'object') {
    return { id: '', type: 'post', title: '', url: '', excerpt: '' };
  }
  const r = raw as Record<string, unknown>;
  const type = r.type === 'user' || r.type === 'board' || r.type === 'tag' ? r.type : 'post';
  const title = typeof r.title === 'string' ? r.title : '';
  const url =
    typeof r.url === 'string' && r.url
      ? r.url
      : typeof r.id === 'string' && r.id
        ? `/posts/${encodeURIComponent(r.id)}`
        : '';
  const excerpt = typeof r.excerpt === 'string' ? r.excerpt : '';
  const highlight = typeof r.highlight === 'string' ? r.highlight : null;
  const view: SearchResultView = {
    id: typeof r.id === 'string' ? r.id : '',
    type,
    title,
    url,
    excerpt,
    highlight
  };
  // 平面 post 行（后端当前实现）附加展示字段；缺省容忍。
  if (typeof r.board_slug === 'string') view.board_slug = r.board_slug;
  if (typeof r.board_name === 'string') view.board_name = r.board_name;
  if (typeof r.author_id === 'string') view.author_id = r.author_id;
  if (typeof r.author_name === 'string') view.author_name = r.author_name;
  if (typeof r.reply_count === 'number') view.reply_count = r.reply_count;
  if (typeof r.view_count === 'number') view.view_count = r.view_count;
  return view;
}

/** 归一化 SearchPage 响应（兼容嵌套 page 与平面形状）。 */
export function normalizeSearchPage(raw: unknown, fallbackQuery: string): SearchPageView {
  const data = (raw ?? {}) as Record<string, unknown>;
  const items = Array.isArray(data.items) ? data.items.map(normalizeSearchItem) : [];
  const page = (data.page ?? {}) as Record<string, unknown>;
  return {
    items,
    query: typeof data.query === 'string' ? data.query : fallbackQuery,
    next_cursor:
      typeof data.next_cursor === 'string'
        ? data.next_cursor
        : typeof page.next_cursor === 'string'
          ? page.next_cursor
          : null,
    has_more: typeof data.has_more === 'boolean' ? data.has_more : typeof page.has_more === 'boolean' ? page.has_more : false
  };
}
