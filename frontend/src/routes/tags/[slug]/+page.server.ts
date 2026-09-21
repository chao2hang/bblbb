// 标签聚合页（公开 SSR，GAP-FIX-SPEC 四节：/tags/[slug]）。
//
// 数据路径：GET /api/v1/tags/{slug}/posts?after=&limit=；后端
// boards.rs::list_tag_posts 已实现。端点暂时失败时不抛 500，保留安全空态。
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
    posts = (data.items ?? []).map((post) => ({
      ...post,
      author_avatar_attachment_id: post.author?.avatar_attachment_id ?? null,
      author_presentation_tokens: post.author?.presentation_tokens ?? null
    }));
    hasMore = data.has_more ?? data.page?.has_more ?? false;
    nextCursor = data.next_cursor ?? data.page?.next_cursor ?? null;
  } else {
    // 真实端点暂时不可用：不抛 500，渲染空态 + 普通重试提示。
    unavailable = true;
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
