// M04-UI-01：帖子详情 SSR —— 服务端取回后端安全投影（转发会话 Cookie），
// 前端只渲染、不做浏览器再裁剪（正文必须来自后端 body_html）。
//
// 公开字段白名单（M00-FRONTEND-09 精神）：与 frontend/src/lib/api/types.ts
// PostDetail 一一对应；任何新字段必须先过权限/隐私评审再补充。
// 可见性契约（M04-VISIBILITY-07）：body_html 仅在 access_summary.unlocked ===
// true 时挑选；未授权请求方后端返回的响应**不含** body_html（字段缺失而非
// null），此处双保险（unlocked 门 + 类型 string 校验）保证受限正文绝不进入
// SSR HTML / hydration payload。
import { error } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { getAuthed } from '$lib/api/server';
import type { AccessSummary, Board, PostAuthor, PostDetail, PublicPresentationTokens, User } from '$lib/api/types';

/** GAP-FIX 社交域：详情响应的 viewer 视角聚合（posts.rs 已注入，
 *  白名单外字段一律不挑）。字段可选——旧投影/测试 fixture 缺失时页面
 *  以 ?? 兜底，不强制 fixture 逐个补字段。 */
export interface PostViewerExtras {
  /** 登录请求方是否已收藏（匿名恒 false）。 */
  viewer_favorited?: boolean;
  favorite_count?: number;
  /** 付费解锁价格（posts.price_coin；后端未投影时缺失）。 */
  price_coin?: number;
}

/** 页面用的详情类型 = 既有安全投影 + viewer 聚合（不进 types.ts）。 */
export type PostDetailPagePost = PostDetail & PostViewerExtras;

/** 「关于作者」侧栏卡：GET /users/{username} 公开投影白名单（M03-PROFILE-01；
 *  post_count/followers/following 为 GAP-FIX 社交域公开计数）。 */
export interface AuthorCardData {
  username: string;
  display_name: string | null;
  level: number;
  signature: string | null;
  /** 公开装扮投影（M07-SHOP-SCHEMA-06）：服务端编译的白名单 Token。 */
  presentation_tokens: Record<string, string | string[]> | null;
  /** 上传头像附件引用（公开；侧栏「关于作者」头像直渲图片）。 */
  avatar_attachment_id: string | null;
  post_count: number;
  followers: number;
  following: number;
}

/** 「所在板块」侧栏卡：GET /boards 列表按 board_id 匹配（板块详情端点按
 *  slug 寻址，帖子详情只带 board_id，故经列表反查；today_post_count 仅
 *  authed 投影返回，UTC 日界内新帖）。 */
export interface BoardCardData {
  slug: string;
  name: string;
  description: string | null;
  post_count: number;
  /** 持久化板块图标（boards.icon；null/未知名由前台回退 slug 视觉映射）。 */
  icon: string | null;
  today_post_count?: number;
}

/** 侧栏板块导航行：与共享 BoardNav.svelte 的 BoardNavItem 结构兼容
 *  （同一次 GET /boards 反查产出，读帖时可跳转其他板块）。
 *  post_count 匿名投影缺失 → 不传（导航行不渲染计数，不假报 0）。 */
export interface BoardNavItemData {
  id: string;
  slug: string;
  name: string;
  post_count?: number;
  icon: string | null;
}

export interface PostDetailPageData {
  post: PostDetailPagePost | null;
  error: string | null;
  /** 关于作者侧栏卡（无作者用户名/拉取失败 → null，主内容不受影响）。 */
  author: AuthorCardData | null;
  /** 所在板块侧栏卡（无 board_id/拉取失败 → null）。 */
  board: BoardCardData | null;
  /** 侧栏板块导航（与 board 同源反查；无 board_id/拉取失败 → 空数组，
   *  BoardNav 整卡不渲染，主内容不受影响）。 */
  boards: BoardNavItemData[];
  /** 当前会话用户（服务端 /me 验证过）。SvelteKit 把根 layout 的 load 数据
   *  并入页面 data，此字段来自 +layout.server.ts（匿名/会话失效/后端不可达
   *  为 null）。页面一切登录态 UI（回复表单、点赞/举报入口）只以此为准——
   *  不得用「Cookie 是否存在」判定：失效 Cookie 会让匿名访客看到回复表单。 */
  user?: User | null;
}

