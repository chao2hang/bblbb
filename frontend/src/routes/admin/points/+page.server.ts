// M13-UI-04：积分/活跃配置页——签到与奖励开关（activity.manage；禁止直接改
// 余额或历史流水，后端账本为唯一裁决）。
import { redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { getAuthed } from '$lib/api/server';

export interface PointsConfigView {
  site_timezone?: string;
  check_in?: Record<string, unknown>;
  [key: string]: unknown;
}

export type AdminPointsLoadState = 'ok' | 'forbidden' | 'not_implemented' | 'error';

export interface AdminPointsPageData {
  state: AdminPointsLoadState;
  config: PointsConfigView | null;
  error: string | null;
}

export const load: PageServerLoad = async ({ cookies, request }): Promise<AdminPointsPageData> => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<PointsConfigView>(cookies, '/api/v1/admin/activity/config', requestId);
  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    if (result.status === 403) return { state: 'forbidden', config: null, error: result.message };
    if (result.status === 501) return { state: 'not_implemented', config: null, error: result.message };
    return { state: 'error', config: null, error: result.message };
  }
  return { state: 'ok', config: result.data, error: null };
};
