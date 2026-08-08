// M13-UI-04：OIDC 管理页——OAuth Client 列表（M11-CONSENT 已实现；secret
// 只显示一次、绝不回显 hash/明文）。
import { redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { getAuthed } from '$lib/api/server';

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
