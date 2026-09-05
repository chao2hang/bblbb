// GAP-FIX（管理域·BI 看板）：/admin/bi 新建页。
// - load：GET /api/v1/admin/bi/metrics?period=day|week|month|year（?period=
//   URL 参数切换周期；admin.manage 权限门）；
// - 只读页（无 action）：4 项指标卡（value/target/进度条/delta_pct 涨跌色）
//   + generated_at；401 → /login、403 → forbidden 态。
import { redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { getAuthed } from '$lib/api/server';
import type { AdminBiMetrics } from '$lib/api/types';

/** 周期白名单（day/week/month/year）。 */
const BI_PERIODS = ['day', 'week', 'month', 'year'] as const;

export type BiPeriod = (typeof BI_PERIODS)[number];

export type AdminBiState = 'ok' | 'forbidden' | 'not_implemented' | 'error';

export interface AdminBiPageData {
  state: AdminBiState;
  period: BiPeriod;
  metrics: AdminBiMetrics | null;
  error: string | null;
}

export const load: PageServerLoad = async ({ cookies, request, url }): Promise<AdminBiPageData> => {
  const requestId = request.headers.get('x-request-id');
  const raw = url.searchParams.get('period') ?? 'day';
  const period: BiPeriod = (BI_PERIODS as readonly string[]).includes(raw)
    ? (raw as BiPeriod)
    : 'day';

  const result = await getAuthed<AdminBiMetrics>(
    cookies,
    `/api/v1/admin/bi/metrics?period=${encodeURIComponent(period)}`,
    requestId
  );
  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    if (result.status === 403) {
      return { state: 'forbidden', period, metrics: null, error: result.message };
    }
    if (result.status === 501) {
      return { state: 'not_implemented', period, metrics: null, error: result.message };
    }
    return { state: 'error', period, metrics: null, error: result.message };
  }
  return { state: 'ok', period, metrics: result.data, error: null };
};
