// M13-UI-01/ADMIN-02：管理用户页——列表/状态更新（If-Match version + reason +
// recent-auth；管理 DTO 不含凭据）。
// M18-ADMIN-DIALOG：新增批量 ?/batchUpdate——循环既有单条端点
// PATCH /api/v1/admin/users/{id}（versions 与 ids 一一对应作 If-Match）。
import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedDelete, authedDeleteBody, authedPatch, authedPost, getAuthed } from '$lib/api/server';
import {
  batchResult,
  emptyBatchSelection,
  parseBatchIds,
  type BatchOutcome
} from '$lib/admin-batch';

export interface AdminRoleOption {
  id: string;
  name: string;
  is_system?: boolean;
}

export interface AdminUserItem {
  id: string;
  username: string;
  email: string;
  email_verified: boolean;
  status: string;
  display_name: string | null;
  level: number;
  /** M20-TRUST 信任等级（TL0–TL4；0070 迁移后必有，旧 fixture 允许缺失）。 */
  trust_level?: number;
  roles: string[];
  /** 站点消费货币实时余额（point_accounts；无账户 = 0）。 */
  coin_balance: number;
  created_at: number;
  updated_at: number;
  last_login_at: number | null;
  version: number;
}

export interface BlacklistEntry {
  id: string;
  nickname: string;
  nickname_normalized: string;
  reason: string | null;
  created_by: string | null;
  created_at: number;
}

export type AdminUsersLoadState = 'ok' | 'forbidden' | 'not_implemented' | 'error';

export interface AdminUsersPageData {
  state: AdminUsersLoadState;
  items: AdminUserItem[] | null;
  error: string | null;
  blacklist?: BlacklistEntry[];
  blacklistTotal?: number;
  rolesList?: AdminRoleOption[];
}

export interface AdminUsersActionData {
  message?: string;
  requestId?: string | null;
  conflict?: boolean;
  stepUpRequired?: boolean;
}

export const load: PageServerLoad = async ({ cookies, request, url }): Promise<AdminUsersPageData> => {
  const requestId = request.headers.get('x-request-id');
  const q = (url.searchParams.get('q') ?? '').trim();
  const status = (url.searchParams.get('status') ?? '').trim();

  const params = new URLSearchParams({ limit: '100' });
  if (q) params.set('q', q);
  if (status) params.set('status', status);

  const result = await getAuthed<{ items: AdminUserItem[] }>(
    cookies,
    `/api/v1/admin/users?${params.toString()}`,
    requestId
  );
  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    if (result.status === 403) {
      return { state: 'forbidden', items: null, error: result.message, blacklist: [], blacklistTotal: 0, rolesList: [] };
    }
    if (result.status === 501) {
      return { state: 'not_implemented', items: null, error: result.message, blacklist: [], blacklistTotal: 0, rolesList: [] };
    }
    return { state: 'error', items: null, error: result.message, blacklist: [], blacklistTotal: 0, rolesList: [] };
  }

  let blacklist: BlacklistEntry[] = [];
  let blacklistTotal = 0;
  const blResult = await getAuthed<{ items: BlacklistEntry[]; total: number }>(
    cookies,
    '/api/v1/admin/nickname-blacklist?limit=100',
    requestId
  );
  if (blResult.ok) {
    blacklist = blResult.data.items;
    blacklistTotal = blResult.data.total;
  }

  let rolesList: AdminRoleOption[] = [];
  const rolesResult = await getAuthed<{ items: AdminRoleOption[] }>(
    cookies,
    '/api/v1/admin/roles',
    requestId
  );
  if (rolesResult.ok && Array.isArray(rolesResult.data.items)) {
    rolesList = rolesResult.data.items;
  }

  return { state: 'ok', items: result.data.items, error: null, blacklist, blacklistTotal, rolesList };
};

