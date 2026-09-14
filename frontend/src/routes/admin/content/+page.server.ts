// M18-ADMIN-CONTENT：内容审核管理页（对齐原型 #admin-article-audit）。
// 拉取管理端待审队列（status=pending_review）+ 当前帖全量修订快照
// （GET /api/v1/admin/posts/{id}/revisions，M18-ADMIN-CONTENT-01），
// 服务端取最后两版计算「修改前/后」对比；审核动作（approve/reject，写审计）。
import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPost, getAuthed } from '$lib/api/server';

/** 管理帖子列表行（GET /api/v1/admin/posts 投影，status=pending_review 队列）。 */
export interface AdminContentPost {
  id: string;
  title: string;
  author_username: string | null;
  board_name: string | null;
  status: string;
  review_status: string;
  created_at: number;
}

/** 单版修订快照（GET /api/v1/admin/posts/{id}/revisions；正文仅审核可见）。 */
export interface AdminContentRevision {
  id: string;
  resource_id: string;
  version: number;
  editor: { id: string } | null;
  reason: string | null;
  created_at: number;
  body_markdown: string | null;
  body_html: string | null;
}

/** 修改前/后对比（服务端由最后两版快照计算；from_version=null = 首次提交）。 */
export interface AdminContentDiff {
  from_version: number | null;
  to_version: number;
  reason: string | null;
  before_body: string | null;
  after_body: string;
}

export interface AdminContentPageData {
  posts: AdminContentPost[];
  /** 当前审核对象（?post= 指定或队列第一篇）。 */
  current: AdminContentPost | null;
  /** 版本对比数据；null = 无法对比（原因见 diff_error），审核操作保持禁用。 */
  diff: AdminContentDiff | null;
  diff_error: string | null;
  error: string | null;
  state: 'ok' | 'forbidden' | 'error';
}

export interface AdminContentActionData {
  message?: string;
  error?: string;
}

export const load: PageServerLoad = async ({ cookies, request, url }) => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<{ items: AdminContentPost[] }>(
    cookies,
    '/api/v1/admin/posts?status=pending_review&limit=30',
    requestId
  );
  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    if (result.status === 403)
      return {
        posts: [],
        current: null,
        diff: null,
        diff_error: null,
        error: '没有审核权限',
        state: 'forbidden'
      } satisfies AdminContentPageData;
    return {
      posts: [],
      current: null,
      diff: null,
      diff_error: null,
      error: result.message,
      state: 'error'
    } satisfies AdminContentPageData;
  }
  const posts = result.data.items ?? [];

  // ?post= 选定待审帖（缺省取队列第一篇）；仅接受队列内的 id，避免越权读取。
  const requested = url.searchParams.get('post');
  const current = posts.find((p) => p.id === requested) ?? posts[0] ?? null;

  // 版本对比：取最后两版快照（v1=首发，vN=当前编辑）。无快照或接口失败时
  // diff=null → 审核操作禁用（避免按错误内容审批，前端展示 diff_error）。
  let diff: AdminContentDiff | null = null;
  let diff_error: string | null = null;
  if (current) {
    const revisions = await getAuthed<{ items: AdminContentRevision[] }>(
      cookies,
      `/api/v1/admin/posts/${encodeURIComponent(current.id)}/revisions`,
      requestId
    );
    if (!revisions.ok) {
      diff_error = `无法获取修订快照（${revisions.status}）：${revisions.message ?? '未知错误'}`;
    } else {
      const withBody = (revisions.data.items ?? []).filter(
        (r): r is AdminContentRevision & { body_markdown: string } =>
          typeof r.body_markdown === 'string' && r.body_markdown.trim().length > 0
      );
      if (withBody.length === 0) {
        diff_error = '该帖子没有任何修订快照，无法生成版本对比';
      } else {
        const after = withBody[withBody.length - 1];
        const before = withBody.length > 1 ? withBody[withBody.length - 2] : null;
        diff = {
          from_version: before ? before.version : null,
          to_version: after.version,
          reason: after.reason ?? null,
          before_body: before ? before.body_markdown : null,
          after_body: after.body_markdown
        };
      }
    }
  }

  return {
    posts,
    current,
    diff,
    diff_error,
    error: null,
    state: 'ok'
  } satisfies AdminContentPageData;
};

export const actions: Actions = {
  approve: async ({ request, cookies }) => {
    const form = await request.formData();
    const id = String(form.get('id') ?? '').trim();
    const reason = String(form.get('reason') ?? '通过审核').trim();
    try {
      const result = await authedPost(
        cookies,
        `/api/v1/admin/posts/${encodeURIComponent(id)}/action`,
        { action: 'approve', reason },
        request.headers.get('x-request-id')
      );
      if (result.ok) return { message: '已通过审核并公开发布' } satisfies AdminContentActionData;
      return fail(result.status, { error: result.message } satisfies AdminContentActionData);
    } catch {
      return fail(503, { error: '操作失败，请稍后重试' } satisfies AdminContentActionData);
    }
  },
  reject: async ({ request, cookies }) => {
    const form = await request.formData();
    const id = String(form.get('id') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim();
    if (!reason) {
      return fail(422, { error: '驳回必须填写驳回原因' } satisfies AdminContentActionData);
    }
    try {
      const result = await authedPost(
        cookies,
        `/api/v1/admin/posts/${encodeURIComponent(id)}/action`,
        { action: 'reject', reason },
        request.headers.get('x-request-id')
      );
      if (result.ok) return { message: '已驳回并退回草稿' } satisfies AdminContentActionData;
      return fail(result.status, { error: result.message } satisfies AdminContentActionData);
    } catch {
      return fail(503, { error: '操作失败，请稍后重试' } satisfies AdminContentActionData);
    }
  }
};
