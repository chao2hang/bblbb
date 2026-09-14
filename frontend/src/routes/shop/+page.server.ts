// M07-UI-02：商城列表——服务端取在售商品（等级门槛/库存/限购/有效期由后端
// 裁决），平衡余额/等级一并安全投影（无敏感字段）。
import { redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { getAuthed } from '$lib/api/server';
import { activityCoinBalance } from '$lib/api/types';
import type { ActivitySummary, ShopProduct, TrustLevelProgress } from '$lib/api/types';
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
  // 注意：后端 GET /api/v1/shop/products 返回具名数组 { products: [...] }（同 admin 端），
  // 不是 { items: [...] }——曾致列表恒为空。
  const productsResult = await getAuthed<{ products: ShopProduct[] }>(
    cookies,
    '/api/v1/shop/products',
    requestId
  );
  if (!productsResult.ok && productsResult.status === 401) throw redirect(303, '/login');
  if (!productsResult.ok) {
    return { products: [], balance: null, level: null, error: productsResult.message } satisfies ShopPageData;
  }

  // 余额来自活动摘要，等级来自独立的 LinuxDo 式信任等级接口。
  let balance: Money | null = null;
  let level: number | null = null;
  const [summaryResult, trustResult] = await Promise.all([
    getAuthed<ActivitySummary>(cookies, '/api/v1/activity/summary', requestId),
    getAuthed<TrustLevelProgress>(cookies, '/api/v1/me/trust-level', requestId)
  ]);
  if (summaryResult?.ok) balance = activityCoinBalance(summaryResult.data);
  if (trustResult?.ok) level = trustResult.data.level;

  return {
    products: productsResult.data.products ?? [],
    balance,
    level,
    error: null
  } satisfies ShopPageData;
};
