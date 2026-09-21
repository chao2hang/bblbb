// M07-UI-08：管理端商城——商品 CRUD/发布/禁用 + 订单/补偿退款。
//
// - load：GET /admin/shop/config、/admin/shop/products、/admin/shop/orders；
//   401 → 登录；各列表分别降级（后端 501/403 显示开发中/无权限态）。
// - update：PATCH 商品元数据（If-Match 版本，409 version_conflict 提示刷新）。
//   新建商品/新建样式统一走装扮工作台（/admin/shop/studio，M07-SHOP-STUDIO）。
// - publish/disable：状态切换；refund：补偿退款（reason 必填）。
import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPost, authedPatch, getAuthed } from '$lib/api/server';
import { adminListStateKeyed, type AdminLoadState } from '$lib/admin';
import { parseBatchIds, batchResult, type BatchOutcome } from '$lib/admin-batch';
import type { ShopConfig, ShopOrder, ShopProduct, Money, CosmeticDef } from '$lib/api/types';

export interface AdminShopPageData {
  products: AdminLoadState<ShopProduct>;
  orders: AdminLoadState<ShopOrder>;
  config: { state: 'ok'; data: ShopConfig } | { state: 'error' | 'forbidden' | 'not_implemented'; message: string };
  /** 装扮样式库定义（M07-SHOP-UI-10）：加载失败降级为空列表（表单仍可用预设）。 */
  cosmetics: CosmeticDef[];
  /** 当前激活的主业务分区（products / cosmetics / orders）。 */
  tab?: string;
}

/** form action 返回投影（SvelteKit Actions 联合返回类型）。 */
export interface AdminShopActionData {
  message?: string;
  requestId?: string | null;
  code?: string | null;
  fieldErrors?: Record<string, string>;
  input?: Record<string, unknown>;
}

export const load: PageServerLoad = async ({ cookies, request, url }) => {
  const requestId = request.headers.get('x-request-id');
  const tab = (url.searchParams.get('tab') ?? 'products').trim();
  // 注意形状：这两个端点返回具名数组（{ products: [] } / { orders: [] }），
  // 不是 { items: [] }——用 adminListStateKeyed 归一（曾致 SSR 500）。
  const productsResult = await getAuthed<{ products: ShopProduct[] }>(
    cookies,
    '/api/v1/admin/shop/products',
    requestId
  );
  if (!productsResult.ok && productsResult.status === 401) throw redirect(303, '/login');
  const products = adminListStateKeyed<ShopProduct>(productsResult, 'products');

  const ordersResult = await getAuthed<{ orders: ShopOrder[] }>(
    cookies,
    '/api/v1/admin/shop/orders',
    requestId
  );
  const orders = adminListStateKeyed<ShopOrder>(ordersResult, 'orders');

  const configResult = await getAuthed<ShopConfig>(cookies, '/api/v1/admin/shop/config', requestId);
  let config: AdminShopPageData['config'];
  if (configResult.ok) {
    config = { state: 'ok', data: configResult.data };
  } else {
    config = { state: configResult.status === 403 ? 'forbidden' : 'error', message: configResult.message };
  }

  // 样式库（可配置装扮）：失败降级为空列表，不阻断商品管理。
  const cosmeticsResult = await getAuthed<{ cosmetics: CosmeticDef[] }>(
    cookies,
    '/api/v1/admin/shop/cosmetics',
    requestId
  );
  const cosmetics: CosmeticDef[] = cosmeticsResult.ok ? cosmeticsResult.data.cosmetics ?? [] : [];

  return { products, orders, config, cosmetics, tab } satisfies AdminShopPageData;
};

const PRODUCT_FIELDS = [
  'kind',
  'slug',
  'title',
  'description_safe',
  'icon_token',
  'presentation_tokens',
  'asset_attachment_id',
  'slot',
  'currency_id',
  'unit_price',
  'quantity_limit',
  'stock_remaining',
  'required_level',
  'validity_seconds',
  'sale_start_at',
  'sale_end_at',
  'refund_policy'
] as const;

/** 装备类商品的默认展示槽位（M07-SHOP-UI-09）：表单未带 slot 时兜底；
 *  reaction_pack/utility 不装备到槽位；title_prefix 装备到称号槽位。 */
const KIND_DEFAULT_SLOT: Record<string, string> = {
  cosmetic_nickname: 'nickname_color',
  cosmetic_avatar: 'avatar_frame',
  cosmetic_avatar_attachment: 'avatar_frame',
  cosmetic_badge: 'profile_badges',
  profile_effect: 'profile_effect',
  post_effect: 'post_effect',
  title_prefix: 'title_prefix'
};

