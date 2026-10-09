// 积分与货币管理页——展示全站用户及其积分余额，支持在列表操作中直接调账。
import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPost, getAuthed } from '$lib/api/server';
import { newClientRequestId } from '$lib/api/client';

export interface AdminPointsUserItem {
  id: string;
  username: string;
  email: string;
  email_verified: boolean;
  status: string;
  display_name: string | null;
  level: number;
  trust_level?: number;
  roles: string[];
  coin_balance: number;
  created_at: number;
  updated_at: number;
  last_login_at: number | null;
  version: number;
}

export type AdminPointsLoadState = 'ok' | 'forbidden' | 'not_implemented' | 'error';

export interface AdminPointsPageData {
  state: AdminPointsLoadState;
  items: AdminPointsUserItem[];
  nextCursor: string | null;
  error: string | null;
  filters: { q: string; status: string };
}

export interface AdminPointsActionData {
  message?: string;
  requestId?: string | null;
  stepUpRequired?: boolean;
}

export const load: PageServerLoad = async ({ cookies, request, url }): Promise<AdminPointsPageData> => {
  const requestId = request.headers.get('x-request-id');
  const rawQ = url.searchParams.get('q') ?? url.searchParams.get('username') ?? '';
  const q = rawQ.trim();
  const status = (url.searchParams.get('status') ?? '').trim();
  const after = (url.searchParams.get('after') ?? '').trim();

  const params = new URLSearchParams({ limit: '30' });
  if (q) params.set('q', q);
  if (status) params.set('status', status);
  if (after) params.set('after', after);

  const result = await getAuthed<{ items: AdminPointsUserItem[]; next_cursor?: string | null }>(
    cookies,
    `/api/v1/admin/users?${params.toString()}`,
    requestId
  );

  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    if (result.status === 403) {
      return { state: 'forbidden', items: [], nextCursor: null, error: result.message, filters: { q, status } };
    }
    if (result.status === 501) {
      return { state: 'not_implemented', items: [], nextCursor: null, error: result.message, filters: { q, status } };
    }
    return { state: 'error', items: [], nextCursor: null, error: result.message, filters: { q, status } };
  }

  return {
    state: 'ok',
    items: result.data.items ?? [],
    nextCursor: result.data.next_cursor || null,
    error: null,
    filters: { q, status }
  };
};

export const actions: Actions = {
  // 积分调整（points.adjust，复用后端 POST /admin/points/adjust）
  adjust: async ({ request, cookies }) => {
    const form = await request.formData();
    const username = String(form.get('username') ?? '').trim();
    const currency = String(form.get('currency') ?? 'coin').trim().toLowerCase();
    const amount = Number(form.get('amount') ?? 0);
    const reason = String(form.get('reason') ?? '').trim();
    if (currency !== 'coin') {
      return fail(422, { message: '调账币种仅支持站点消费货币' });
    }
    if (!username || !amount || !reason) {
      return fail(422, { message: '用户名、非 0 调整数额与调整原因均为必填' });
    }
    const client_request_id = newClientRequestId();
    try {
      const result = await authedPost(
        cookies,
        '/api/v1/admin/points/adjust',
        { username, currency, amount, reason, client_request_id },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { message: `已成功为 ${username} 调整 ${amount > 0 ? '+' : ''}${amount} 站点消费货币` } satisfies AdminPointsActionData;
      }
      if (result.code === 'step_up_required') {
        return fail(403, {
          message: '此操作需要重新验证身份，请输入密码重新验证后重试',
          stepUpRequired: true
        } satisfies AdminPointsActionData);
      }
      return fail(result.status, { message: result.message, requestId: result.requestId } satisfies AdminPointsActionData);
    } catch {
      return fail(503, { message: '调整积分服务暂不可用，请稍后重试' } satisfies AdminPointsActionData);
    }
  },

  /** 重新验证身份（step-up 窗口过期后；与 storage/roles 页同款交互）。 */
  reauth: async ({ request, cookies }) => {
    const form = await request.formData();
    const password = String(form.get('password') ?? '');
    if (!password) {
      return fail(422, {
        message: '请输入当前密码'
      } satisfies AdminPointsActionData);
    }
    try {
      const result = await authedPost(
        cookies,
        '/api/v1/auth/re-auth',
        { password },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return {
          message: '已重新验证身份，请重试刚才的操作'
        } satisfies AdminPointsActionData;
      }
      return fail(result.status, {
        message: result.message
      } satisfies AdminPointsActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, {
        message: '验证失败，请稍后重试'
      } satisfies AdminPointsActionData);
    }
  }
};
