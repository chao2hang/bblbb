// M13-UI-04 + GAP-FIX（视觉对齐 M17-GAPFIX-06）：下载计费——
// 1) 计费策略可编辑表单（PATCH /admin/download-billing/config，PolicyBody）；
// 2) 下载交易列表（GET /admin/download-billing/transactions；数据模型=
//    download_authorizations 扣费记录，非队列——原型「取消/重试」无对应语义）。
import { fail, isRedirect, redirect, type Cookies } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPatch, getAuthed } from '$lib/api/server';

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

export interface DownloadBillingTransaction {
  id: string;
  username: string;
  filename: string;
  amount: number;
  created_at: number;
}

export type AdminDownloadBillingState = 'ok' | 'forbidden' | 'not_implemented' | 'error';

export interface AdminDownloadBillingPageData {
  state: AdminDownloadBillingState;
  config: DownloadBillingConfigView | null;
  error: string | null;
  transactions: {
    state: 'ok' | 'forbidden' | 'error';
    items: DownloadBillingTransaction[];
    nextCursor: string | null;
    message: string | null;
  };
}

/** save action 返回投影（SvelteKit Actions 联合类型）。 */
export interface AdminDownloadBillingActionData {
  config?: DownloadBillingConfigView | null;
  transactions?: AdminDownloadBillingPageData['transactions'];
  message?: string;
  requestId?: string | null;
}

async function reloadTransactions(cookies: Cookies, requestId: string | null): Promise<AdminDownloadBillingPageData['transactions']> {
  const result = await getAuthed<{ items: DownloadBillingTransaction[]; next_cursor: string | null }>(
    cookies,
    '/api/v1/admin/download-billing/transactions?limit=20',
    requestId
  );
  if (result.ok) {
    return { state: 'ok', items: result.data.items, nextCursor: result.data.next_cursor || null, message: null };
  }
  if (result.status === 403) return { state: 'forbidden', items: [], nextCursor: null, message: result.message };
  return { state: 'error', items: [], nextCursor: null, message: result.message };
}

export const load: PageServerLoad = async ({ cookies, request }): Promise<AdminDownloadBillingPageData> => {
  const requestId = request.headers.get('x-request-id');
  const transactions = await reloadTransactions(cookies, requestId);
  const result = await getAuthed<DownloadBillingConfigView>(
    cookies,
    '/api/v1/admin/download-billing/config',
    requestId
  );
  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    if (result.status === 403) {
      return { state: 'forbidden', config: null, error: result.message, transactions };
    }
    if (result.status === 501) {
      return { state: 'not_implemented', config: null, error: result.message, transactions };
    }
    return { state: 'error', config: null, error: result.message, transactions };
  }
  return { state: 'ok', config: result.data, error: null, transactions };
};

export const actions: Actions = {
  /** 保存计费策略（PATCH /admin/download-billing/config；reason 写审计）。 */
  save: async ({ request, cookies }) => {
    const form = await request.formData();
    const reason = String(form.get('reason') ?? '').trim();
    if (!reason) {
      return fail(422, { message: '操作原因必填（写入审计日志）' } satisfies AdminDownloadBillingActionData);
    }
    const body: Record<string, unknown> = { reason };
    body.is_enabled = form.get('is_enabled') === 'on';
    const mode = String(form.get('mode') ?? '').trim();
    if (mode) body.mode = mode;
    const amountRaw = String(form.get('amount') ?? '').trim();
    if (amountRaw !== '') {
      const amount = Number(amountRaw);
      if (!Number.isInteger(amount) || amount < 0) {
        return fail(422, { message: '默认下载价格需为非负整数' } satisfies AdminDownloadBillingActionData);
      }
      body.amount = amount;
      body.currency_id = 'coin';
    }
    const ttl = String(form.get('authorization_ttl_seconds') ?? '').trim();
    if (ttl) body.authorization_ttl_seconds = Number(ttl);
    const dailyLimitRaw = String(form.get('daily_user_limit') ?? '').trim();
    if (dailyLimitRaw !== '') {
      const dailyLimit = Number(dailyLimitRaw);
      if (!Number.isInteger(dailyLimit) || dailyLimit < 0) {
        return fail(422, { message: '单用户日限额需为非负整数' } satisfies AdminDownloadBillingActionData);
      }
      body.daily_user_limit = dailyLimit;
    }

    try {
      const result = await authedPatch<unknown>(
        cookies,
        '/api/v1/admin/download-billing/config',
        body,
        {},
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        const configResult = await getAuthed<DownloadBillingConfigView>(
          cookies,
          '/api/v1/admin/download-billing/config',
          request.headers.get('x-request-id')
        );
        return {
          config: configResult.ok ? configResult.data : null,
          message: '计费策略已保存'
        } satisfies AdminDownloadBillingActionData;
      }
      return fail(result.status, { message: result.message, requestId: result.requestId } satisfies AdminDownloadBillingActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '保存失败，请稍后重试' } satisfies AdminDownloadBillingActionData);
    }
  }
};
