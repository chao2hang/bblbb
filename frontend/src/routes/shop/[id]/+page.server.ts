// M07-UI-03/04：商品详情 + 购买确认。
//
// - load：服务端取商品、余额/等级（activity summary）、已持有权益；401 → 登录。
// - purchase action：POST /api/v1/shop/orders（服务端重算价格/库存/等级/限购），
//   Idempotency-Key = 表单稳定 client_request_id——重复提交（网络超时重试）不
//   重复扣款；409 按 code 区分余额不足/商品换版/售罄/限购/权益未发放。
// - 无 JS：原生 form[method=POST] action=?/purchase，隐藏幂等键保持稳定。

import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPost, getAuthed } from '$lib/api/server';
import { activityCoinBalance } from '$lib/api/types';
import type {
  ActivitySummary,
  CosmeticDef,
  TrustLevelProgress,
  Entitlement,
  OrderCreateResult,
  ShopProduct,
  Money
} from '$lib/api/types';

export interface ShopProductPageData {
  product: ShopProduct | null;
  cosmetics?: CosmeticDef[];
  balance: Money | null;
  level: number | null;
  /** 已持有该商品权益数（限购展示）。 */
  ownedCount: number;
  error: string | null;
}

export interface ShopActionData {
  ok?: boolean;
  message?: string;
  code?: string | null;
  requestId?: string | null;
}

export const load: PageServerLoad = async ({ cookies, request, params }) => {
  const requestId = request.headers.get('x-request-id');
  const productResult = await getAuthed<ShopProduct>(
    cookies,
    `/api/v1/shop/products/${encodeURIComponent(params.id)}`,
    requestId
  );
  if (!productResult.ok && productResult.status === 401) throw redirect(303, '/login');
  if (!productResult.ok) {
    return { product: null, cosmetics: [], balance: null, level: null, ownedCount: 0, error: productResult.message } satisfies ShopProductPageData;
  }

  let balance: Money | null = null;
  let level: number | null = null;
  const [summaryResult, trustResult] = await Promise.all([
    getAuthed<ActivitySummary>(cookies, '/api/v1/activity/summary', requestId),
    getAuthed<TrustLevelProgress>(cookies, '/api/v1/me/trust-level', requestId)
  ]);
  if (summaryResult?.ok) balance = activityCoinBalance(summaryResult.data);
  if (trustResult?.ok) level = trustResult.data.level;

  // 已持有该商品数量（限购剩余展示）。
  let ownedCount = 0;
  const entResult = await getAuthed<{ entitlements?: Entitlement[]; items?: Entitlement[] }>(cookies, '/api/v1/me/entitlements', requestId);
  if (entResult?.ok) {
    ownedCount = (entResult.data.entitlements ?? entResult.data.items ?? []).filter(
      (e) => e.product_id === params.id && e.status !== 'revoked' && e.status !== 'consumed'
    ).length;
  }

  const cosmeticsResult = await getAuthed<{ cosmetics?: CosmeticDef[] }>(cookies, '/api/v1/shop/cosmetics', requestId);
  // 样式库拉取失败/未实现时降级为空列表（与 entResult 同款可选链兜底），
  // 不阻断商品详情展示。
  return { product: productResult.data, cosmetics: cosmeticsResult?.ok ? cosmeticsResult.data.cosmetics ?? [] : [], balance, level, ownedCount, error: null } satisfies ShopProductPageData;
};

export const actions: Actions = {
  purchase: async ({ request, cookies, params }) => {
    const form = await request.formData();
    const quantity = Number(form.get('quantity') ?? 1);
    const clientRequestId = String(form.get('client_request_id') ?? '').trim();
    if (!Number.isInteger(quantity) || quantity < 1 || quantity > 99) {
      return fail(422, { message: '购买数量需为 1–99 的整数' } satisfies ShopActionData);
    }
    if (clientRequestId.length < 16) {
      return fail(422, { message: '请求标识缺失，请刷新页面后重试' } satisfies ShopActionData);
    }
    try {
      const result = await authedPost<OrderCreateResult>(
        cookies,
        '/api/v1/shop/orders',
        {
          product_id: params.id,
          expected_product_version: Number(form.get('expected_product_version') ?? 0),
          quantity,
          client_request_id: clientRequestId
        },
        request.headers.get('x-request-id'),
        { 'Idempotency-Key': clientRequestId }
      );
      if (result.ok) {
        // 兼容旧版后端的顶层 order_id，同时优先使用规范化的 order.id。
        const legacyResult = result.data as OrderCreateResult & { order_id?: string };
        const orderId = result.data?.order?.id ?? legacyResult.order_id;
        if (!orderId) {
          return fail(502, {
            message: '订单已提交，但返回的订单号无效，请到积分明细核对扣款状态。'
          } satisfies ShopActionData);
        }
        throw redirect(303, `/shop/orders/${encodeURIComponent(orderId)}`);
      }
      return fail(result.status, {
        message: result.message,
        code: result.code,
        requestId: result.requestId
      } satisfies ShopActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '下单失败，请稍后重试' } satisfies ShopActionData);
    }
  }
};
