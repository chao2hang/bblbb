// M13-UI-04：等级与附件配额页——读取等级 1 的附件配额策略（M06-QUOTA 已实现；
// 脱敏投影）。
import { redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { getAuthed } from '$lib/api/server';

export interface LevelQuotaView {
  level: number;
  policy: {
    level: number;
    single_file_max_bytes: number;
    total_bytes: number;
    daily_upload_bytes: number;
    retention_days: number;
    policy_version: number;
  } | null;
}

export type AdminLevelsState = 'ok' | 'forbidden' | 'not_implemented' | 'error';

export interface AdminLevelsPageData {
  state: AdminLevelsState;
  quota: LevelQuotaView | null;
  error: string | null;
}

export const load: PageServerLoad = async ({ cookies, request }): Promise<AdminLevelsPageData> => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<LevelQuotaView>(
    cookies,
    '/api/v1/admin/levels/1/attachment-quota',
    requestId
  );
  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    if (result.status === 403) return { state: 'forbidden', quota: null, error: result.message };
    if (result.status === 501) return { state: 'not_implemented', quota: null, error: result.message };
    return { state: 'error', quota: null, error: result.message };
  }
  return { state: 'ok', quota: result.data, error: null };
};
