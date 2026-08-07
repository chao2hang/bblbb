// M12-UI-05/06：管理员 Marketplace 控制台。
//
// - load：Clients（含 scopes/balance）、Offers、Webhook 投递记录；
// - upsertClient：注册/更新 Client + 逐 scope 审批 + 限额 + status
//   （If-Match + reason + 审计）；高风险动作展示审计依据；
// - rotateWebhook / emergencyDisable / runReconciliation / retryRefund：
//   全部强制 reason + recent-auth（后端）+ 审计。
import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPost, authedPatch, getAuthed } from '$lib/api/server';
import { adminListState, type AdminLoadState } from '$lib/admin';
import type {
  MarketplaceBalanceView,
  MarketplaceClientView,
  MarketplaceDeliveryView,
  MarketplaceOfferView,
  MarketplaceRefundView
} from '$lib/api/types';

export interface AdminMarketplacePageData {
  clients: AdminLoadState<MarketplaceClientView>;
  offers: AdminLoadState<MarketplaceOfferView>;
  deliveries: AdminLoadState<MarketplaceDeliveryView>;
  balances: MarketplaceBalanceView[];
}

export interface AdminMarketplaceActionData {
  message?: string;
  requestId?: string | null;
  code?: string | null;
  secret?: string | null;
}

