// P0 整改：/admin/feature-flags —— 可选能力运行时开关（持久化 + 审计）。
// - load：GET /api/v1/admin/feature-flags（admin.manage）；
// - toggle action：PATCH /api/v1/admin/feature-flags/{name}
//   （If-Match version + reason 审计；成功后后端原地重载快照）；
// - killSwitch action：POST /api/v1/admin/feature-flags/kill-switch
//   （紧急关闭全部可选能力；危险操作，reason 必填）。
import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPatch, authedPost, getAuthed } from '$lib/api/server';
import { newClientRequestId } from '$lib/api/client';
import { flagLabel } from './flags';

export type AdminFlagsState = 'ok' | 'forbidden' | 'not_implemented' | 'error';

export interface AdminFlagItem {
  name: string;
  enabled: boolean;
  effective_at: number;
  version: number;
  updated_by: string | null;
  updated_at: number;
}

export interface AdminFlagsPageData {
  state: AdminFlagsState;
  flags: AdminFlagItem[] | null;
  kill_switch: boolean;
  error: string | null;
}

export interface AdminFlagsActionData {
  message?: string;
  requestId?: string | null;
  conflict?: boolean;
}

export const load: PageServerLoad = async ({ cookies, request }): Promise<AdminFlagsPageData> => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<{
    flags: AdminFlagItem[];
    kill_switch: boolean;
  }>(cookies, '/api/v1/admin/feature-flags', requestId);
  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    if (result.status === 403) {
      return { state: 'forbidden', flags: null, kill_switch: false, error: result.message };
    }
    if (result.status === 501) {
      return { state: 'not_implemented', flags: null, kill_switch: false, error: result.message };
    }
    return { state: 'error', flags: null, kill_switch: false, error: result.message };
  }
  return {
    state: 'ok',
    flags: result.data.flags,
    kill_switch: result.data.kill_switch === true,
    error: null
  };
};

export const actions: Actions = {
  /** 启停单个 Flag（If-Match version + reason）。 */
  toggle: async ({ request, cookies }) => {
    const form = await request.formData();
    const name = String(form.get('name') ?? '').trim();
    const enabled = String(form.get('enabled') ?? '') === 'true';
    const version = Number(form.get('version') ?? 0);
    const reason = String(form.get('reason') ?? '').trim();
    if (!name || version <= 0) return fail(422, { message: '参数缺失' });
    if (!reason) return fail(422, { message: '操作原因必填（写审计）' });

    try {
      const result = await authedPatch(
        cookies,
        `/api/v1/admin/feature-flags/${encodeURIComponent(name)}`,
        { enabled, expected_version: version, reason },
        { 'If-Match': String(version) },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { message: `功能「${flagLabel(name)}」已${enabled ? '启用' : '停用'}` };
      }
      if (result.status === 409) {
        return fail(409, {
          conflict: true,
          message: `版本冲突：${result.message}（Flag 已被其他人修改，请刷新后重试）`
        });
      }
      return fail(result.status, { message: result.message, requestId: result.requestId });
    } catch {
      return fail(503, { message: '保存失败，请稍后重试' });
    }
  },

  /** 紧急关闭（kill switch）：全部可选能力立即禁用并持久化。 */
  killSwitch: async ({ request, cookies }) => {
    const form = await request.formData();
    const reason = String(form.get('reason') ?? '').trim();
    if (!reason) return fail(422, { message: '紧急关闭原因必填（写审计）' });
    try {
      const result = await authedPost(
        cookies,
        '/api/v1/admin/feature-flags/kill-switch',
        { reason },
        request.headers.get('x-request-id'),
        // 契约要求 Idempotency-Key（16-200 字符）：kill-switch 为危险操作，
        // 带幂等键便于审计与重放识别（后端全局状态天然幂等）。
        { 'Idempotency-Key': newClientRequestId() }
      );
      if (result.ok) {
        return { message: '紧急关闭已生效：全部可选能力已禁用' };
      }
      return fail(result.status, { message: result.message, requestId: result.requestId });
    } catch {
      return fail(503, { message: '操作失败，请稍后重试' });
    }
  }
};
