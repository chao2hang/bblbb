// M13-UI-01/ADMIN-02：管理用户页——列表/状态更新（If-Match version + reason +
// recent-auth；管理 DTO 不含凭据）。
import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPatch, getAuthed } from '$lib/api/server';

export interface AdminUserItem {
  id: string;
  username: string;
  email: string;
  email_verified: boolean;
  status: string;
  display_name: string | null;
  level: number;
  roles: string[];
  created_at: number;
  updated_at: number;
  last_login_at: number | null;
  version: number;
}

export type AdminUsersLoadState = 'ok' | 'forbidden' | 'not_implemented' | 'error';

export interface AdminUsersPageData {
  state: AdminUsersLoadState;
  items: AdminUserItem[] | null;
  error: string | null;
}

export interface AdminUsersActionData {
  message?: string;
  requestId?: string | null;
  conflict?: boolean;
}

export const load: PageServerLoad = async ({ cookies, request }): Promise<AdminUsersPageData> => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<{ items: AdminUserItem[] }>(
    cookies,
    '/api/v1/admin/users?limit=100',
    requestId
  );
  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    if (result.status === 403) {
      return { state: 'forbidden', items: null, error: result.message };
    }
    if (result.status === 501) {
      return { state: 'not_implemented', items: null, error: result.message };
    }
    return { state: 'error', items: null, error: result.message };
  }
  return { state: 'ok', items: result.data.items, error: null };
};

export const actions: Actions = {
  update: async ({ request, cookies }) => {
    const form = await request.formData();
    const id = String(form.get('id') ?? '').trim();
    const status = String(form.get('status') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim();
    const version = Number(form.get('version') ?? 0);
    if (!reason) return fail(422, { message: '操作原因必填（写审计）' });
    if (!['pending', 'active', 'restricted', 'banned'].includes(status)) {
      return fail(422, { message: '无效状态' });
    }
    try {
      const result = await authedPatch<AdminUserItem>(
        cookies,
        `/api/v1/admin/users/${encodeURIComponent(id)}`,
        { status, reason },
        { 'If-Match': String(version) },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { message: `用户 ${result.data.username} 状态已更新为 ${result.data.status}` };
      }
      if (result.status === 409) {
        return fail(409, { conflict: true, message: `版本冲突：${result.message}（请刷新后重试）` });
      }
      return fail(result.status, { message: result.message, requestId: result.requestId });
    } catch {
      return fail(503, { message: '保存失败，请稍后重试' });
    }
  }
};