/** 作者白名单：id/username/display_name/level/profile_url/avatar_attachment_id
 *  （契约 Author；avatar_attachment_id 为公开头像附件引用，列表头像直渲图片）。 */
function pickAuthor(raw: unknown): PostAuthor | null {
  if (!raw || typeof raw !== 'object') return null;
  const a = raw as Record<string, unknown>;
  if (typeof a.id !== 'string' || !a.id) return null;
  const out: PostAuthor = { id: a.id };
  if (typeof a.username === 'string') out.username = a.username;
  if (typeof a.display_name === 'string') out.display_name = a.display_name;
  if (typeof a.level === 'number') out.level = a.level;
  if (typeof a.profile_url === 'string') out.profile_url = a.profile_url;
  if (typeof a.avatar_attachment_id === 'string') out.avatar_attachment_id = a.avatar_attachment_id;
  if (a.presentation_tokens && typeof a.presentation_tokens === 'object') {
    out.presentation_tokens = a.presentation_tokens as PublicPresentationTokens;
  }
  return out;
}

/** access_summary 白名单：policy/unlocked/required_level?（契约 AccessSummary）。 */
function pickAccessSummary(raw: unknown): AccessSummary | undefined {
  if (!raw || typeof raw !== 'object') return undefined;
  const a = raw as Record<string, unknown>;
  if (typeof a.policy !== 'string' || typeof a.unlocked !== 'boolean') return undefined;
  const out: AccessSummary = {
    policy: a.policy as AccessSummary['policy'],
    unlocked: a.unlocked
  };
  if (typeof a.required_level === 'number') out.required_level = a.required_level;
  return out;
}

/** 详情投影白名单挑选：任何未列举字段（含 email/凭据/隐藏正文）不进输出。 */
function pickPost(raw: unknown): PostDetail {
  const r = (raw ?? {}) as Record<string, unknown>;
  const access = pickAccessSummary(r.access_summary);
  const out: PostDetail = {
    id: typeof r.id === 'string' ? r.id : '',
    title: typeof r.title === 'string' ? r.title : '',
    created_at: typeof r.created_at === 'number' ? r.created_at : 0,
    updated_at: typeof r.updated_at === 'number' ? r.updated_at : 0
  };
  if (typeof r.post_type === 'string') {
    out.post_type = r.post_type as 'article' | 'discussion';
  }
  if (typeof r.status === 'string') out.status = r.status;
  const author = pickAuthor(r.author);
  if (author) out.author = author;
  if (access) out.access_summary = access;
  if (Array.isArray(r.capabilities)) {
    out.capabilities = r.capabilities.filter((c) => typeof c === 'string') as string[];
  }
  if (typeof r.reply_count === 'number') out.reply_count = r.reply_count;
  if (typeof r.view_count === 'number') out.view_count = r.view_count;
  if (typeof r.closed_at === 'number' || r.closed_at === null) {
    out.closed_at = r.closed_at as number | null;
  }
  // 正文仅对已解锁请求方返回：unlocked 门 + string 校验双保险。
  if (access?.unlocked === true && typeof r.body_html === 'string') {
    out.body_html = r.body_html;
  }
  if (Array.isArray(r.tags)) {
    out.tags = r.tags.filter((t) => typeof t === 'string') as string[];
  }
  return out;
}

/** 作者公开投影白名单（AuthorCardData；bio 已下线，只保留签名；
 *  presentation_tokens 为服务端编译的公开装扮投影，逐槽位白名单渲染）。 */
function pickAuthorCard(raw: unknown): AuthorCardData | null {
  if (!raw || typeof raw !== 'object') return null;
  const a = raw as Record<string, unknown>;
  if (typeof a.username !== 'string' || !a.username) return null;
  const rawTokens = a.presentation_tokens;
  const presentationTokens =
    rawTokens && typeof rawTokens === 'object' && !Array.isArray(rawTokens)
      ? (Object.fromEntries(
          Object.entries(rawTokens as Record<string, unknown>).filter(
            ([k, v]) =>
              typeof k === 'string' &&
              (typeof v === 'string' || (Array.isArray(v) && v.every((x) => typeof x === 'string')))
          )
        ) as Record<string, string | string[]>)
      : null;
  return {
    username: a.username,
    display_name: typeof a.display_name === 'string' ? a.display_name : null,
    level: typeof a.level === 'number' ? a.level : 1,
    signature: typeof a.signature === 'string' ? a.signature : null,
    presentation_tokens: presentationTokens,
    avatar_attachment_id: typeof a.avatar_attachment_id === 'string' ? a.avatar_attachment_id : null,
    post_count: typeof a.post_count === 'number' ? a.post_count : 0,
    followers: typeof a.followers === 'number' ? a.followers : 0,
    following: typeof a.following === 'number' ? a.following : 0
  };
}

