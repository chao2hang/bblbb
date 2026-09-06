// 下载账单页（需登录，GAP-FIX-SPEC 四节：/me/billing）。
//
// - load：GET /api/v1/activity/summary（coin 余额，统计卡）+
//   GET /api/v1/me/download-transactions（下载流水）。401 → /login。
// - 「重新下载」action（sign）：行携带授权引用时调
//   POST /api/v1/download-authorizations/{id}/sign-url（client.ts
//   signDownloadUrl 的服务端等价实现——action 在服务端运行，须走
//   server.ts authedPost 转发会话 Cookie + CSRF + Idempotency-Key）。
//   行只有 attachment 引用时退回 POST /api/v1/attachments/{id}/download
//   （后端会复用有效授权、不重复扣费）。
//
// 流水形状按后端实际实现双形状归一化（grep backend/src/routes/download.rs
// get_me_download_transactions）：
//   - 实际返回 {transactions: [{id, operation_id, currency_id,
//     delta_balance, balance_after, created_at}]}——**不含 attachment/
//     authorization 引用**；
//   - 契约/客户端投影 {items: DownloadTransaction[]} 或数组（含
//     attachment_id/attachment_name/charged）。
// client.ts 的 listDownloadTransactions 只归一化 items/数组形状（本批不
// 改 client.ts），因此 load 在此处直连并同时容忍两种形状；行没有授权引用
// 时「重新下载」给出可读错误（不 500）。
import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPost, getAuthed } from '$lib/api/server';
import type { ActivitySummary } from '$lib/api/types';

/** 表格行视图（两种后端形状归一化后的投影）。 */
export interface BillingRow {
  id: string;
  /** Unix 毫秒。 */
  created_at: number;
  /** 币种（实际后端 currency_id；契约 charged.currency）。 */
  currency: string;
  /** 支出金额（正数展示）。 */
  amount: number;
  /** 交易后余额（实际后端独有；契约形状无此字段）。 */
  balance_after: number | null;
  attachment_id: string | null;
  attachment_name: string | null;
  /** 下载授权引用（sign-url 重签用；当前两种形状均不返回，保留字段等后端补齐）。 */
  authorization_id: string | null;
}

export interface BillingPageData {
  summary: ActivitySummary | null;
  rows: BillingRow[];
  totals: { count: number; spentCoin: number; lastAt: number | null };
  error: string | null;
}

export interface BillingActionData {
  ok?: boolean;
  message?: string;
  /** 签名成功时的一次性下载链接（短时效，由后端签发）。 */
  url?: string | null;
  expiresAt?: string | null;
  requestId?: string | null;
}

/** 下载结果（DownloadResult 投影：client.ts signDownloadUrl 同构）。 */
interface DownloadResultView {
  download_url?: string;
  url_expires_at?: string;
}

/** /me/download-transactions 响应（实际 + 契约形状并集）。 */
interface TransactionsResponse {
  transactions?: Array<Record<string, unknown>>;
  items?: Array<Record<string, unknown>>;
}

/** 归一化两种形状的流水行。 */
function normalizeRows(data: TransactionsResponse): BillingRow[] {
  const raw = data.transactions ?? data.items ?? [];
  return raw.map((r) => {
    const row = r as Record<string, unknown>;
    const charged = (row.charged ?? null) as { currency?: string; amount?: number } | null;
    const currency =
      typeof row.currency_id === 'string' ? row.currency_id : (charged?.currency ?? 'coin');
    const amount =
      typeof row.delta_balance === 'number'
        ? Math.abs(row.delta_balance)
        : (charged?.amount ?? 0);
    return {
      id: String(row.id ?? ''),
      created_at: typeof row.created_at === 'number' ? row.created_at : 0,
      currency,
      amount,
      balance_after: typeof row.balance_after === 'number' ? row.balance_after : null,
      attachment_id: typeof row.attachment_id === 'string' ? row.attachment_id : null,
      attachment_name:
        typeof row.attachment_name === 'string' ? row.attachment_name : null,
      authorization_id: typeof row.authorization_id === 'string' ? row.authorization_id : null
    } satisfies BillingRow;
  });
}

export const load: PageServerLoad = async ({ cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const summaryResult = await getAuthed<ActivitySummary>(
    cookies,
    '/api/v1/activity/summary',
    requestId
  );
  if (!summaryResult.ok && summaryResult.status === 401) throw redirect(303, '/login');

  const txResult = await getAuthed<TransactionsResponse>(
    cookies,
    '/api/v1/me/download-transactions',
    requestId
  );
  if (!txResult.ok && txResult.status === 401) throw redirect(303, '/login');

  const rows = txResult.ok ? normalizeRows(txResult.data) : [];
  const coinRows = rows.filter((r) => r.currency === 'coin');
  const totals = {
    count: rows.length,
    spentCoin: coinRows.reduce((acc, r) => acc + r.amount, 0),
    lastAt: rows.length ? Math.max(...rows.map((r) => r.created_at)) : null
  };

  return {
    summary: summaryResult.ok ? summaryResult.data : null,
    rows,
    totals,
    error: txResult.ok ? null : txResult.message
  } satisfies BillingPageData;
};

export const actions: Actions = {
  /** 每行「重新下载」：优先按授权重签（不重复扣费），无授权引用时按
   *  附件重新走下载端点（复用有效授权）。两者都缺 → 422 可读错误。 */
  sign: async ({ cookies, request }) => {
    const form = await request.formData();
    const authorizationId = String(form.get('authorization_id') ?? '').trim();
    const attachmentId = String(form.get('attachment_id') ?? '').trim();
    const clientRequestId = String(form.get('client_request_id') ?? '').trim();
    const requestId = request.headers.get('x-request-id');

    if (clientRequestId.length < 16) {
      return fail(422, {
        message: '请求标识缺失，请刷新页面后重试'
      } satisfies BillingActionData);
    }

    let path: string;
    if (authorizationId) {
      // POST /download-authorizations/{id}/sign-url（client.ts signDownloadUrl）。
      path = `/api/v1/download-authorizations/${encodeURIComponent(authorizationId)}/sign-url`;
    } else if (attachmentId) {
      // 退回下载端点：复用有效授权，不重复扣费（M06-UI-04）。
      path = `/api/v1/attachments/${encodeURIComponent(attachmentId)}/download`;
    } else {
      return fail(422, {
        message: '这条流水没有可用的下载授权引用，暂时无法重新下载'
      } satisfies BillingActionData);
    }

    const result = await authedPost<DownloadResultView>(
      cookies,
      path,
      { client_request_id: clientRequestId },
      requestId,
      { 'Idempotency-Key': clientRequestId }
    );
    if (!result.ok) {
      return fail(result.status, {
        message: result.message,
        requestId: result.requestId ?? null
      } satisfies BillingActionData);
    }

    const data = result.data ?? {};
    return {
      ok: true,
      message: '签名成功，请在有效期内完成下载',
      url: typeof data.download_url === 'string' ? data.download_url : null,
      expiresAt: typeof data.url_expires_at === 'string' ? data.url_expires_at : null
    } satisfies BillingActionData;
  }
};