function productBody(form: FormData): Record<string, unknown> {
  const body: Record<string, unknown> = {
    reason: String(form.get('reason') ?? '').trim()
  };
  for (const field of PRODUCT_FIELDS) {
    const raw = form.get(field);
    if (raw === null) continue;
    const value = String(raw).trim();
    if (field === 'presentation_tokens') {
      // 始终按数组提交（空串 → []）：编辑时清空全部 Token 才能真正落库，
      // 否则空字段被跳过、后端 COALESCE 会保留旧 Token。
      body[field] = value.split(',').map((token) => token.trim()).filter(Boolean);
      continue;
    }
    if (field === 'slot') {
      // 槽位显式提交：空串 = 消耗品/道具类清空槽位（后端按类型放行）；
      // 装备类商品由表单保证非空。字段缺省（旧表单/无 JS 回退）不投。
      if (value !== '') body[field] = value;
      continue;
    }
    if (value === '') continue;
    if (field === 'unit_price' || field === 'quantity_limit' || field === 'required_level') {
      body[field] = Number(value);
    } else if (field === 'stock_remaining') {
      body[field] = value === 'null' ? null : Number(value);
    } else if (field === 'validity_seconds' || field === 'sale_start_at' || field === 'sale_end_at') {
      body[field] = value === 'null' || value === '0' ? null : Number(value);
    } else {
      body[field] = value;
    }
  }
  // 默认货币与槽位兜底（装备类商品缺省槽位按类型映射；P2-06）
  if (!body.currency_id) body.currency_id = 'coin';
  if (!body.slot && typeof body.kind === 'string' && KIND_DEFAULT_SLOT[body.kind]) {
    body.slot = KIND_DEFAULT_SLOT[body.kind];
  }
  return body;
}

