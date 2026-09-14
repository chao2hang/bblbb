// M03-UI-07 + GAP-FIX（视觉对齐 M17-GAPFIX-07）：角色与权限——
// 角色卡 + 权限复选网格；保存走既有 PATCH /admin/roles/{id}
// （If-Match=updated_at 乐观锁 + reason + recent-auth；system 角色
// 权限不可改，前端对应只读）。all_permissions 为注册表全量目录。
// 角色委派完整 CRUD：新增「创建角色」POST /admin/roles（自定义角色，
// 权限名必须存在于注册表；recent-auth 403 step_up_required → reauth 弹窗）。
import { fail, isRedirect, redirect, type Cookies } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPatch, authedPost, getAuthed } from '$lib/api/server';
import { adminListState, type AdminLoadState, type AdminRoleItem } from '$lib/admin';

export interface AdminRolesPageData {
  loadState: AdminLoadState<AdminRoleItem>;
  /** 权限目录（注册表全量，升序）。 */
  allPermissions: string[];
}

/** save action 返回投影（allPermissions 为常量，前端从 data 取）。 */
export interface AdminRolesActionData {
  loadState: AdminLoadState<AdminRoleItem>;
  message?: string | null;
  /** 403 step_up_required → 页面展示重新验证（reauth）表单。 */
  stepUpRequired?: boolean;
}

async function reloadRoles(cookies: Cookies, requestId: string | null): Promise<AdminRolesPageData['loadState']> {
  const result = await getAuthed<{ items: AdminRoleItem[] }>(cookies, '/api/v1/admin/roles', requestId);
  if (!result.ok && result.status === 401) throw redirect(303, '/login');
  return adminListState(result);
}

export const load: PageServerLoad = async ({ cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<{ items: AdminRoleItem[]; all_permissions?: string[] }>(
    cookies,
    '/api/v1/admin/roles',
    requestId
  );
  if (!result.ok && result.status === 401) throw redirect(303, '/login');
  const allPermissions = [...(result.ok ? (result.data.all_permissions ?? []) : [])].sort();
  return { loadState: adminListState(result), allPermissions } satisfies AdminRolesPageData;
};

export const actions: Actions = {
  /** 保存角色权限（全量语义；system 角色后端拒绝，前端对应只读）。 */
  save: async ({ request, cookies }) => {
    const form = await request.formData();
    const reason = String(form.get('reason') ?? '').trim();
    const id = String(form.get('id') ?? '').trim();
    const version = Number(form.get('version') ?? 0);
    const permissions = form.getAll('permissions').map(String);
    if (!id) return fail(422, { loadState: await reloadRoles(cookies, null), message: '缺少角色标识' } satisfies AdminRolesActionData);
    if (!Number.isInteger(version) || version < 1) {
      return fail(409, { loadState: await reloadRoles(cookies, null), message: '版本缺失或无效，请刷新后重试' } satisfies AdminRolesActionData);
    }
    if (!reason) {
      return fail(422, { loadState: await reloadRoles(cookies, null), message: '操作原因必填（写入审计日志）' } satisfies AdminRolesActionData);
    }

    try {
      const result = await authedPatch<unknown>(
        cookies,
        `/api/v1/admin/roles/${encodeURIComponent(id)}`,
        { permissions, reason },
        { 'If-Match': String(version) },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { loadState: await reloadRoles(cookies, request.headers.get('x-request-id')), message: '角色权限已保存' } satisfies AdminRolesActionData;
      }
      if (result.code === 'step_up_required') {
        return fail(403, {
          loadState: await reloadRoles(cookies, null),
          message: '此操作需要重新验证身份，请输入密码重新验证后重试',
          stepUpRequired: true
        } satisfies AdminRolesActionData);
      }
      if (result.status === 409) {
        return fail(409, { loadState: await reloadRoles(cookies, request.headers.get('x-request-id')), message: `版本冲突：${result.message}，请刷新后重试` } satisfies AdminRolesActionData);
      }
      return fail(result.status, { loadState: await reloadRoles(cookies, request.headers.get('x-request-id')), message: result.message } satisfies AdminRolesActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { loadState: await reloadRoles(cookies, null), message: '保存失败，请稍后重试' } satisfies AdminRolesActionData);
    }
  },

  /** 创建自定义角色：POST /admin/roles（name 小写字母/数字/下划线；权限≥1）。 */
  create: async ({ request, cookies }) => {
    const form = await request.formData();
    const reason = String(form.get('reason') ?? '').trim();
    const name = String(form.get('name') ?? '').trim();
    const displayName = String(form.get('display_name') ?? '').trim();
    const description = String(form.get('description') ?? '').trim();
    const permissions = form.getAll('permissions').map(String);

    if (!reason) {
      return fail(422, { loadState: await reloadRoles(cookies, null), message: '操作原因必填（写入审计日志）' } satisfies AdminRolesActionData);
    }
    if (!name || !/^[a-z0-9_]{1,64}$/.test(name)) {
      return fail(422, {
        loadState: await reloadRoles(cookies, null),
        message: '角色标识无效：仅允许小写字母、数字与下划线（≤64 字符）'
      } satisfies AdminRolesActionData);
    }
    if (permissions.length === 0) {
      return fail(422, { loadState: await reloadRoles(cookies, null), message: '至少勾选一项权限' } satisfies AdminRolesActionData);
    }

    try {
      const result = await authedPost<unknown>(
        cookies,
        '/api/v1/admin/roles',
        {
          name,
          display_name: displayName || name,
          description: description || undefined,
          permissions,
          reason
        },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return {
          loadState: await reloadRoles(cookies, request.headers.get('x-request-id')),
          message: `角色 ${displayName || name} 已创建`
        } satisfies AdminRolesActionData;
      }
      if (result.code === 'step_up_required') {
        return fail(403, {
          loadState: await reloadRoles(cookies, null),
          message: '此操作需要重新验证身份，请输入密码重新验证后重试',
          stepUpRequired: true
        } satisfies AdminRolesActionData);
      }
      return fail(result.status, { loadState: await reloadRoles(cookies, request.headers.get('x-request-id')), message: result.message } satisfies AdminRolesActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { loadState: await reloadRoles(cookies, null), message: '创建失败，请稍后重试' } satisfies AdminRolesActionData);
    }
  },

  /** 重新验证身份（step-up 窗口过期后；与 storage 页同款交互）。 */
  reauth: async ({ request, cookies }) => {
    const form = await request.formData();
    const password = String(form.get('password') ?? '');
    if (!password) {
      return fail(422, {
        loadState: await reloadRoles(cookies, null),
        message: '请输入当前密码'
      } satisfies AdminRolesActionData);
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
          loadState: await reloadRoles(cookies, request.headers.get('x-request-id')),
          message: '已重新验证身份，请重试刚才的操作'
        } satisfies AdminRolesActionData;
      }
      return fail(result.status, {
        loadState: await reloadRoles(cookies, null),
        message: result.message
      } satisfies AdminRolesActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, {
        loadState: await reloadRoles(cookies, null),
        message: '验证失败，请稍后重试'
      } satisfies AdminRolesActionData);
    }
  }
};
