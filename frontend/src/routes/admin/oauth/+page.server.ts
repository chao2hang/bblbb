// M13-UI-04：OIDC 管理页——OAuth Client 列表（M11-CONSENT 已实现；secret
// 只显示一次、绝不回显 hash/明文）。
import { redirect, type Cookies } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { fail, type Actions } from '@sveltejs/kit';
import { authedPatch, getAuthed } from '$lib/api/server';

export interface AdminOAuthClientItem {
  id: string;
  name: string;
  client_type: string;
  client_id: string;
  status: string;
  version: number;
  [key: string]: unknown;
}

export type AdminOAuthState = 'ok' | 'forbidden' | 'not_implemented' | 'error';

export interface AdminOAuthPageData {
  state: AdminOAuthState;
  clients: AdminOAuthClientItem[] | null;
  error: string | null;
}

export interface AdminOAuthActionData {
  clients: AdminOAuthClientItem[] | null;
  message?: string | null;
}

async function reloadClients(cookies: Cookies, requestId: string | null): Promise<AdminOAuthClientItem[] | null> {
  const result = await getAuthed<{ clients: AdminOAuthClientItem[] }>(cookies, '/api/v1/admin/oauth-clients', requestId);
  if (!result.ok && result.status === 401) throw redirect(303, '/login');
  return result.ok ? result.data.clients : null;
}

export const actions: Actions = {
  /** 禁用/启用（M17-GAPFIX-07）：PATCH status（部分更新，其余字段保持）。 */
  toggle: async ({ request, cookies }) => {
    const form = await request.formData();
    const id = String(form.get('id') ?? '').trim();
    const status = String(form.get('status') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim();
    if (!id || !status) return fail(422, { clients: null, message: '缺少参数' } satisfies AdminOAuthActionData);
    if (!reason) return fail(422, { clients: null, message: '操作原因必填（写审计）' } satisfies AdminOAuthActionData);
    try {
      const result = await authedPatch<unknown>(
        cookies,
        `/api/v1/admin/oauth-clients/${encodeURIComponent(id)}`,
        { status, reason },
        {},
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { clients: await reloadClients(cookies, request.headers.get('x-request-id')), message: `客户端已${status === 'active' ? '启用' : '禁用'}` } satisfies AdminOAuthActionData;
      }
      return fail(result.status, { clients: await reloadClients(cookies, request.headers.get('x-request-id')), message: result.message } satisfies AdminOAuthActionData);
    } catch {
      return fail(503, { clients: null, message: '操作失败，请稍后重试' } satisfies AdminOAuthActionData);
    }
  }
};

export const load: PageServerLoad = async ({ cookies, request }): Promise<AdminOAuthPageData> => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<{ clients: AdminOAuthClientItem[] }>(
    cookies,
    '/api/v1/admin/oauth-clients',
    requestId
  );
  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    if (result.status === 403) return { state: 'forbidden', clients: null, error: result.message };
    if (result.status === 501) return { state: 'not_implemented', clients: null, error: result.message };
    return { state: 'error', clients: null, error: result.message };
  }
  return { state: 'ok', clients: result.data.clients, error: null };
};
