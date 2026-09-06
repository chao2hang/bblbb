// M03-UI-07 + GAP-FIX（视觉对齐 M17-GAPFIX-07）：角色与权限——
// 角色卡 + 权限复选网格；保存走既有 PATCH /admin/roles/{id}
// （If-Match=updated_at 乐观锁 + reason + recent-auth；system 角色
// 权限不可改，前端对应只读）。all_permissions 为注册表全量目录。
import { fail, redirect, type Cookies } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPatch, getAuthed } from '$lib/api/server';
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
}

export interface AdminRolesActionData {
  loadState: AdminLoadState<AdminRoleItem>;
  message?: string | null;
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
      if (result.status === 409) {
        return fail(409, { loadState: await reloadRoles(cookies, request.headers.get('x-request-id')), message: `版本冲突：${result.message}，请刷新后重试` } satisfies AdminRolesActionData);
      }
      return fail(result.status, { loadState: await reloadRoles(cookies, request.headers.get('x-request-id')), message: result.message } satisfies AdminRolesActionData);
    } catch {
      return fail(503, { loadState: await reloadRoles(cookies, null), message: '保存失败，请稍后重试' } satisfies AdminRolesActionData);
    }
  }
};
