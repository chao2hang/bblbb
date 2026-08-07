// M07-UI-02：商城列表——服务端取在售商品（等级门槛/库存/限购/有效期由后端
// 裁决），平衡余额/等级一并安全投影（无敏感字段）。
import { redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { getAuthed } from '$lib/api/server';
import type { ActivitySummary, ShopProduct } from '$lib/api/types';
import type { Money } from '$lib/api/types';

export interface ShopPageData {
  products: ShopProduct[];
  /** 当前用户 coin 余额（缺失容忍）。 */
  balance: Money | null;
  level: number | null;
  error: string | null;
}

export const load: PageServerLoad = async ({ cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const productsResult = await getAuthed<{ items: ShopProduct[] }>(
    cookies,
    '/api/v1/shop/products',
    requestId
  );
  if (!productsResult.ok && productsResult.status === 401) throw redirect(303, '/login');
  if (!productsResult.ok) {
    return { products: [], balance: null, level: null, error: productsResult.message } satisfies ShopPageData;
  }

  // 余额/等级来自活跃摘要（M07-UI-01 安全投影）；失败不阻断商品列表。
  let balance: Money | null = null;
  let level: number | null = null;
  const summaryResult = await getAuthed<ActivitySummary>(cookies, '/api/v1/activity/summary', requestId);
  if (summaryResult.ok) {
    const summary = summaryResult.data;
    level = typeof summary.level === 'number' ? summary.level : null;
    const coin = (summary.balances ?? []).find((b) => b.currency === 'coin');
    balance = coin ?? null;
  }

  return {
    products: productsResult.data.items ?? [],
    balance,
    level,
    error: null
  } satisfies ShopPageData;
};
