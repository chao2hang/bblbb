// 首页 / 板块分类页共用的公开信息流数据层（M18-BOARD-05：分类页与首页同构）。
//
// 板块分类页（routes/boards/[slug]）自 M18-BOARD-05 起采用与首页完全相同的
// 信息流（GET /api/v1/posts?board_id=…，cursor 分页、参与者预览、右栏推荐位），
// 数据层沉淀到本模块供两页共用——首页保持既有行为，分类页按板块裁剪。
//
// 公开字段白名单（M00-FRONTEND-09 精神）：与 frontend/src/lib/api/types.ts
// PostSummary 投影一一对应；任何新字段必须先过权限/隐私评审再补充。
// presentation_tokens（作者/参与者）是后端编译的公开装扮投影
// （M07-SHOP-SCHEMA-06：key/value 均为注册白名单 Token，无任意 CSS/HTML/URL；
// 未知值在渲染层被 wardrobe tokens 白名单丢弃），与悬浮资料卡同源，可进白名单。
import type { PostSummary } from '$lib/api/client';
import type { PublicPresentationTokens } from '$lib/api/types';

/** 帖子行公开字段白名单（首页/分类页 SSR 输出的精确键集）。 */
export const POST_PUBLIC = [
  'id',
  'title',
  'author_id',
  'author_name',
  'author_display_name',
  'board_id',
  'is_featured',
  'reply_count',
  'view_count',
  'like_count',
  'pinned',
  'created_at',
  'last_reply_at',
  // 参与者预览（楼主之外已发布回复的不同作者；公开 id/username/display_name
  // + 服务端编译的 presentation_tokens 安全投影，经隐私评审后进入白名单——
  // 列表「参与者」列头像栈的数据源，已装备头像框随行渲染）。
  'participants'
] as const;

/** 信息流行投影：键集合与 POST_PUBLIC 完全一致（隐私守卫测试断言的精确键集）。
 *  author_presentation_tokens / author_avatar_attachment_id 由 pickPostRow 从
 *  嵌套 author 投影压平（与 author_display_name 同构），供参与者列楼主头像
 *  渲染已装备头像框与上传头像。 */
export type FeedRow = Pick<PostSummary, (typeof POST_PUBLIC)[number]> & {
  author_presentation_tokens?: PublicPresentationTokens | null;
  author_avatar_attachment_id?: string | null;
};

/** 按白名单挑选字段：`Pick<T, K>` 保证漏掉字段会立即编译失败，无需运行时校验。 */
function pick<T extends object, K extends keyof T>(item: T, keys: readonly K[]): Pick<T, K> {
  const out = {} as Pick<T, K>;
  for (const key of keys) out[key] = item[key];
  return out;
}

/** 帖子行白名单投影：GET /api/v1/posts 为嵌套作者投影（author.id/username），
 *  此处压平为 author_id（与 /search 平面投影同构，PostList 两种都能吃）。
 *  author_display_name 为作者昵称（作者自设的公开展示名，与 username 同为
 *  公开投影字段；列表 chip 优先昵称、缺省回退用户名）。
 *  作者装扮压平到 author_presentation_tokens、上传头像压平到
 *  author_avatar_attachment_id（与 author_display_name 同构，无装扮为 null）；
 *  参与者行的 avatar_attachment_id / presentation_tokens 原样透传，后端未携带
 *  时不写键（与 post_summary_json 的缺省行为一致）。 */
export function pickPostRow(p: PostSummary): FeedRow {
  return {
    id: p.id,
    title: p.title,
    author_id: p.author_id ?? p.author?.id,
    author_name: p.author_name ?? p.author?.username ?? null,
    author_display_name: p.author_display_name ?? p.author?.display_name ?? null,
    board_id: p.board_id ?? null,
    is_featured: p.is_featured ?? Boolean(p.featured_at),
    reply_count: p.reply_count,
    view_count: p.view_count,
    like_count: p.like_count ?? 0,
    pinned: p.pinned ?? Boolean(p.pinned_at),
    created_at: p.created_at,
    last_reply_at: p.last_reply_at ?? null,
    author_presentation_tokens: p.author?.presentation_tokens ?? null,
    author_avatar_attachment_id: p.author?.avatar_attachment_id ?? null,
    participants: (p.participants ?? []).map((u) => {
      const row: NonNullable<PostSummary['participants']>[number] = {
        id: u.id,
        username: u.username ?? null,
        display_name: u.display_name ?? null
      };
      if (u.avatar_attachment_id) row.avatar_attachment_id = u.avatar_attachment_id;
      if (u.presentation_tokens) row.presentation_tokens = u.presentation_tokens;
      return row;
    })
  };
}

/** GET /api/v1/posts 响应：契约 PostPage {items, page{next_cursor,has_more}}
 *  与历史平面形状双容忍（同 client.ts listComments 的归一化策略）。 */
interface PostsPage {
  items?: PostSummary[];
  next_cursor?: string | null;
  has_more?: boolean;
  page?: { next_cursor?: string | null; has_more?: boolean };
}

/** 拉取一页公开信息流（相对路径走同源 /api/v1，SSR 与客户端 loadMore 同构；
 *  会话经同源 Cookie 透传，可见性由后端按请求方裁剪）。 */
export async function fetchPostsPage(
  fetchFn: typeof fetch,
  opts: {
    sort?: string | null;
    limit: number;
    after?: string | null;
    boardId?: string | null;
    tag?: string | null;
  }
): Promise<{ rows: FeedRow[]; nextCursor: string | null; hasMore: boolean }> {
  const params = new URLSearchParams({ limit: String(opts.limit) });
  if (opts.sort) params.set('sort', opts.sort);
  if (opts.boardId) params.set('board_id', opts.boardId);
  if (opts.tag) params.set('tag', opts.tag);
  if (opts.after) params.set('after', opts.after);
  try {
    const response = await fetchFn(`/api/v1/posts?${params.toString()}`, {
      credentials: 'same-origin',
      headers: { Accept: 'application/json' }
    });
    if (!response.ok) throw new Error(`posts page ${response.status}`);
    const data = (await response.json()) as PostsPage;
    return {
      rows: (data.items ?? []).map(pickPostRow),
      nextCursor: data.page?.next_cursor ?? data.next_cursor ?? null,
      hasMore: data.page?.has_more ?? data.has_more ?? false
    };
  } catch {
    return { rows: [], nextCursor: null, hasMore: false };
  }
}
