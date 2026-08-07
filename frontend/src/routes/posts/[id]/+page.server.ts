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
import type { AccessSummary, PostAuthor, PostDetail } from '$lib/api/types';

export interface PostDetailPageData {
  post: PostDetail | null;
  /** 会话 Cookie 是否存在（仅 SSR 渲染回复表单的提示；真实鉴权由后端裁决）。 */
  authed: boolean;
  error: string | null;
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
    return { post: null, authed, error: result.message } satisfies PostDetailPageData;
  }
  return { post: pickPost(result.data), authed, error: null } satisfies PostDetailPageData;
};
