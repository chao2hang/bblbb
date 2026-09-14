// 发现页（公开 SSR）：算法推荐流（前端先行，推荐算法后做）。
//   ① 推荐信息流：GET /api/v1/recommendations（兴趣画像 v1；匿名冷启动
//      由后端退化为热门+新鲜度排序）。端点失败时回退
//      GET /api/v1/posts?sort=popular（旧数据路径，保证页面可用）；
//   ② 板块导航（GET /api/v1/boards）：桌面右栏「推荐板块 + 社区服务」。
// 旧版的热门标签 chip / 侧栏话题 / 手动排序 tab（热门|最新|精华）已随
// 「算法推送」产品决策移除——发现页不再提供手动排序入口。
// 公开页容错降级：区块失败不阻塞整页（区块各自空态）。
import type { PageServerLoad } from './$types';
import { getAuthed } from '$lib/api/server';
import type { Board, PageResult, PostSummary } from '$lib/api/types';

export interface DiscoverPageData {
  /** 推荐流内容（推荐行附带算法理由 reason）。 */
  posts: DiscoverFeedItem[];
  /** 板块导航（全部可见板块；桌面右栏用）。 */
  boards: Board[];
  /** 任一区块失败时的提示（页面仍渲染其余区块）。 */
  error: string | null;
}

/** 推荐流行：posts 投影 + 算法推荐理由（推荐端点附加字段）。 */
export type DiscoverFeedItem = PostSummary & { reason?: string | null };

/** GET /api/v1/posts 响应：契约 PostPage 为 {items, page{next_cursor,
 *  has_more}}，实现也曾返回平面形状——与 client.ts listComments 相同的
 *  双形状容忍归一化（此处内联）。 */
interface PostsPage {
  items?: PostSummary[];
  next_cursor?: string | null;
  has_more?: boolean;
  page?: { next_cursor?: string | null; has_more?: boolean };
}

interface RecommendationsResponse {
  items?: DiscoverFeedItem[];
  strategy?: string;
}

function normalizePostsPage(data: PostsPage): PostSummary[] {
  return data.items ?? [];
}

export const load: PageServerLoad = async ({ cookies, request, url }) => {
  // 兼容旧链接（/discover?sort=…）：sort 参数已废弃，读取但不参与取数，
  // 避免旧书签/外链 400。推荐算法就绪后这里改为调用推荐端点。
  void url.searchParams.get('sort');
  const requestId = request.headers.get('x-request-id');

  // 主路径：推荐端点（登录 = 兴趣画像 v1；匿名 = 热度+新鲜度兜底）。
  const recResult = await getAuthed<RecommendationsResponse>(
    cookies,
    '/api/v1/recommendations?limit=8',
    requestId
  );
  // 回退路径：推荐端点不可用时走既有热门列表（保证页面可用）。
  const postsResult = recResult.ok
    ? { ok: true as const, data: recResult.data }
    : await getAuthed<PostsPage>(cookies, '/api/v1/posts?sort=popular&limit=8', requestId);
  const boardsResult = await getAuthed<PageResult<Board>>(cookies, '/api/v1/boards', requestId);

  const boards = boardsResult.ok ? (boardsResult.data.items ?? []) : [];
  // board_id → 板块信息（推荐流行补齐 board_slug/board_name 供行徽标）。
  const boardById = new Map(boards.map((b) => [b.id, b]));

  const rawItems: DiscoverFeedItem[] = postsResult.ok
    ? normalizePostsPage(postsResult.data)
    : [];
  const posts = rawItems.map((p) => {
    const board = p.board_id ? boardById.get(p.board_id) : undefined;
    return {
      ...p,
      board_slug: board?.slug ?? null,
      board_name: board?.name ?? null,
      pinned: p.pinned ?? Boolean(p.pinned_at)
    } satisfies DiscoverFeedItem;
  });

  // 公开页容错：任一区块失败时记录提示，其余区块照常渲染。
  const failed = [postsResult, boardsResult].find((r) => !r.ok);

  return {
    posts,
    boards,
    error: failed && !failed.ok ? failed.message : null
  } satisfies DiscoverPageData;
};

