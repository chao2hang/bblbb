// M12-UI-01/02/03：托管 Checkout 确认页（Session + CSRF + 幂等键）。
//
// - load：GET /api/v1/marketplace/checkout-intents/{id}——商户/商品/数量/
//   准确金额/余额变化/Scope/授权期限全部来自服务端快照；
// - confirm/deny action：POST /confirm（决策 + interaction_id +
//   expected_intent_version + 稳定幂等键）；请求体不含可篡改的价格、用户、
//   货币或余额字段；
// - 状态覆盖：成功 / 失败（insufficient_funds 等）/ 处理中 / 重复请求
//   （idempotency replay 原结果）/ 过期 Intent / request ID。
import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPost, getAuthed } from '$lib/api/server';
import type { MarketplaceCheckoutView, MarketplacePurchaseView } from '$lib/api/types';

export interface CheckoutPageData {
  checkout: MarketplaceCheckoutView | null;
  error: { code: string; message: string } | null;
}

export interface CheckoutActionData {
  purchase?: MarketplacePurchaseView;
  message?: string;
  code?: string | null;
  requestId?: string | null;
  ok?: boolean;
}

export const load: PageServerLoad = async ({ params, cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<MarketplaceCheckoutView>(
    cookies,
    `/api/v1/marketplace/checkout-intents/${encodeURIComponent(params.id)}`,
    requestId
  );
  if (!result.ok && result.status === 401) throw redirect(303, '/login');
  if (!result.ok) {
    return { checkout: null, error: { code: result.code ?? 'error', message: result.message } } satisfies CheckoutPageData;
  }
  return { checkout: result.data, error: null } satisfies CheckoutPageData;
};

export const actions: Actions = {
  confirm: async ({ params, request, cookies }) => {
    const form = await request.formData();
    const expectedVersion = Number(form.get('expected_intent_version') ?? 0);
    const clientRequestId = String(form.get('client_request_id') ?? '').trim();
    if (!Number.isInteger(expectedVersion) || expectedVersion < 1) {
      return fail(422, { message: '结账信息已变化，请刷新页面后重新确认' } satisfies CheckoutActionData);
    }
    if (clientRequestId.length < 16) {
      return fail(422, { message: '请求标识缺失，请刷新页面后重试' } satisfies CheckoutActionData);
    }
    try {
      const result = await authedPost<{ purchase: MarketplacePurchaseView }>(
        cookies,
        `/api/v1/marketplace/checkout-intents/${encodeURIComponent(params.id)}/confirm`,
        {
          interaction_id: params.id,
          decision: 'confirm',
          expected_intent_version: expectedVersion
        },
        request.headers.get('x-request-id'),
        { 'Idempotency-Key': clientRequestId }
      );
      if (result.ok) {
        return { ok: true, purchase: result.data.purchase } satisfies CheckoutActionData;
      }
      return fail(result.status, {
        message: result.message,
        code: result.code,
        requestId: result.requestId
      } satisfies CheckoutActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '结账服务暂不可用，请稍后重试' } satisfies CheckoutActionData);
    }
  },
  deny: async ({ params, request, cookies }) => {
    const form = await request.formData();
    const expectedVersion = Number(form.get('expected_intent_version') ?? 0);
    const clientRequestId = String(form.get('client_request_id') ?? '').trim();
    if (!Number.isInteger(expectedVersion) || expectedVersion < 1 || clientRequestId.length < 16) {
      return fail(422, { message: '请求无效，请刷新页面后重试' } satisfies CheckoutActionData);
    }
    try {
      const result = await authedPost<{ checkout: { intent_id: string; status: string } }>(
        cookies,
        `/api/v1/marketplace/checkout-intents/${encodeURIComponent(params.id)}/confirm`,
        {
          interaction_id: params.id,
          decision: 'deny',
          expected_intent_version: expectedVersion
        },
        request.headers.get('x-request-id'),
        { 'Idempotency-Key': clientRequestId }
      );
      if (result.ok) {
        return { ok: true, message: '已取消本次授权' } satisfies CheckoutActionData;
      }
      return fail(result.status, { message: result.message, code: result.code } satisfies CheckoutActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '取消失败，请稍后重试' } satisfies CheckoutActionData);
    }
  }
};
