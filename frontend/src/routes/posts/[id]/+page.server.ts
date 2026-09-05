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
import { getAuthed, SESSION_COOKIE } from '$lib/api/server';
import type { AccessSummary, Board, PostAuthor, PostDetail } from '$lib/api/types';

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
  bio: string | null;
  signature: string | null;
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
  today_post_count?: number;
}

export interface PostDetailPageData {
  post: PostDetailPagePost | null;
  /** 会话 Cookie 是否存在（仅 SSR 渲染回复表单的提示；真实鉴权由后端裁决）。 */
  authed: boolean;
  error: string | null;
  /** 关于作者侧栏卡（无作者用户名/拉取失败 → null，主内容不受影响）。 */
  author: AuthorCardData | null;
  /** 所在板块侧栏卡（无 board_id/拉取失败 → null）。 */
  board: BoardCardData | null;
}

/** 作者白名单：id/username/display_name/level/profile_url（契约 Author）。 */
function pickAuthor(raw: unknown): PostAuthor | null {
  if (!raw || typeof raw !== 'object') return null;
  const a = raw as Record<string, unknown>;
  if (typeof a.id !== 'string' || !a.id) return null;
  const out: PostAuthor = { id: a.id };
  if (typeof a.username === 'string') out.username = a.username;
  if (typeof a.display_name === 'string') out.display_name = a.display_name;
  if (typeof a.level === 'number') out.level = a.level;
  if (typeof a.profile_url === 'string') out.profile_url = a.profile_url;
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
  return out;
}

/** 作者公开投影白名单（AuthorCardData；隐藏 bio/签名等一并不进输出）。 */
function pickAuthorCard(raw: unknown): AuthorCardData | null {
  if (!raw || typeof raw !== 'object') return null;
  const a = raw as Record<string, unknown>;
  if (typeof a.username !== 'string' || !a.username) return null;
  return {
    username: a.username,
    display_name: typeof a.display_name === 'string' ? a.display_name : null,
    level: typeof a.level === 'number' ? a.level : 1,
    bio: typeof a.bio === 'string' ? a.bio : null,
    signature: typeof a.signature === 'string' ? a.signature : null,
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
  const authed = cookies.get(SESSION_COOKIE) !== null;
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
      authed,
      error: result.message,
      author: null,
      board: null
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
        ...(typeof match.today_post_count === 'number'
          ? { today_post_count: match.today_post_count }
          : {})
      };
    }
  }

  return { post, authed, error: null, author, board } satisfies PostDetailPageData;
};