export const load: PageServerLoad = async ({ cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const clientsResult = await getAuthed<{ clients: MarketplaceClientView[] }>(
    cookies,
    '/api/v1/admin/marketplace/clients?limit=100',
    requestId
  );
  if (!clientsResult.ok && clientsResult.status === 401) throw redirect(303, '/login');
  const clients =
    clientsResult.ok === true
      ? ({ ok: true, data: { items: clientsResult.data.clients } } as const)
      : clientsResult;
  const clientsState = adminListState<MarketplaceClientView>(clients);

  const offersResult = await getAuthed<{ offers: MarketplaceOfferView[] }>(
    cookies,
    '/api/v1/admin/marketplace/offers',
    requestId
  );
  const offers =
    offersResult.ok === true
      ? ({ ok: true, data: { items: offersResult.data.offers } } as const)
      : offersResult;
  const offersState = adminListState<MarketplaceOfferView>(offers);

  const deliveriesResult = await getAuthed<{ deliveries: MarketplaceDeliveryView[] }>(
    cookies,
    '/api/v1/admin/marketplace/webhook-deliveries?limit=50',
    requestId
  );
  const deliveries =
    deliveriesResult.ok === true
      ? ({ ok: true, data: { items: deliveriesResult.data.deliveries } } as const)
      : deliveriesResult;
  const deliveriesState = adminListState<MarketplaceDeliveryView>(deliveries);

  const balances: MarketplaceBalanceView[] =
    clientsState.state === 'ok' ? clientsState.items.flatMap((c) => (c.balance ? [c.balance] : [])) : [];

  return { clients: clientsState, offers: offersState, deliveries: deliveriesState, balances } satisfies AdminMarketplacePageData;
};

const CLIENT_FIELDS = [
  'name',
  'owner_user_id',
  'terms_url',
  'privacy_url',
  'webhook_url',
  'redirect_uris',
  'fee_bps',
  'status'
] as const;

function clientBody(form: FormData): Record<string, unknown> {
  const body: Record<string, unknown> = { reason: String(form.get('reason') ?? '').trim() };
  for (const field of CLIENT_FIELDS) {
    const raw = form.get(field);
    if (raw === null) continue;
    const value = String(raw).trim();
    if (value === '') continue;
    if (field === 'redirect_uris') {
      body[field] = value
        .split(/[\n,]/)
        .map((s) => s.trim())
        .filter(Boolean);
    } else if (field === 'fee_bps') {
      body[field] = Number(value);
    } else {
      body[field] = value;
    }
  }
  // 逐 scope 审批（状态 + 限额）。
  const scopeEntries: Array<Record<string, unknown>> = [];
  const scopes = ['marketplace.checkout.create', 'marketplace.purchase', 'marketplace.offer.write', 'marketplace.purchases.read', 'marketplace.refund', 'marketplace.webhook.manage'];
  for (const scope of scopes) {
    const status = String(form.get(`scope_${scope}`) ?? '');
    if (status === 'approved' || status === 'disabled') {
      const perTx = Number(form.get(`limit_${scope}_per_tx`) ?? 0);
      const daily = Number(form.get(`limit_${scope}_daily`) ?? 0);
      const limits: Record<string, number> = {};
      if (perTx > 0) limits.max_amount_per_transaction = perTx;
      if (daily > 0) limits.max_amount_daily = daily;
      scopeEntries.push({ scope, status, limits });
    }
  }
  if (scopeEntries.length > 0) body.scopes = scopeEntries;
  return body;
}

export const actions: Actions = {
  upsertClient: async ({ request, cookies }) => {
    const form = await request.formData();
    const key = String(form.get('client_id') ?? '').trim();
    const version = Number(form.get('version') ?? 1);
    const reason = String(form.get('reason') ?? '').trim();
    if (!key) return fail(422, { message: '缺少 Client 标识' } satisfies AdminMarketplaceActionData);
    if (!reason) return fail(422, { message: '操作原因必填' } satisfies AdminMarketplaceActionData);
    try {
      const result = await authedPatch<MarketplaceClientView>(
        cookies,
        `/api/v1/admin/marketplace/clients/${encodeURIComponent(key)}`,
        clientBody(form),
        { 'If-Match': String(version) },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { message: `Client「${result.data.name}」已更新（version ${result.data.version}）` } satisfies AdminMarketplaceActionData;
      }
      if (result.status === 409) {
        return fail(409, { message: `版本冲突：${result.message}，请刷新后重试` } satisfies AdminMarketplaceActionData);
      }
      return fail(result.status, { message: result.message, code: result.code, requestId: result.requestId } satisfies AdminMarketplaceActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '更新失败，请稍后重试' } satisfies AdminMarketplaceActionData);
    }
  },
  rotateWebhook: async ({ request, cookies }) => {
    const form = await request.formData();
    const clientId = String(form.get('client_id') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim();
    if (!clientId) return fail(422, { message: '缺少 Client 标识' } satisfies AdminMarketplaceActionData);
    if (!reason) return fail(422, { message: '操作原因必填' } satisfies AdminMarketplaceActionData);
    try {
      const result = await authedPost<{ webhook_secret: string }>(
        cookies,
        `/api/v1/admin/marketplace/clients/${encodeURIComponent(clientId)}/rotate-webhook-secret`,
        { reason },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return {
          message: 'Webhook Secret 已轮换',
          secret: result.data.webhook_secret
        } satisfies AdminMarketplaceActionData;
      }
      return fail(result.status, { message: result.message } satisfies AdminMarketplaceActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '轮换失败，请稍后重试' } satisfies AdminMarketplaceActionData);
    }
  },
  emergencyDisable: async ({ request, cookies }) => {
    const form = await request.formData();
    const clientId = String(form.get('client_id') ?? '').trim();
    const version = Number(form.get('version') ?? 0);
    const reason = String(form.get('reason') ?? '').trim();
    if (!clientId || !Number.isInteger(version) || version < 1) {
      return fail(422, { message: 'Client 标识或版本缺失' } satisfies AdminMarketplaceActionData);
    }
    if (!reason) return fail(422, { message: '操作原因必填' } satisfies AdminMarketplaceActionData);
    try {
      const result = await authedPost<MarketplaceClientView>(
        cookies,
        `/api/v1/admin/marketplace/clients/${encodeURIComponent(clientId)}/emergency-disable`,
        { reason },
        request.headers.get('x-request-id'),
        { 'If-Match': String(version) }
      );
      if (result.ok) return { message: `Client 已紧急停用（${reason}）` } satisfies AdminMarketplaceActionData;
      return fail(result.status, { message: result.message } satisfies AdminMarketplaceActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '停用失败，请稍后重试' } satisfies AdminMarketplaceActionData);
    }
  },
  runReconciliation: async ({ request, cookies }) => {
    const form = await request.formData();
    const clientId = String(form.get('client_id') ?? '').trim();
    const afterCursor = Number(form.get('after_cursor') ?? 0);
    const reason = String(form.get('reason') ?? '').trim();
    if (!clientId) return fail(422, { message: '缺少 Client 标识' } satisfies AdminMarketplaceActionData);
    if (!reason) return fail(422, { message: '操作原因必填' } satisfies AdminMarketplaceActionData);
    try {
      const result = await authedPost<{ status: string; purchases_count: number; window_identity_sum: number }>(
        cookies,
        '/api/v1/admin/marketplace/reconciliation/run',
        { client_id: clientId, after_cursor: afterCursor, reason },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        const ok = result.data.status === 'consistent';
        return {
          message: `对账完成：${ok ? '一致' : '存在差异'}（${result.data.purchases_count} 笔，恒等式 ${result.data.window_identity_sum}）`
        } satisfies AdminMarketplaceActionData;
      }
      return fail(result.status, { message: result.message } satisfies AdminMarketplaceActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '对账失败，请稍后重试' } satisfies AdminMarketplaceActionData);
    }
  },
  retryRefund: async ({ request, cookies }) => {
    const form = await request.formData();
    const refundId = String(form.get('refund_id') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim();
    if (!refundId) return fail(422, { message: '缺少退款标识' } satisfies AdminMarketplaceActionData);
    if (!reason) return fail(422, { message: '操作原因必填' } satisfies AdminMarketplaceActionData);
    try {
      const result = await authedPost<MarketplaceRefundView>(
        cookies,
        `/api/v1/admin/marketplace/refunds/${encodeURIComponent(refundId)}/retry`,
        { reason },
        request.headers.get('x-request-id')
      );
      if (result.ok) return { message: `退款已处理：${result.data.status}` } satisfies AdminMarketplaceActionData;
      return fail(result.status, { message: result.message } satisfies AdminMarketplaceActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '重试失败，请稍后重试' } satisfies AdminMarketplaceActionData);
    }
  }
};
