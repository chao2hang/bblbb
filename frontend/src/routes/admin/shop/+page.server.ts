// M07-UI-08：管理端商城——商品 CRUD/发布/禁用 + 订单/补偿退款。
//
// - load：GET /admin/shop/config、/admin/shop/products、/admin/shop/orders；
//   401 → 登录；各列表分别降级（后端 501/403 显示开发中/无权限态）。
// - create/update：POST/PATCH 商品；update 携带 If-Match 版本（409
//   version_conflict 提示刷新）。
// - publish/disable：状态切换；refund：补偿退款（reason 必填）。
import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPost, authedPatch, getAuthed } from '$lib/api/server';
import { adminListState, type AdminLoadState } from '$lib/admin';
import type { ShopConfig, ShopOrder, ShopProduct, Money } from '$lib/api/types';

export interface AdminShopPageData {
  products: AdminLoadState<ShopProduct>;
  orders: AdminLoadState<ShopOrder>;
  config: { state: 'ok'; data: ShopConfig } | { state: 'error' | 'forbidden' | 'not_implemented'; message: string };
}

/** form action 返回投影（SvelteKit Actions 联合返回类型）。 */
export interface AdminShopActionData {
  message?: string;
  requestId?: string | null;
  code?: string | null;
}

export const load: PageServerLoad = async ({ cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const productsResult = await getAuthed<{ items: ShopProduct[] }>(
    cookies,
    '/api/v1/admin/shop/products',
    requestId
  );
  if (!productsResult.ok && productsResult.status === 401) throw redirect(303, '/login');
  const products = adminListState(productsResult);

  const ordersResult = await getAuthed<{ items: ShopOrder[] }>(
    cookies,
    '/api/v1/admin/shop/orders',
    requestId
  );
  const orders = adminListState(ordersResult);

  const configResult = await getAuthed<ShopConfig>(cookies, '/api/v1/admin/shop/config', requestId);
  let config: AdminShopPageData['config'];
  if (configResult.ok) {
    config = { state: 'ok', data: configResult.data };
  } else {
    config = { state: configResult.status === 403 ? 'forbidden' : 'error', message: configResult.message };
  }

  return { products, orders, config } satisfies AdminShopPageData;
};

const PRODUCT_FIELDS = ['kind', 'slug', 'title', 'description_safe', 'icon_token', 'slot', 'currency_id', 'unit_price', 'quantity_limit', 'required_level', 'validity_seconds', 'sale_start_at', 'sale_end_at', 'refund_policy'] as const;

function productBody(form: FormData): Record<string, unknown> {
  const body: Record<string, unknown> = {
    reason: String(form.get('reason') ?? '').trim()
  };
  for (const field of PRODUCT_FIELDS) {
    const raw = form.get(field);
    if (raw === null) continue;
    const value = String(raw).trim();
    if (value === '') continue;
    if (field === 'unit_price' || field === 'quantity_limit' || field === 'required_level') {
      body[field] = Number(value);
    } else if (field === 'validity_seconds' || field === 'sale_start_at' || field === 'sale_end_at') {
      body[field] = value === 'null' || value === '0' ? null : Number(value);
    } else {
      body[field] = value;
    }
  }
  return body;
}

export const actions: Actions = {
  create: async ({ request, cookies }) => {
    const form = await request.formData();
    const reason = String(form.get('reason') ?? '').trim();
    if (!reason) {
      return fail(422, { message: '操作原因必填' } satisfies AdminShopActionData);
    }
    try {
      const result = await authedPost<ShopProduct>(cookies, '/api/v1/admin/shop/products', productBody(form), request.headers.get('x-request-id'));
      if (result.ok) {
        return { message: `商品「${result.data.title}」已创建` } satisfies AdminShopActionData;
      }
      return fail(result.status, { message: result.message, requestId: result.requestId } satisfies AdminShopActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '创建失败，请稍后重试' } satisfies AdminShopActionData);
    }
  },
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
  }
};
