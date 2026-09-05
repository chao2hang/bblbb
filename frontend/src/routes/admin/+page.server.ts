// GAP-FIX（M17-GAPFIX-07·视觉对齐）：/admin 仪表盘——对齐原型 admin.html：
// 周期 Tab（今日/本周/本月/今年）+ 统计四卡 + 运营趋势图（趋势组件） +
// 关键入口 + 最近管理员操作 + 生产健康提示。
// 数据源：GET /api/v1/admin/stats（统计）与 GET /api/v1/admin/stats/trend
// （8 桶时间序列；period 来自 URL，Tab 为链接，无 JS 可切换）。
import { redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { getAuthed } from '$lib/api/server';
import type { AdminStats, AdminStatsTrend } from '$lib/api/types';

export type AdminDashboardState = 'ok' | 'forbidden' | 'not_implemented' | 'error';

export interface AdminDashboardPageData {
  state: AdminDashboardState;
  stats: AdminStats | null;
  trend: AdminStatsTrend | null;
  period: 'day' | 'week' | 'month' | 'year';
  error: string | null;
}

const PERIODS = new Set(['day', 'week', 'month', 'year']);

export const load: PageServerLoad = async ({ cookies, request, url }): Promise<AdminDashboardPageData> => {
  const requestId = request.headers.get('x-request-id');
  const periodParam = url.searchParams.get('period') ?? 'day';
  const period = (PERIODS.has(periodParam) ? periodParam : 'day') as AdminDashboardPageData['period'];

  const result = await getAuthed<AdminStats>(cookies, '/api/v1/admin/stats', requestId);
  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    if (result.status === 403) {
      return { state: 'forbidden', stats: null, trend: null, period, error: result.message };
    }
    if (result.status === 501) {
      return { state: 'not_implemented', stats: null, trend: null, period, error: result.message };
    }
    return { state: 'error', stats: null, trend: null, period, error: result.message };
  }

  // 趋势 best-effort：失败不阻塞统计卡（图卡显示空态）。
  const trendResult = await getAuthed<AdminStatsTrend>(
    cookies,
    `/api/v1/admin/stats/trend?period=${period}`,
    requestId
  );
  const trend = trendResult.ok ? trendResult.data : null;

  return { state: 'ok', stats: result.data, trend, period, error: null };
};