export const actions: Actions = {
  update: async ({ request, cookies }) => {
    const form = await request.formData();
    const id = String(form.get('id') ?? '').trim();
    const status = String(form.get('status') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim();
    const version = Number(form.get('version') ?? 0);
    if (!reason) return fail(422, { message: '操作原因必填（写审计）' });
    if (!['pending', 'active', 'restricted', 'banned'].includes(status)) {
      return fail(422, { message: '无效状态' });
    }
    try {
      const result = await authedPatch<AdminUserItem>(
        cookies,
        `/api/v1/admin/users/${encodeURIComponent(id)}`,
        { status, reason },
        { 'If-Match': String(version) },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { message: `用户 ${result.data.username} 状态已更新为 ${result.data.status}` };
      }
      if (result.code === 'step_up_required') {
        return fail(403, {
          message: '此操作需要重新验证身份，请输入密码重新验证后重试',
          stepUpRequired: true
        } satisfies AdminUsersActionData);
      }
      if (result.status === 409) {
        return fail(409, { conflict: true, message: `版本冲突：${result.message}（请刷新后重试）` } satisfies AdminUsersActionData);
      }
      return fail(result.status, { message: result.message, requestId: result.requestId } satisfies AdminUsersActionData);
    } catch {
      return fail(503, { message: '保存失败，请稍后重试' });
    }
  },

  /**
   * M20-TRUST：手动设置信任等级（POST /api/v1/admin/users/{id}/trust-level，
   * level.manage）。TL4 只能由此授予；原因写审计 admin.trust_level.set。
   */
  setTrust: async ({ request, cookies }) => {
    const form = await request.formData();
    const id = String(form.get('id') ?? '').trim();
    const level = Number(form.get('level') ?? -1);
    const reason = String(form.get('reason') ?? '').trim();
    if (!id) return fail(422, { message: '缺少用户标识' });
    if (!Number.isInteger(level) || level < 0 || level > 4) {
      return fail(422, { message: '信任等级必须为 0–4' });
    }
    if (!reason) return fail(422, { message: '操作原因必填（写审计）' });
    try {
      const result = await authedPost<{ from_level: number; to_level: number; changed: boolean }>(
        cookies,
        `/api/v1/admin/users/${encodeURIComponent(id)}/trust-level`,
        { level, reason },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        const suffix = result.data.changed === false ? '（等级未变化）' : '';
        return { message: `信任等级已设置为 TL${result.data.to_level}${suffix}` };
      }
      if (result.code === 'step_up_required') {
        return fail(403, {
          message: '此操作需要重新验证身份，请输入密码重新验证后重试',
          stepUpRequired: true
        } satisfies AdminUsersActionData);
      }
      if (result.status === 404) {
        return fail(404, { message: '目标用户不存在' } satisfies AdminUsersActionData);
      }
      return fail(result.status, { message: result.message, requestId: result.requestId } satisfies AdminUsersActionData);
    } catch {
      return fail(503, { message: '保存失败，请稍后重试' });
    }
  },

  /**
   * 批量设置状态（M18-ADMIN-DIALOG）：循环既有单条端点
   * PATCH /api/v1/admin/users/{id}（status/reason + If-Match 乐观锁），
   * versions 与 ids 顺序一一对应；逐条 try/catch 汇总成败。
   */
  /**
   * 批量设置状态（M18-ADMIN-BATCH-01）：对接后端原生 POST /api/v1/admin/users/batch 单事务原子接口。
   */
  batchUpdate: async ({ request, cookies }) => {
    const form = await request.formData();
    const ids = parseBatchIds(form);
    const status = String(form.get('status') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim();
    if (!reason) return fail(422, { message: '操作原因必填（写审计）' });
    if (!['pending', 'active', 'restricted', 'banned'].includes(status)) {
      return fail(422, { message: '无效状态' });
    }
    if (ids.length === 0) {
      const empty = batchResult(emptyBatchSelection(), '批量设置状态');
      return fail(empty.status, { message: empty.message });
    }
    try {
      const result = await authedPost<{ ok: boolean; affected: number; ids: string[] }>(
        cookies,
        '/api/v1/admin/users/batch',
        { ids, status, reason },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { message: `批量更新成功：已设置 ${result.data.affected} 位用户状态为 ${status}` };
      }
      return fail(result.status, { message: result.message });
    } catch {
      return fail(503, { message: '网络错误，批量操作失败' });
    }
  },

  /**
   * 一键随机用户昵称：原违规昵称加入黑名单，生成全新规范随机昵称。
   */
  randomizeNickname: async ({ request, cookies }) => {
    const form = await request.formData();
    const id = String(form.get('id') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim() || '管理员一键随机重置违规昵称';
    if (!id) return fail(422, { message: '缺少用户标识' });
    try {
      const result = await authedPost<{
        ok: boolean;
        user_id: string;
        old_nickname: string;
        new_nickname: string;
        version: number;
      }>(
        cookies,
        `/api/v1/admin/users/${encodeURIComponent(id)}/randomize-nickname`,
        { reason },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return {
          message: `已将「${result.data.old_nickname}」重置为「${result.data.new_nickname}」并加入黑名单`
        };
      }
      if (result.code === 'step_up_required') {
        return fail(403, {
          message: '此操作需要重新验证身份，请输入密码重新验证后重试',
          stepUpRequired: true
        } satisfies AdminUsersActionData);
      }
      if (result.status === 404) {
        return fail(404, { message: '目标用户不存在' } satisfies AdminUsersActionData);
      }
      return fail(result.status, { message: result.message, requestId: result.requestId } satisfies AdminUsersActionData);
    } catch {
      return fail(503, { message: '随机昵称失败，请稍后重试' } satisfies AdminUsersActionData);
    }
  },

  /**
   * 手动添加昵称到黑名单。
   */
  addBlacklist: async ({ request, cookies }) => {
    const form = await request.formData();
    const nickname = String(form.get('nickname') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim() || '管理员手动加入黑名单';
    if (!nickname) return fail(422, { message: '昵称不能为空' });
    try {
      const result = await authedPost<{ ok: boolean; nickname: string; inserted: boolean }>(
        cookies,
        '/api/v1/admin/nickname-blacklist',
        { nickname, reason },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { message: `已将昵称「${nickname}」加入黑名单` };
      }
      if (result.code === 'step_up_required') {
        return fail(403, {
          message: '此操作需要重新验证身份，请输入密码重新验证后重试',
          stepUpRequired: true
        } satisfies AdminUsersActionData);
      }
      return fail(result.status, { message: result.message, requestId: result.requestId } satisfies AdminUsersActionData);
    } catch {
      return fail(503, { message: '添加失败，请稍后重试' } satisfies AdminUsersActionData);
    }
  },

  /**
   * 从黑名单移除条目。
   */
  deleteBlacklist: async ({ request, cookies }) => {
    const form = await request.formData();
    const id = String(form.get('id') ?? '').trim();
    if (!id) return fail(422, { message: '缺少条目标识' });
    try {
      const result = await authedDelete(
        cookies,
        `/api/v1/admin/nickname-blacklist/${encodeURIComponent(id)}`,
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { message: '已从黑名单移除' };
      }
      if (result.code === 'step_up_required') {
        return fail(403, {
          message: '此操作需要重新验证身份，请输入密码重新验证后重试',
          stepUpRequired: true
        } satisfies AdminUsersActionData);
      }
      return fail(result.status, { message: result.message, requestId: result.requestId } satisfies AdminUsersActionData);
    } catch {
      return fail(503, { message: '删除失败，请稍后重试' } satisfies AdminUsersActionData);
    }
  },

  /** 重新验证身份（step-up 窗口过期后；与 storage/roles 页同款交互）。 */
  reauth: async ({ request, cookies }) => {
    const form = await request.formData();
    const password = String(form.get('password') ?? '');
    if (!password) {
      return fail(422, {
        message: '请输入当前密码'
      } satisfies AdminUsersActionData);
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
        } satisfies AdminUsersActionData;
      }
      return fail(result.status, {
        message: result.message
      } satisfies AdminUsersActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, {
        message: '验证失败，请稍后重试'
      } satisfies AdminUsersActionData);
    }
  },

  /** 授予角色：POST /admin/users/{id}/roles {role_name, reason}。 */
  grantRole: async ({ request, cookies }) => {
    const form = await request.formData();
    const userId = String(form.get('user_id') ?? '').trim();
    const roleName = String(form.get('role_name') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim();
    if (!userId || !roleName) {
      return fail(422, { message: '缺少用户或角色' } satisfies AdminUsersActionData);
    }
    if (!reason) {
      return fail(422, { message: '操作原因必填（写入审计日志）' } satisfies AdminUsersActionData);
    }
    try {
      const result = await authedPost<{ roles?: string[] }>(
        cookies,
        `/api/v1/admin/users/${encodeURIComponent(userId)}/roles`,
        { role_name: roleName, reason },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { message: `已成功为用户授予角色「${roleName}」` } satisfies AdminUsersActionData;
      }
      if (result.code === 'step_up_required') {
        return fail(403, {
          message: '此操作需要重新验证身份，请输入密码重新验证后重试',
          stepUpRequired: true
        } satisfies AdminUsersActionData);
      }
      return fail(result.status, { message: result.message } satisfies AdminUsersActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '授予失败，请稍后重试' } satisfies AdminUsersActionData);
    }
  },

  /** 撤销角色：DELETE /admin/users/{id}/roles/{role_name}（body {reason}）。 */
  revokeRole: async ({ request, cookies }) => {
    const form = await request.formData();
    const userId = String(form.get('user_id') ?? '').trim();
    const roleName = String(form.get('role_name') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim();
    if (!userId || !roleName) {
      return fail(422, { message: '缺少用户或角色' } satisfies AdminUsersActionData);
    }
    if (!reason) {
      return fail(422, { message: '操作原因必填（写入审计日志）' } satisfies AdminUsersActionData);
    }
    try {
      const result = await authedDeleteBody<{ roles?: string[] }>(
        cookies,
        `/api/v1/admin/users/${encodeURIComponent(userId)}/roles/${encodeURIComponent(roleName)}`,
        { reason },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { message: `已成功撤销用户的角色「${roleName}」` } satisfies AdminUsersActionData;
      }
      if (result.code === 'step_up_required') {
        return fail(403, {
          message: '此操作需要重新验证身份，请输入密码重新验证后重试',
          stepUpRequired: true
        } satisfies AdminUsersActionData);
      }
      return fail(result.status, { message: result.message } satisfies AdminUsersActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '撤销失败，请稍后重试' } satisfies AdminUsersActionData);
    }
  }
};
