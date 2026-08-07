// M07-UI-04：订单结果页——订单状态、扣费快照、entitlement 发放状态与补偿
// 待处理态；重复请求（同一幂等键）只会返回原订单。
import { redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { getAuthed } from '$lib/api/server';
import type { ShopOrder, Money } from '$lib/api/types';

export interface ShopOrderPageData {
  order: ShopOrder | null;
  balance: Money | null;
  error: string | null;
}

export const load: PageServerLoad = async ({ cookies, request, params }) => {
  const requestId = request.headers.get('x-request-id');
  const orderResult = await getAuthed<ShopOrder>(
    cookies,
    `/api/v1/shop/orders/${encodeURIComponent(params.id)}`,
    requestId
  );
  if (!orderResult.ok && orderResult.status === 401) throw redirect(303, '/login');
  if (!orderResult.ok) {
    return { order: null, balance: null, error: orderResult.message } satisfies ShopOrderPageData;
  }
  return { order: orderResult.data, balance: null, error: null } satisfies ShopOrderPageData;
};