export const actions: Actions = {
  update: async ({ request, cookies }) => {
    const form = await request.formData();
    const id = String(form.get('id') ?? '').trim();
    const version = Number(form.get('version') ?? 0);
    if (!id) {
      return fail(422, { message: '缺少商品标识' } satisfies AdminShopActionData);
    }
    if (!Number.isInteger(version) || version < 1) {
      return fail(422, { message: '商品版本缺失或无效，请刷新后重试' } satisfies AdminShopActionData);
    }
    try {
      const result = await authedPatch<ShopProduct>(
        cookies,
        `/api/v1/admin/shop/products/${encodeURIComponent(id)}`,
        productBody(form),
        { 'If-Match': String(version) },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { message: `商品「${result.data.title}」已更新` } satisfies AdminShopActionData;
      }
      if (result.status === 409) {
        return fail(409, { message: `版本冲突：${result.message}，请刷新后重试` } satisfies AdminShopActionData);
      }
      return fail(result.status, { message: result.message, requestId: result.requestId } satisfies AdminShopActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '更新失败，请稍后重试' } satisfies AdminShopActionData);
    }
  },
  publish: async ({ request, cookies }) => {
    const form = await request.formData();
    const id = String(form.get('id') ?? '').trim();
    if (!id) {
      return fail(422, { message: '缺少商品标识' } satisfies AdminShopActionData);
    }
    try {
      const result = await authedPost<ShopProduct>(
        cookies,
        `/api/v1/admin/shop/products/${encodeURIComponent(id)}/publish`,
        {},
        request.headers.get('x-request-id')
      );
      if (result.ok) return { message: '商品已发布' } satisfies AdminShopActionData;
      return fail(result.status, { message: result.message } satisfies AdminShopActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '发布失败，请稍后重试' } satisfies AdminShopActionData);
    }
  },
  /** 批量上架（M18-ADMIN-BATCH）：循环调用 publish 单条端点（无 If-Match，
   * 与单条 publish action 完全一致），逐条汇总成败。 */
  batchPublish: async ({ request, cookies }) => {
    const form = await request.formData();
    const ids = parseBatchIds(form);
    if (ids.length === 0) {
      return fail(422, { message: '未选择任何商品' } satisfies AdminShopActionData);
    }
    const outcome: BatchOutcome = { okCount: 0, failures: [] };
    for (const id of ids) {
      try {
        const r = await authedPost<ShopProduct>(
          cookies,
          `/api/v1/admin/shop/products/${encodeURIComponent(id)}/publish`,
          {},
          request.headers.get('x-request-id')
        );
        if (r.ok) outcome.okCount++;
        else outcome.failures.push({ id, message: r.message });
      } catch {
        outcome.failures.push({ id, message: '网络错误' });
      }
    }
    const r = batchResult(outcome, '批量上架');
    return r.ok
      ? { message: r.message } satisfies AdminShopActionData
      : fail(r.status, { message: r.message } satisfies AdminShopActionData);
  },
  disable: async ({ request, cookies }) => {
    const form = await request.formData();
    const id = String(form.get('id') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim();
    if (!id) {
      return fail(422, { message: '缺少商品标识' } satisfies AdminShopActionData);
    }
    if (!reason) {
      return fail(422, { message: '停售原因必填' } satisfies AdminShopActionData);
    }
    try {
      const result = await authedPost<ShopProduct>(
        cookies,
        `/api/v1/admin/shop/products/${encodeURIComponent(id)}/disable`,
        { reason },
        request.headers.get('x-request-id')
      );
      if (result.ok) return { message: '商品已停售' } satisfies AdminShopActionData;
      return fail(result.status, { message: result.message } satisfies AdminShopActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '停售失败，请稍后重试' } satisfies AdminShopActionData);
    }
  },
  /** 批量下架（M18-ADMIN-BATCH）：循环调用 disable 单条端点（POST { reason }，
   * 无 If-Match，与单条 disable action 完全一致），同一原因逐条写审计。 */
  batchDisable: async ({ request, cookies }) => {
    const form = await request.formData();
    const ids = parseBatchIds(form);
    const reason = String(form.get('reason') ?? '').trim();
    if (ids.length === 0) {
      return fail(422, { message: '未选择任何商品' } satisfies AdminShopActionData);
    }
    if (!reason) {
      return fail(422, { message: '停售原因必填（写审计）' } satisfies AdminShopActionData);
    }
    const outcome: BatchOutcome = { okCount: 0, failures: [] };
    for (const id of ids) {
      try {
        const r = await authedPost<ShopProduct>(
          cookies,
          `/api/v1/admin/shop/products/${encodeURIComponent(id)}/disable`,
          { reason },
          request.headers.get('x-request-id')
        );
        if (r.ok) outcome.okCount++;
        else outcome.failures.push({ id, message: r.message });
      } catch {
        outcome.failures.push({ id, message: '网络错误' });
      }
    }
    const r = batchResult(outcome, '批量下架');
    return r.ok
      ? { message: r.message } satisfies AdminShopActionData
      : fail(r.status, { message: r.message } satisfies AdminShopActionData);
  },
  refund: async ({ request, cookies }) => {
    const form = await request.formData();
    const id = String(form.get('id') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim();
    const amountRaw = String(form.get('amount') ?? '').trim();
    if (!id) {
      return fail(422, { message: '缺少订单标识' } satisfies AdminShopActionData);
    }
    if (!reason) {
      return fail(422, { message: '退款原因必填' } satisfies AdminShopActionData);
    }
    const amount: Money | null = amountRaw === '' ? null : { currency: 'coin', amount: Number(amountRaw) };
    try {
      const result = await authedPost<ShopOrder>(
        cookies,
        `/api/v1/admin/shop/orders/${encodeURIComponent(id)}/refund`,
        { reason_code: 'compensation', reason, amount },
        request.headers.get('x-request-id')
      );
      if (result.ok) return { message: '退款补偿已提交' } satisfies AdminShopActionData;
      return fail(result.status, { message: result.message, requestId: result.requestId } satisfies AdminShopActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '退款失败，请稍后重试' } satisfies AdminShopActionData);
    }
  },
  /** 更新装扮样式（改名/调样式/归档恢复），原因写审计。 */
  updateCosmetic: async ({ request, cookies }) => {
    const form = await request.formData();
    const id = String(form.get('id') ?? '').trim();
    const name = String(form.get('name') ?? '').trim();
    const status = String(form.get('status') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim() || '更新装扮样式';
    if (!id) return fail(422, { message: '缺少样式标识' } satisfies AdminShopActionData);
    const body: Record<string, unknown> = { reason };
    if (name) body.name = name;
    if (status) body.status = status;
    if (form.get('style')) {
      try {
        body.style = JSON.parse(String(form.get('style')));
      } catch {
        return fail(422, { message: '样式参数无效' } satisfies AdminShopActionData);
      }
    }
    try {
      const result = await authedPatch<CosmeticDef>(
        cookies,
        `/api/v1/admin/shop/cosmetics/${encodeURIComponent(id)}`,
        body,
        {},
        request.headers.get('x-request-id')
      );
      if (result.ok) return { message: `样式「${result.data.name}」已更新` } satisfies AdminShopActionData;
      return fail(result.status, { message: result.message, requestId: result.requestId } satisfies AdminShopActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '更新样式失败，请稍后重试' } satisfies AdminShopActionData);
    }
  },
  /** 快捷发布商品（Steam头像框/Steam背景/彩色昵称快速上架，单事务）。 */
  quickPublish: async ({ request, cookies }) => {
    const form = await request.formData();
    const cosmeticRaw = String(form.get('cosmetic') ?? '').trim();
    const productRaw = String(form.get('product') ?? '').trim();
    if (!cosmeticRaw || !productRaw) {
      return fail(422, { message: '缺少商品或装扮配置参数' } satisfies AdminShopActionData);
    }
    let cosmetic: Record<string, unknown>;
    let product: Record<string, unknown>;
    try {
      cosmetic = JSON.parse(cosmeticRaw);
      product = JSON.parse(productRaw);
    } catch {
      return fail(422, { message: '配置数据格式无效' } satisfies AdminShopActionData);
    }
    try {
      const result = await authedPost<{ product: ShopProduct; cosmetic: CosmeticDef }>(
        cookies,
        '/api/v1/admin/shop/studio/publish',
        { cosmetic, product },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { message: `商品「${result.data.product.title}」已成功上架！` } satisfies AdminShopActionData;
      }
      return fail(result.status, { message: result.message, requestId: result.requestId } satisfies AdminShopActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '上架失败，请稍后重试' } satisfies AdminShopActionData);
    }
  }
};