/** 辅助卡片获取（getAuthed 失败/网络异常 → null，绝不阻塞主内容 SSR）。 */
async function safeGet<T>(
  cookies: Parameters<typeof getAuthed>[0],
  path: string,
  requestId: string | null
): Promise<T | null> {
  try {
    const result = await getAuthed<T>(cookies, path, requestId);
    return result.ok ? result.data : null;
  } catch {
    return null;
  }
}

export const load: PageServerLoad = async ({ params, cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const id = params.id;
  const result = await getAuthed<unknown>(
    cookies,
    `/api/v1/posts/${encodeURIComponent(id)}`,
    requestId
  );
  if (!result.ok) {
    // 404（不存在/deleted）→ 与后端一致不泄漏存在性。
    if (result.status === 404) throw error(404, '帖子不存在或不可见');
    return {
      post: null,
      error: result.message,
      author: null,
      board: null,
      boards: []
    } satisfies PostDetailPageData;
  }
  const raw = (result.data ?? {}) as Record<string, unknown>;
  // GAP-FIX 社交域：viewer 收藏态 + 计数（posts.rs 详情已注入；仅标量挑选）。
  const post: PostDetailPagePost = {
    ...pickPost(result.data),
    viewer_favorited: raw.viewer_favorited === true,
    favorite_count: typeof raw.favorite_count === 'number' ? raw.favorite_count : 0
  };
  if (typeof raw.price_coin === 'number') post.price_coin = raw.price_coin;

  // ── 侧栏卡片（辅助数据，失败静默降级，不影响正文/回复 SSR 基线）──
  // board_id 只用于服务端反查板块卡，不进 post 输出（既有白名单不含它）。
  const boardId = typeof raw.board_id === 'string' ? raw.board_id : null;
  const authorUsername = post.author?.username ?? null;

  const [authorRaw, boardsRaw] = await Promise.all([
    authorUsername
      ? safeGet<unknown>(cookies, `/api/v1/users/${encodeURIComponent(authorUsername)}`, requestId)
      : Promise.resolve(null),
    boardId
      ? safeGet<{ items?: Board[] }>(cookies, '/api/v1/boards', requestId)
      : Promise.resolve(null)
  ]);

  const author = authorUsername ? pickAuthorCard(authorRaw) : null;
  let board: BoardCardData | null = null;
  // 板块导航行：同一次列表反查的全部板块（服务端已按请求方可见性裁剪），
  // 字段白名单挑选——id/slug/name 恒需，post_count 仅在数值投影时携带。
  const navBoards: BoardNavItemData[] =
    boardId && Array.isArray(boardsRaw?.items)
      ? (boardsRaw!.items as Board[]).flatMap((b) =>
          b && b.id && b.slug && b.name
            ? [
                {
                  id: b.id,
                  slug: b.slug,
                  name: b.name,
                  icon: b.icon ?? null,
                  ...(typeof b.post_count === 'number' ? { post_count: b.post_count } : {})
                }
              ]
            : []
        )
      : [];
  if (boardId && Array.isArray(boardsRaw?.items)) {
    // today_post_count 不在契约 Board 类型里（authed 聚合，GAP-FIX 社交域），
    // 此处局部扩展而不是改 types.ts。
    const match = (boardsRaw.items as Array<Board & { today_post_count?: number }>).find(
      (b) => b && b.id === boardId
    );
    if (match) {
      board = {
        slug: match.slug,
        name: match.name,
        description: match.description ?? null,
        post_count: match.post_count ?? 0,
        icon: match.icon ?? null,
        ...(typeof match.today_post_count === 'number'
          ? { today_post_count: match.today_post_count }
          : {})
      };
    }
  }

  return { post, error: null, author, board, boards: navBoards } satisfies PostDetailPageData;
};
