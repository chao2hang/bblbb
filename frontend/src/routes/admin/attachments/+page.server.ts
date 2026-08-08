// M13-UI-04：附件管理页——存储配置脱敏视图 + 下载策略入口（M06 已实现）。
import { redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { getAuthed } from '$lib/api/server';

export interface AttachmentAdminView {
  backend?: string;
  configured?: boolean;
  path_style?: boolean;
  region?: string | null;
  endpoint?: string | null;
  bucket?: string | null;
  signed_url_ttl_seconds?: number;
  credentials?: { access_key_id_configured?: boolean; secret_configured?: boolean };
}

export type AdminAttachmentsState = 'ok' | 'forbidden' | 'not_implemented' | 'error';

export interface AdminAttachmentsPageData {
  state: AdminAttachmentsState;
  config: AttachmentAdminView | null;
  error: string | null;
}

export const load: PageServerLoad = async ({ cookies, request }): Promise<AdminAttachmentsPageData> => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<AttachmentAdminView>(
    cookies,
    '/api/v1/admin/storage/config',
    requestId
  );
  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    if (result.status === 403) return { state: 'forbidden', config: null, error: result.message };
    if (result.status === 501) return { state: 'not_implemented', config: null, error: result.message };
    return { state: 'error', config: null, error: result.message };
  }
  return { state: 'ok', config: result.data, error: null };
};
