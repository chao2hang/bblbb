// M07-UI-05：衣柜——装备/卸下/徽章（最多 3 个）/装饰预览。
//
// - load：GET /me/presentation（服务端编译安全 Token）+ GET /me/entitlements；
//   401 → 登录。
// - equip/unequip action：POST /me/entitlements/{id}/equip|unequip，body 含
//   expected_presentation_version（乐观并发）；409 slot_conflict/version_conflict
//   → 提示刷新。成功 use:enhance 默认 invalidateAll 刷新投影。
// - 过期自动卸下由后端裁决；前端对 expired 权益只展示历史态，不提供装备入口。

import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPost, getAuthed } from '$lib/api/server';
import type { Entitlement, Presentation } from '$lib/api/types';

export interface WardrobePageData {
  presentation: Presentation | null;
  entitlements: Entitlement[];
  error: string | null;
}

export interface WardrobeActionData {
  ok?: boolean;
  message?: string;
  code?: string | null;
  requestId?: string | null;
}

export const load: PageServerLoad = async ({ cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const presResult = await getAuthed<Presentation>(cookies, '/api/v1/me/presentation', requestId);
  if (!presResult.ok && presResult.status === 401) throw redirect(303, '/login');
  if (!presResult.ok) {
    return { presentation: null, entitlements: [], error: presResult.message } satisfies WardrobePageData;
  }
  const entResult = await getAuthed<{ items: Entitlement[] }>(cookies, '/api/v1/me/entitlements', requestId);
  if (!entResult.ok) {
    return { presentation: presResult.data, entitlements: [], error: entResult.message } satisfies WardrobePageData;
  }
  return { presentation: presResult.data, entitlements: entResult.data.items ?? [], error: null } satisfies WardrobePageData;
};

async function runEquip(
  cookies: Parameters<Actions['equip']>[0]['cookies'],
  entitlementId: string,
  expectedVersion: number,
  requestId: string | null,
  kind: 'equip' | 'unequip'
): Promise<ReturnType<typeof fail> | WardrobeActionData> {
  if (!entitlementId) {
    return fail(422, { message: '缺少权益标识' } satisfies WardrobeActionData);
  }
  if (!Number.isInteger(expectedVersion) || expectedVersion < 1) {
    return fail(422, { message: '展示版本缺失或无效，请刷新页面后重试' } satisfies WardrobeActionData);
  }
  try {
    const result = await authedPost<Presentation>(
      cookies,
      `/api/v1/me/entitlements/${encodeURIComponent(entitlementId)}/${kind}`,
      { expected_presentation_version: expectedVersion },
      requestId,
      { 'Idempotency-Key': `${kind}-${entitlementId}-${expectedVersion}` }
    );
    if (result.ok) {
      return { ok: true, message: kind === 'equip' ? '已装备' : '已卸下' } satisfies WardrobeActionData;
    }
    if (result.status === 409) {
      return fail(409, {
        message: result.message,
        code: result.code
      } satisfies WardrobeActionData);
    }
    return fail(result.status, { message: result.message, requestId: result.requestId } satisfies WardrobeActionData);
  } catch (e) {
    if (isRedirect(e)) throw e;
    return fail(503, { message: '衣柜服务暂不可用，请稍后重试' } satisfies WardrobeActionData);
  }
}

export const actions: Actions = {
  equip: async ({ request, cookies }) => {
    const form = await request.formData();
    return runEquip(
      cookies,
      String(form.get('entitlement_id') ?? ''),
      Number(form.get('expected_presentation_version') ?? 0),
      request.headers.get('x-request-id'),
      'equip'
    );
  },
  unequip: async ({ request, cookies }) => {
    const form = await request.formData();
    return runEquip(
      cookies,
      String(form.get('entitlement_id') ?? ''),
      Number(form.get('expected_presentation_version') ?? 0),
      request.headers.get('x-request-id'),
      'unequip'
    );
  }
};
