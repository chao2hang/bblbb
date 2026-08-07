// M12-UI-04：用户自己的 Marketplace Purchase 查询与退款状态入口。
//
// - load：GET /api/v1/marketplace/purchases（Session；只返回本人交易，
//   隐藏其他 Client/用户交易）；每笔附带 refunds 状态；
// - 无退款 action（退款是 Client 服务/管理员操作）；页面展示退款入口状态
//   （可退款字段来自服务端 Purchase/Refund 快照）。
import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { getAuthed } from '$lib/api/server';
import type { MarketplacePurchaseView } from '$lib/api/types';

export interface PurchasesPageData {
  purchases: MarketplacePurchaseView[];
  error: string | null;
}

export interface PurchasesActionData {
  message?: string;
  requestId?: string | null;
}

export const load: PageServerLoad = async ({ cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<{ purchases: MarketplacePurchaseView[] }>(
    cookies,
    '/api/v1/marketplace/purchases?limit=100',
    requestId
  );
  if (!result.ok && result.status === 401) throw redirect(303, '/login');
  if (!result.ok) {
    return { purchases: [], error: result.message } satisfies PurchasesPageData;
  }
  return { purchases: result.data.purchases ?? [], error: null } satisfies PurchasesPageData;
};

export const actions: Actions = {
  /** 刷新入口：重新查询（供 503 降级后恢复）。 */
  refresh: async ({ cookies, request }) => {
    const result = await getAuthed<{ purchases: MarketplacePurchaseView[] }>(
      cookies,
      '/api/v1/marketplace/purchases?limit=100',
      request.headers.get('x-request-id')
    );
    if (result.ok) return { message: `共 ${result.data.purchases?.length ?? 0} 笔交易` } satisfies PurchasesActionData;
    return fail(result.status, { message: result.message } satisfies PurchasesActionData);
  }
};
