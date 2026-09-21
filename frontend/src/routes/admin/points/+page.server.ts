// M13-UI-04 + GAP-FIX（视觉对齐 M17-GAPFIX-06）：积分与货币——
// 1) 全站流水（GET /admin/points/ledger：username/asset/kind/from/to 过滤 +
//    after 游标分页，points.adjust 权限，403 独立降级）；
// 2) 积分/活跃配置只读卡（activity.manage，后端账本为唯一裁决）。
import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPost, getAuthed } from '$lib/api/server';
import { newClientRequestId } from '$lib/api/client';

export interface PointsConfigView {
  site_timezone?: string;
  check_in?: Record<string, unknown>;
  [key: string]: unknown;
}

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

export type AdminPointsLoadState = 'ok' | 'forbidden' | 'not_implemented' | 'error';

export interface AdminPointsPageData {
  state: AdminPointsLoadState;
  config: PointsConfigView | null;
  error: string | null;
  ledger: PointsLedgerState;
}

export interface AdminPointsActionData {
  message?: string;
  requestId?: string | null;
  stepUpRequired?: boolean;
}

export const load: PageServerLoad = async ({ cookies, request, url }): Promise<AdminPointsPageData> => {
  const requestId = request.headers.get('x-request-id');

  // ── 全站流水（过滤条件来自 URL，GET 表单可直接驱动，无 JS 可用）──
  const rawAsset = url.searchParams.get('asset')?.trim() ?? '';
  // 流水筛选也只暴露 B币；兼容旧链接中的 b_coin 别名。
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
      items: ledgerResult.data.items,
      nextCursor: ledgerResult.data.next_cursor || null,
      message: null,
      filters
    };
  } else if (ledgerResult.status === 403) {
    ledger = { state: 'forbidden', items: [], nextCursor: null, message: ledgerResult.message, filters };
  } else {
    ledger = { state: 'error', items: [], nextCursor: null, message: ledgerResult.message, filters };
  }

  // ── 积分/活跃配置（只读）──
  const result = await getAuthed<PointsConfigView>(cookies, '/api/v1/admin/activity/config', requestId);
  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    if (result.status === 403) return { state: 'forbidden', config: null, error: result.message, ledger };
    if (result.status === 501) return { state: 'not_implemented', config: null, error: result.message, ledger };
    return { state: 'error', config: null, error: result.message, ledger };
  }
  return { state: 'ok', config: result.data, error: null, ledger };
};

export const actions: Actions = {
  // M18-ADMIN-POINTS-01：积分调整（points.adjust，复用后端 POST /admin/points/adjust）
  adjust: async ({ request, cookies }) => {
    const form = await request.formData();
    const username = String(form.get('username') ?? '').trim();
    const currency = String(form.get('currency') ?? 'coin').trim().toLowerCase();
    const amount = Number(form.get('amount') ?? 0);
    const reason = String(form.get('reason') ?? '').trim();
    if (currency !== 'coin') {
      return fail(422, { message: '调账币种仅支持 B币' });
    }
    if (!username || !amount || !reason) {
      return fail(422, { message: '用户名、非 0 调整数额与调整原因均为必填' });
    }
    const client_request_id = newClientRequestId();
    try {
      const result = await authedPost(
        cookies,
        '/api/v1/admin/points/adjust',
        { username, currency, amount, reason, client_request_id },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { message: `已成功为 ${username} 调整 ${amount > 0 ? '+' : ''}${amount} B币` } satisfies AdminPointsActionData;
      }
      if (result.code === 'step_up_required') {
        return fail(403, {
          message: '此操作需要重新验证身份，请输入密码重新验证后重试',
          stepUpRequired: true
        } satisfies AdminPointsActionData);
      }
      return fail(result.status, { message: result.message, requestId: result.requestId } satisfies AdminPointsActionData);
    } catch {
      return fail(503, { message: '调整积分服务暂不可用，请稍后重试' } satisfies AdminPointsActionData);
    }
  },

  /** 重新验证身份（step-up 窗口过期后；与 storage/roles 页同款交互）。 */
  reauth: async ({ request, cookies }) => {
    const form = await request.formData();
    const password = String(form.get('password') ?? '');
    if (!password) {
      return fail(422, {
        message: '请输入当前密码'
      } satisfies AdminPointsActionData);
    }
    try {
      const result = await authedPost(
        cookies,
        '/api/v1/auth/re-auth',
        { password },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return {
          message: '已重新验证身份，请重试刚才的操作'
        } satisfies AdminPointsActionData;
      }
      return fail(result.status, {
        message: result.message
      } satisfies AdminPointsActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, {
        message: '验证失败，请稍后重试'
      } satisfies AdminPointsActionData);
    }
  }
};
