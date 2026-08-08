// M13-UI-04：下载计费配置页——站点级 download-billing 策略（脱敏；Secret/
// 签名 URL 永不回显）。
import { redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { getAuthed } from '$lib/api/server';

export interface DownloadBillingConfigView {
  configured?: boolean;
  mode?: string;
  amount?: number;
  authorization_ttl_seconds?: number;
  daily_user_limit?: number | null;
  grace_on_disable?: boolean;
  version?: number;
  is_enabled?: boolean;
}

export type AdminDownloadBillingState = 'ok' | 'forbidden' | 'not_implemented' | 'error';

export interface AdminDownloadBillingPageData {
  state: AdminDownloadBillingState;
  config: DownloadBillingConfigView | null;
  error: string | null;
}

export const load: PageServerLoad = async ({ cookies, request }): Promise<AdminDownloadBillingPageData> => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<DownloadBillingConfigView>(
    cookies,
    '/api/v1/admin/download-billing/config',
    requestId
  );
  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    if (result.status === 403) {
      return { state: 'forbidden', config: null, error: result.message };
    }
    if (result.status === 501) {
      return { state: 'not_implemented', config: null, error: result.message };
    }
    return { state: 'error', config: null, error: result.message };
  }
  return { state: 'ok', config: result.data, error: null };
};
