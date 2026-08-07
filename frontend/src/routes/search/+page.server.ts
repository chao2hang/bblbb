// M08-UI-01/02：公开搜索 SSR。
//
// - load：校验 q/limit/after（src/lib/search.ts 统一入口）→ GET
//   /api/v1/search（公开端点，仍转发会话 Cookie 以命中登录态限流桶）→
//   归一化 SearchPage/平面返回（不拼接正文）；
// - 429 → rateLimited（Retry-After）；challenge_required → challenge 态
//   （M08-CRAWL-06 未启用时不会出现，前端仍给出无障碍回退）；
// - 搜索结果只渲染后端安全投影字段（title/url/excerpt/highlight + 平面行
//   展示字段），任何 body/隐藏正文字段不进 SSR HTML / hydration payload。
import { getAuthed } from '$lib/api/server';
import { normalizeSearchPage, normalizeSearchQuery, SEARCH_LIMIT_DEFAULT } from '$lib/search';
import type { SearchResultView } from '$lib/api/types';
import type { PageServerLoad } from './$types';

export interface SearchPageData {
  q: string;
  limit: number;
  after: string | null;
  /** 查询校验错误（超长/非法 cursor）；null = 无。 */
  invalid: string | null;
  /** 是否已执行过搜索（q 非空）。 */
  searched: boolean;
  results: SearchResultView[];
  nextCursor: string | null;
  hasMore: boolean;
  error: string | null;
  rateLimited: { retryAfterSecs: number | null } | null;
  challenge: { challengeUrl: string | null } | null;
}

export const load: PageServerLoad = async ({ url, cookies, request }): Promise<SearchPageData> => {
  const requestId = request.headers.get('x-request-id');
  const normalized = normalizeSearchQuery({
    q: url.searchParams.get('q'),
    limit: url.searchParams.get('limit'),
    after: url.searchParams.get('after')
  });

  if (!normalized.q) {
    // 引导态：未搜索。noindex 由页面 meta 保证；不发起请求。
    return {
      q: '',
      limit: SEARCH_LIMIT_DEFAULT,
      after: null,
      invalid: null,
      searched: false,
      results: [],
      nextCursor: null,
      hasMore: false,
      error: null,
      rateLimited: null,
      challenge: null
    } satisfies SearchPageData;
  }

  if (normalized.invalid) {
    // 校验失败：不调用 API（防滥用/超长查询），直接给出提示。
    return {
      q: normalized.q,
      limit: normalized.limit,
      after: null,
      invalid: normalized.invalid,
      searched: true,
      results: [],
      nextCursor: null,
      hasMore: false,
      error: null,
      rateLimited: null,
      challenge: null
    } satisfies SearchPageData;
  }

  const params = new URLSearchParams({ q: normalized.q, limit: String(normalized.limit) });
  if (normalized.after) params.set('after', normalized.after);
  const result = await getAuthed<unknown>(
    cookies,
    `/api/v1/search?${params.toString()}`,
    requestId
  );

  if (!result.ok) {
    if (result.code && /challenge/.test(result.code)) {
      // M08-CRAWL-06：要求一次性挑战（未启用时后端不返回此码）。
      return {
        q: normalized.q,
        limit: normalized.limit,
        after: normalized.after,
        invalid: null,
        searched: true,
        results: [],
        nextCursor: null,
        hasMore: false,
        error: null,
        rateLimited: null,
        challenge: { challengeUrl: null }
      } satisfies SearchPageData;
    }
    if (result.status === 429) {
      return {
        q: normalized.q,
        limit: normalized.limit,
        after: normalized.after,
        invalid: null,
        searched: true,
        results: [],
        nextCursor: null,
        hasMore: false,
        error: null,
        rateLimited: { retryAfterSecs: result.retryAfterSecs },
        challenge: null
      } satisfies SearchPageData;
    }
    return {
      q: normalized.q,
      limit: normalized.limit,
      after: normalized.after,
      invalid: null,
      searched: true,
      results: [],
      nextCursor: null,
      hasMore: false,
      error: result.message,
      rateLimited: null,
      challenge: null
    } satisfies SearchPageData;
  }

  const page = normalizeSearchPage(result.data, normalized.q);
  return {
    q: normalized.q,
    limit: normalized.limit,
    after: normalized.after,
    invalid: null,
    searched: true,
    results: page.items,
    nextCursor: page.next_cursor,
    hasMore: page.has_more,
    error: null,
    rateLimited: null,
    challenge: null
  } satisfies SearchPageData;
};
