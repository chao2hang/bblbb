// 市场目录页（需登录，GAP-FIX-SPEC 四节：/marketplace）。
//
// 用户侧 offers 列表后端未实现（grep backend/src/routes/marketplace.rs：
// 只有 POST /marketplace/offers（Confidential Client 登记）与
// GET /marketplace/offers/{id}（按 id 读取），没有公开列表端点；
// client.ts 也没有 listMarketplaceOffers 封装）→ 按规格降级方案：
//   - 「我的市场交易」摘要：GET /api/v1/marketplace/purchases（Session，
//     只返回本人交易）取前几笔渲染（商户/金额/状态/平台费）；
//   - 精选应用卡片区：静态运营位卡片（说明：市场应用由管理员配置）；
//   - 「查看全部购买」链接 /marketplace/purchases。
// 401 → redirect /login。
import { redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { getAuthed } from '$lib/api/server';
import type { MarketplacePurchaseView } from '$lib/api/types';

/** 摘要表展示的最大行数（全量见 /marketplace/purchases）。 */
const SUMMARY_LIMIT = 5;

export interface MarketplacePageData {
  /** 摘要行（最近 N 笔）。 */
  purchases: MarketplacePurchaseView[];
  /** 交易统计（笔数/支出/平台费，按首笔币种口径）。 */
  totals: { count: number; spent: number; fees: number; currency: string | null };
  error: string | null;
}

export const load: PageServerLoad = async ({ cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<{ purchases?: MarketplacePurchaseView[] }>(
    cookies,
    '/api/v1/marketplace/purchases?limit=100',
    requestId
  );
  if (!result.ok && result.status === 401) throw redirect(303, '/login');
  if (!result.ok) {
    return {
      purchases: [],
      totals: { count: 0, spent: 0, fees: 0, currency: null },
      error: result.message
    } satisfies MarketplacePageData;
  }

  const purchases = result.data.purchases ?? [];
  const currency = purchases[0]?.currency_id ?? null;
  const totals = {
    count: purchases.length,
    spent: purchases.reduce((acc, p) => acc + (p.status !== 'refunded' ? p.amount : 0), 0),
    fees: purchases.reduce((acc, p) => acc + p.fee_amount, 0),
    currency
  };

  return {
    purchases: purchases.slice(0, SUMMARY_LIMIT),
    totals,
    error: null
  } satisfies MarketplacePageData;
};
