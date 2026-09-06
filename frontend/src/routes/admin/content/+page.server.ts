// M18-ADMIN-CONTENT：内容审核管理页（对齐原型 #admin-article-audit）。
// 拉取管理端帖子列表（支持 status=pending 筛选）+ 审核动作（approve/reject，写审计）。
import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPost, getAuthed } from '$lib/api/server';

export interface AdminContentPost {
  id: string;
  title: string;
  author_name: string | null;
  board_name: string | null;
  status: string;
  summary: string | null;
  created_at: number;
}

export interface AdminContentPageData {
  posts: AdminContentPost[];
  error: string | null;
  state: 'ok' | 'forbidden' | 'error';
}

export interface AdminContentActionData {
  message?: string;
  error?: string;
}

export const load: PageServerLoad = async ({ cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<{ items: AdminContentPost[] }>(
    cookies,
    '/api/v1/admin/posts?limit=30',
    requestId
  );
  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    if (result.status === 403) return { posts: [], error: '没有审核权限', state: 'forbidden' } satisfies AdminContentPageData;
    return { posts: [], error: result.message, state: 'error' } satisfies AdminContentPageData;
  }
  return {
    posts: result.data.items ?? [],
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
