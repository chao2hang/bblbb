// M13-UI-04 + GAP-FIX（视觉对齐 M17-GAPFIX-06）：等级管理——
// 1) 等级列表（GET /admin/levels：level/name/min_exp/user_count/权益/状态，
//    level.manage）+ 详细设置行内编辑（PATCH /admin/levels/{level}，
//    If-Match + reason 审计，level.manage）；
// 2) 附件配额只读卡（M06-QUOTA，脱敏投影）。
import { fail, isRedirect, redirect, type Cookies } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPatch, getAuthed } from '$lib/api/server';

/** 等级规则行（GET /admin/levels 投影）。 */
export interface AdminLevelRule {
  level: number;
  name: string;
  min_exp: number;
  user_count: number;
  attachment_quota: number;
  daily_post_limit: number;
  daily_comment_limit: number;
  is_enabled: boolean;
  version: number;
}

export interface LevelQuotaView {
  level: number;
  policy: {
    level: number;
    single_file_max_bytes: number;
    total_bytes: number;
    daily_upload_bytes: number;
    retention_days: number;
    policy_version: number;
  } | null;
}

export type AdminLevelsState = 'ok' | 'forbidden' | 'not_implemented' | 'error';

export interface AdminLevelsPageData {
  state: AdminLevelsState;
  quota: LevelQuotaView | null;
  error: string | null;
  levels: {
    state: 'ok' | 'forbidden' | 'error';
    items: AdminLevelRule[];
    message: string | null;
  };
}

/** update action 返回投影（SvelteKit Actions 联合类型）。 */
export interface AdminLevelsActionData {
  levels: {
    state: 'ok' | 'forbidden' | 'error';
    items: AdminLevelRule[];
    message: string | null;
  };
  /** 结果提示（成功/校验失败/版本冲突）。 */
  message?: string | null;
}

async function reloadLevels(cookies: Cookies, requestId: string | null): Promise<AdminLevelsPageData['levels']> {
  const result = await getAuthed<{ items: AdminLevelRule[] }>(cookies, '/api/v1/admin/levels', requestId);
  if (result.ok) {
    return { state: 'ok', items: result.data.items, message: null };
  }
  if (result.status === 403) return { state: 'forbidden', items: [], message: result.message };
  return { state: 'error', items: [], message: result.message };
}

export const load: PageServerLoad = async ({ cookies, request }): Promise<AdminLevelsPageData> => {
  const requestId = request.headers.get('x-request-id');
  const levels = await reloadLevels(cookies, requestId);

  const result = await getAuthed<LevelQuotaView>(
    cookies,
    '/api/v1/admin/levels/1/attachment-quota',
    requestId
  );
  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    if (result.status === 403) return { state: 'forbidden', quota: null, error: result.message, levels };
    if (result.status === 501) return { state: 'not_implemented', quota: null, error: result.message, levels };
    return { state: 'error', quota: null, error: result.message, levels };
  }
  return { state: 'ok', quota: result.data, error: null, levels };
};

export const actions: Actions = {
  /** 详细设置：PATCH /admin/levels/{level}（If-Match + reason；缺省字段保持原值）。 */
  update: async ({ request, cookies }) => {
    const form = await request.formData();
    const level = Number(form.get('level') ?? 0);
    const version = Number(form.get('version') ?? 0);
    const reason = String(form.get('reason') ?? '').trim();
    if (!Number.isInteger(level) || level < 1) {
      return fail(422, { levels: await reloadLevels(cookies, null), message: '缺少等级标识' } satisfies AdminLevelsActionData);
    }
    if (!Number.isInteger(version) || version < 1) {
      return fail(409, { levels: await reloadLevels(cookies, null), message: '版本缺失或无效，请刷新后重试' } satisfies AdminLevelsActionData);
    }
    if (!reason) {
      return fail(422, { levels: await reloadLevels(cookies, null), message: '操作原因必填（写入审计日志）' } satisfies AdminLevelsActionData);
    }

    const body: Record<string, unknown> = { reason };
    const name = String(form.get('name') ?? '').trim();
    if (name) body.name = name;
    for (const key of ['min_exp', 'daily_post_limit', 'daily_comment_limit', 'attachment_quota'] as const) {
      const raw = String(form.get(key) ?? '').trim();
      if (raw === '') continue;
      const n = Number(raw);
      if (!Number.isInteger(n) || n < 0) {
        return fail(422, { levels: await reloadLevels(cookies, null), message: `${key} 需为非负整数` } satisfies AdminLevelsActionData);
      }
      body[key] = n;
    }
    const isEnabledRaw = String(form.get('is_enabled') ?? '').trim();
    if (isEnabledRaw === 'true' || isEnabledRaw === 'on') body.is_enabled = true;
    if (isEnabledRaw === 'false') body.is_enabled = false;

    try {
      const result = await authedPatch<unknown>(
        cookies,
        `/api/v1/admin/levels/${level}`,
        body,
        { 'If-Match': String(version) },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { levels: await reloadLevels(cookies, request.headers.get('x-request-id')), message: `等级 ${level} 已更新` } satisfies AdminLevelsActionData;
      }
      if (result.status === 409) {
        return fail(409, { levels: await reloadLevels(cookies, request.headers.get('x-request-id')), message: `版本冲突：${result.message}，请刷新后重试` } satisfies AdminLevelsActionData);
      }
      return fail(result.status, { levels: await reloadLevels(cookies, request.headers.get('x-request-id')), message: result.message } satisfies AdminLevelsActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { levels: await reloadLevels(cookies, null), message: '保存失败，请稍后重试' } satisfies AdminLevelsActionData);
    }
  }
};
