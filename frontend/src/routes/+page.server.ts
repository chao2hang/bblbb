// M00-FRONTEND-08：无 JavaScript 基线 —— 公开阅读数据在 SSR 阶段取回。
// 无 JS 时首页仍能展示板块、热门标签与最新讨论（数据来自 SSR HTML）。
//
// M00-FRONTEND-09：hydration/预取隐私守卫 —— load 输出只保留公开字段白名单。
// 即使后端意外多返回了邮箱以外的凭据/令牌/隐藏正文，也不会进入 SSR HTML、
// hydration payload（__data.json）或客户端 store（预取与 load 共用同一输出）。
//
// GAP-FIX（首页增强）：
// - 筛选 tab（全部|精华|热门）：直连 GET /api/v1/posts?sort=featured|popular
//   （后端 ListPostsQuery 支持；原走 /search 无 sort 参数）；
// - 加载更多：?after= created_at 游标（无 JS 整页翻页 + JS 客户端追加）；
// - 社区统计卡：GET /api/v1/stats（getPublicStats，公开页容错降级）；
// - 右栏「热门内容」：sort=popular 前 3 条（「换一批」无后端随机端点，不做）。
// M18-BOARD-05：信息流行白名单/取页逻辑沉淀到 $lib/api/feed（分类页共用），
// 本文件只保留首页自身的编排（boards/tags/posts/popular/stats 并发）。
import { listBoards, listTags, getPublicStats, type Board, type Tag } from '$lib/api/client';
import { fetchPostsPage, type FeedRow } from '$lib/api/feed';
import type { PageServerLoad } from './$types';

/** 公开字段白名单：与 frontend/src/lib/api/types.ts 中的投影一一对应。
 *  任何新字段必须先过权限/隐私评审，再补充到白名单与类型。 */
const BOARD_PUBLIC = ['id', 'slug', 'name', 'description', 'post_count', 'is_active', 'icon'] as const;
const TAG_PUBLIC = ['id', 'slug', 'name', 'usage_count'] as const;

/** 首页筛选 tab：'' = 全部（latest 键序）| featured = 精华 | following = 已关注 | popular = 热门。 */
export type HomeSort = '' | 'featured' | 'following' | 'popular';

/** 按白名单挑选字段：`Pick<T, K>` 保证漏掉字段会立即编译失败，无需运行时校验。 */
function pick<T extends object, K extends keyof T>(item: T, keys: readonly K[]): Pick<T, K> {
  const out = {} as Pick<T, K>;
  for (const key of keys) out[key] = item[key];
  return out;
}

/** 社区统计（GET /api/v1/stats；失败降级 null 不渲染卡）。 */
export interface HomeStats {
  members: number;
  posts: number;
  comments: number;
  boards: number;
  tags: number;
}

export interface HomePageData {
  boards: Array<Pick<Board, (typeof BOARD_PUBLIC)[number]>>;
  tags: Array<Pick<Tag, (typeof TAG_PUBLIC)[number]>>;
  posts: FeedRow[];
  /** 当前筛选（'' | featured | following | popular）。 */
  sort: HomeSort;
  /** 当前板块/标签过滤（首页 bottom sheet 选择后仍留在首页）。 */
  boardId: string | null;
  tag: string | null;
  /** 当前 ?after= 游标（无 JS 加载更多回退）。 */
  after: string | null;
  nextCursor: string | null;
  hasMore: boolean;
  stats: HomeStats | null;
  /** 右栏热门内容（sort=popular 前 3，同一白名单投影）。 */
  popular: FeedRow[];
}

export const load: PageServerLoad = async ({ fetch, url }): Promise<HomePageData> => {
  const sortParam = url.searchParams.get('sort') ?? '';
  const sort: HomeSort =
    sortParam === 'featured' || sortParam === 'following' || sortParam === 'popular'
      ? sortParam
      : '';
  const boardId = url.searchParams.get('board_id')?.trim() || null;
  const tag = url.searchParams.get('tag')?.trim() || null;
  const after = url.searchParams.get('after');

  const [boards, tags, postsPage, popularPage, statsResult] = await Promise.allSettled([
    listBoards(fetch),
    listTags(fetch),
    fetchPostsPage(fetch, { sort, limit: 8, after, boardId, tag }),
    // 「热门内容」推荐位（右栏 3 条；固定 popular 排序，无「换一批」——
    // 后端无随机/刷新端点，随机化需后端支持，暂不做）。
    fetchPostsPage(fetch, { sort: 'popular', limit: 3 }),
    getPublicStats(fetch)
  ]);

  const page = postsPage.status === 'fulfilled' ? postsPage.value : { rows: [], nextCursor: null, hasMore: false };
  const popular = popularPage.status === 'fulfilled' ? popularPage.value.rows : [];
  const statsRaw = statsResult.status === 'fulfilled' ? statsResult.value : null;
  const stats: HomeStats | null =
    statsRaw && typeof statsRaw.members === 'number' && typeof statsRaw.posts === 'number'
      ? {
          members: statsRaw.members,
          posts: statsRaw.posts,
          comments: typeof statsRaw.comments === 'number' ? statsRaw.comments : 0,
          boards: typeof statsRaw.boards === 'number' ? statsRaw.boards : 0,
          tags: typeof statsRaw.tags === 'number' ? statsRaw.tags : 0
        }
      : null;

  return {
    boards:
      boards.status === 'fulfilled'
        ? boards.value.items.map((b: Board) => pick(b, BOARD_PUBLIC))
        : [],
    tags:
      tags.status === 'fulfilled'
        ? tags.value.items.map((t: Tag) => pick(t, TAG_PUBLIC))
        : [],
    posts: page.rows,
    sort,
    boardId,
    tag,
    after,
    nextCursor: page.nextCursor,
    hasMore: page.hasMore,
    stats,
    popular
  };
};
