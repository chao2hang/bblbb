// 标签聚合页（公开 SSR，GAP-FIX-SPEC 四节：/tags/[slug]）。
//
// 数据路径（按后端实际实现，grep backend/src/routes/）：
// 1. 主路径：GET /api/v1/tags/{slug}/posts?after=&limit=——**后端当前未实现**
//    （backend/src/routes/tags.rs 不存在，/api/v1/tags 列表在 boards.rs，
//    无按标签取帖子的路由）。失败时（404 等）不抛 500，走退化路径；
// 2. 退化路径：GET /api/v1/search?tag={slug}——/search 的 SearchQuery 只接受
//    q/limit/after（无 tag 参数，q 必填），当前同样失败 → 页面渲染空态
//    （「这个标签下还没有内容」+ 发布引导），不 500。
// 后端补齐任一端点后本页自动启用真实数据（双形状归一化见下）。
//
// 标签元信息（名称/描述/颜色）来自 GET /api/v1/tags 按 slug 匹配；列表
// 失败或未命中时以 slug 兜底渲染（不泄漏存在性，也不 404）。
import type { PageServerLoad } from './$types';
import { getAuthed } from '$lib/api/server';
import type { PostSummary, Tag } from '$lib/api/types';

export interface TagDetailPageData {
  slug: string;
  /** 标签元信息（/api/v1/tags 按 slug 匹配；未命中为 null）。 */
  tag: Tag | null;
  /** 该标签下的帖子（主/退化路径均不可用时为空数组）。 */
  posts: PostSummary[];
  /** keyset 分页（?after=）。 */
  hasMore: boolean;
  nextCursor: string | null;
  /** 标签帖子端点当前不可用（主+退化路径均失败）→ 空态附提示。 */
  unavailable: boolean;
}

/** 帖子分页双形状（契约 {items,page{next_cursor,has_more}} / 平面形状）。 */
interface PostsPage {
  items?: PostSummary[];
  next_cursor?: string | null;
  has_more?: boolean;
  page?: { next_cursor?: string | null; has_more?: boolean };
}

export const load: PageServerLoad = async ({ params, url, cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const slug = params.slug;
  const after = (url.searchParams.get('after') ?? '').trim() || null;
  const limit = 20;

  const pageParams = new URLSearchParams({ limit: String(limit) });
  if (after) pageParams.set('after', after);

  const [tagsResult, postsResult] = await Promise.all([
    getAuthed<{ items?: Tag[] }>(cookies, '/api/v1/tags', requestId),
    getAuthed<PostsPage>(
      cookies,
      `/api/v1/tags/${encodeURIComponent(slug)}/posts?${pageParams}`,
      requestId
    )
  ]);

  const tag = tagsResult.ok
    ? (tagsResult.data.items ?? []).find((t) => t.slug === slug) ?? null
    : null;

  let posts: PostSummary[] = [];
  let hasMore = false;
  let nextCursor: string | null = null;
  let unavailable = false;

  if (postsResult.ok) {
    const data = postsResult.data;
    posts = data.items ?? [];
    hasMore = data.has_more ?? data.page?.has_more ?? false;
    nextCursor = data.next_cursor ?? data.page?.next_cursor ?? null;
  } else {
    // 退化路径：GET /search?tag=（后端未实现 tag 参数时同样失败 → 空态）。
    const searchParams = new URLSearchParams({ tag: slug, limit: String(limit) });
    const searchResult = await getAuthed<PostsPage>(
      cookies,
      `/api/v1/search?${searchParams}`,
      requestId
    );
    if (searchResult.ok) {
      const data = searchResult.data;
      posts = data.items ?? [];
      hasMore = data.has_more ?? data.page?.has_more ?? false;
      nextCursor = data.next_cursor ?? data.page?.next_cursor ?? null;
    } else {
      // 主路径 + 退化路径均失败：不抛 500，渲染空态 + 引导（error 保持
      // null——这是预期的降级态而非服务错误）。
      unavailable = true;
    }
  }

  return {
    slug,
    tag,
    posts,
    hasMore,
    nextCursor,
    unavailable
  } satisfies TagDetailPageData;
};
