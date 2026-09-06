// 发现页（公开 SSR，GAP-FIX-SPEC 四节）：三区块聚合——
//   ① 热门标签（GET /api/v1/tags，按 usage_count 排序前 12，chip 链 /tags/{slug}）；
//   ② 活跃内容（GET /api/v1/posts?sort=popular&limit=8——后端 posts.rs
//      ListPostsQuery 支持 sort=latest|popular，无独立 listPosts 封装，
//      故按 server.ts getAuthed 模式直连；首页的 search 端点不接受空 q，
//      这里不走 /search）；
//   ③ 板块导航（GET /api/v1/boards，BoardCard 卡片）。
// 公开页容错降级：任一区块失败不阻塞整页（区块各自空态）。
import type { PageServerLoad } from './$types';
import { getAuthed } from '$lib/api/server';
import type { Board, PageResult, PostSummary, Tag } from '$lib/api/types';

export interface DiscoverPageData {
  /** 热门标签（usage_count 降序前 12）。 */
  tags: Tag[];
  /** 活跃内容（sort=popular 前 8，已用板块列表补齐 board_slug/board_name）。 */
  posts: PostSummary[];
  /** 板块导航（全部可见板块）。 */
  boards: Board[];
  /** 任一区块失败时的提示（页面仍渲染其余区块）。 */
  error: string | null;
}

/** GET /api/v1/posts 响应：契约 PostPage 为 {items, page{next_cursor,
 *  has_more}}，实现也曾返回平面形状——与 client.ts listComments 相同的双
 * 形状容忍归一化（此处内联，因 client.ts 本批不改）。 */
interface PostsPage {
  items?: PostSummary[];
  next_cursor?: string | null;
  has_more?: boolean;
  page?: { next_cursor?: string | null; has_more?: boolean };
}

function normalizePostsPage(data: PostsPage): PostSummary[] {
  return data.items ?? [];
}

export const load: PageServerLoad = async ({ cookies, request }) => {
  const requestId = request.headers.get('x-request-id');

  const [tagsResult, postsResult, boardsResult] = await Promise.all([
    getAuthed<{ items?: Tag[]; groups?: unknown[] }>(cookies, '/api/v1/tags', requestId),
    getAuthed<PostsPage>(cookies, '/api/v1/posts?sort=popular&limit=8', requestId),
    getAuthed<PageResult<Board>>(cookies, '/api/v1/boards', requestId)
  ]);

  const boards = boardsResult.ok ? (boardsResult.data.items ?? []) : [];
  // board_id → 板块信息（活跃内容行补齐 board_slug/board_name 供 PostList 徽标）。
  const boardById = new Map(boards.map((b) => [b.id, b]));

  const posts = normalizePostsPage(postsResult.ok ? postsResult.data : {}).map((p) => {
    const board = p.board_id ? boardById.get(p.board_id) : undefined;
    return {
      ...p,
      board_slug: board?.slug ?? null,
      board_name: board?.name ?? null,
      pinned: p.pinned ?? Boolean(p.pinned_at)
    } satisfies PostSummary;
  });

  const tags = (tagsResult.ok ? (tagsResult.data.items ?? []) : [])
    .slice()
    .sort((a, b) => b.usage_count - a.usage_count)
    .slice(0, 12);

  // 公开页容错：任一区块失败时记录提示，其余区块照常渲染。
  const failed = [tagsResult, postsResult, boardsResult].find((r) => !r.ok);

  return {
    tags,
    posts,
    boards,
    error: failed && !failed.ok ? failed.message : null
  } satisfies DiscoverPageData;
};
