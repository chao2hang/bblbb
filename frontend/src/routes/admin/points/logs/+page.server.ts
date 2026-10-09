// 积分日志（全站积分流水明细与多维度筛选）
import { redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { getAuthed } from '$lib/api/server';

/** 流水行（GET /admin/points/ledger 投影）。 */
export interface PointsLedgerItem {
  id: string;
  username: string;
  kind: string;
  currency: string;
  amount: number;
  balance_after: number | null;
  source_type: string | null;
  memo: string | null;
  created_at: number;
}

export interface PointsLedgerState {
  state: 'ok' | 'forbidden' | 'error';
  items: PointsLedgerItem[];
  nextCursor: string | null;
  message: string | null;
  /** 当前过滤条件（回填表单 + 构造分页链接）。 */
  filters: { username: string; asset: string; kind: string; from: string; to: string };
}

export interface AdminPointsLogsPageData {
  ledger: PointsLedgerState;
}

export const load: PageServerLoad = async ({ cookies, request, url }): Promise<AdminPointsLogsPageData> => {
  const requestId = request.headers.get('x-request-id');

  // ── 全站流水（过滤条件来自 URL，GET 表单驱动，无 JS 可用）──
  const rawAsset = url.searchParams.get('asset')?.trim() ?? '';
  // 流水筛选只暴露站点消费货币；兼容旧链接中的 b_coin 别名。
  const asset = rawAsset === 'coin' || rawAsset === 'b_coin' ? 'coin' : '';
  const filters = {
    username: url.searchParams.get('username')?.trim() ?? '',
    asset,
    kind: url.searchParams.get('kind')?.trim() ?? '',
    from: url.searchParams.get('from')?.trim() ?? '',
    to: url.searchParams.get('to')?.trim() ?? ''
  };
  const params = new URLSearchParams();
  params.set('limit', '20');
  const after = url.searchParams.get('after')?.trim() ?? '';
  if (after) params.set('after', after);
  if (filters.username) params.set('username', filters.username);
  if (filters.asset) params.set('asset', filters.asset);
  if (filters.kind) params.set('kind', filters.kind);
  // 日期输入（yyyy-MM-dd）→ Unix 毫秒（含端点）。
  const fromMs = Date.parse(filters.from);
  if (filters.from && !Number.isNaN(fromMs)) params.set('from', String(fromMs));
  const toMs = filters.to ? Date.parse(`${filters.to}T23:59:59.999`) : Number.NaN;
  if (filters.to && !Number.isNaN(toMs)) params.set('to', String(toMs));

  let ledger: PointsLedgerState;
  const ledgerResult = await getAuthed<{ items: PointsLedgerItem[]; next_cursor: string | null }>(
    cookies,
    `/api/v1/admin/points/ledger?${params.toString()}`,
    requestId
  );
  if (ledgerResult.ok) {
    ledger = {
      state: 'ok',
      items: ledgerResult.data.items ?? [],
      nextCursor: ledgerResult.data.next_cursor || null,
      message: null,
      filters
    };
  } else if (ledgerResult.status === 401) {
    throw redirect(303, '/login');
  } else if (ledgerResult.status === 403) {
    ledger = { state: 'forbidden', items: [], nextCursor: null, message: ledgerResult.message, filters };
  } else {
    ledger = { state: 'error', items: [], nextCursor: null, message: ledgerResult.message, filters };
  }

  return { ledger };
};
