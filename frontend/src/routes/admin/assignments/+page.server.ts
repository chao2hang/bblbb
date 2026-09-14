// 角色委派管理页（角色委派完整 CRUD）：用户搜索 + 角色授予/撤销。
//
// 后端契约（均已实现，本页此前仅为占位）：
// - GET  /api/v1/admin/roles                        —— 可授予角色目录（role.manage）
// - GET  /api/v1/admin/users?q=&limit=              —— 用户搜索（user.manage）
// - GET  /api/v1/admin/users/{id}                   —— 用户投影（含实时 roles）
// - POST /api/v1/admin/users/{id}/roles             —— 授予角色 {role_name, reason}
//   （幂等：重复授予直接返回现有角色集，不重复审计）；
// - DELETE /api/v1/admin/users/{id}/roles/{role}    —— 撤销角色（body {reason}）。
//
// 授权与错误一律来自后端响应：401 → 登录、403 → 无权限、404 → 不存在。
import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedDeleteBody, authedPost, getAuthed } from '$lib/api/server';
import { adminListState, type AdminLoadState, type AdminRoleItem } from '$lib/admin';
import { parseBatchIds, batchResult, type BatchOutcome } from '$lib/admin-batch';

/** 管理用户投影的子集（本页只读这些字段；不含凭据）。 */
export interface AdminAssignmentsUser {
  id: string;
  username: string;
  email: string;
  status: string;
  display_name: string | null;
  level: number;
  roles: string[];
}

export interface AdminAssignmentsPageData {
  /** 可授予角色目录。 */
  loadState: AdminLoadState<AdminRoleItem>;
  /** 当前搜索关键字（GET 表单回填）。 */
  q: string;
  /** 搜索结果（q 非空时返回）。 */
  users: AdminAssignmentsUser[] | null;
  /** 选中用户（?user= 时返回，含实时角色）。 */
  selectedUser: AdminAssignmentsUser | null;
  /** 用户搜索/详情失败信息（角色目录失败走 loadState）。 */
  userError: string | null;
}

/** grant/revoke 动作返回：操作后的用户最新投影 + 提示。 */
export interface AdminAssignmentsActionData {
  user: AdminAssignmentsUser | null;
  message?: string | null;
}

async function fetchUser(
  cookies: Parameters<PageServerLoad>[0]['cookies'],
  requestId: string | null,
  userId: string
): Promise<{ user: AdminAssignmentsUser | null; status: number | null; message: string | null }> {
  const result = await getAuthed<AdminAssignmentsUser>(
    cookies,
    `/api/v1/admin/users/${encodeURIComponent(userId)}`,
    requestId
  );
  if (result.ok) return { user: result.data, status: null, message: null };
  return { user: null, status: result.status, message: result.message };
}

export const load: PageServerLoad = async ({ cookies, request, url }) => {
  const requestId = request.headers.get('x-request-id');
  const q = (url.searchParams.get('q') ?? '').trim();
  const userId = (url.searchParams.get('user') ?? '').trim();

  const rolesResult = await getAuthed<{ items: AdminRoleItem[] }>(
    cookies,
    '/api/v1/admin/roles',
    requestId
  );
  if (!rolesResult.ok && rolesResult.status === 401) throw redirect(303, '/login');
  const loadState = adminListState(rolesResult);

  let users: AdminAssignmentsUser[] | null = null;
  let selectedUser: AdminAssignmentsUser | null = null;
  let userError: string | null = null;

  if (q) {
    const params = new URLSearchParams({ q, limit: '20' });
    const result = await getAuthed<{ items: AdminAssignmentsUser[] }>(
      cookies,
      `/api/v1/admin/users?${params.toString()}`,
      requestId
    );
    if (result.ok) {
      users = result.data.items;
    } else if (result.status === 401) {
      throw redirect(303, '/login');
    } else {
      userError = result.message;
    }
  }

  if (userId) {
    const fetched = await fetchUser(cookies, requestId, userId);
    if (fetched.status === 401) throw redirect(303, '/login');
    if (fetched.user) {
      selectedUser = fetched.user;
    } else {
      userError = fetched.message ?? '用户不存在';
    }
  }

  return { loadState, q, users, selectedUser, userError } satisfies AdminAssignmentsPageData;
};

export const actions: Actions = {
  /** 授予角色：POST /admin/users/{id}/roles {role_name, reason}。 */
  grant: async ({ request, cookies }) => {
    const form = await request.formData();
    const userId = String(form.get('user_id') ?? '').trim();
    const roleName = String(form.get('role_name') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim();
    if (!userId || !roleName) {
      return fail(422, { user: null, message: '缺少用户或角色' } satisfies AdminAssignmentsActionData);
    }
    if (!reason) {
      return fail(422, { user: null, message: '操作原因必填（写入审计日志）' } satisfies AdminAssignmentsActionData);
    }
    try {
      const result = await authedPost<{ roles?: string[] }>(
        cookies,
        `/api/v1/admin/users/${encodeURIComponent(userId)}/roles`,
        { role_name: roleName, reason },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        const fetched = await fetchUser(cookies, request.headers.get('x-request-id'), userId);
        return {
          user: fetched.user,
          message: fetched.user
            ? `已授予角色 ${roleName}（当前 ${fetched.user.roles.length} 个角色）`
            : '角色已授予'
        } satisfies AdminAssignmentsActionData;
      }
      return fail(result.status, { user: null, message: result.message } satisfies AdminAssignmentsActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { user: null, message: '授予失败，请稍后重试' } satisfies AdminAssignmentsActionData);
    }
  },

  /** 撤销角色：DELETE /admin/users/{id}/roles/{role_name}（body {reason}）。 */
  revoke: async ({ request, cookies }) => {
    const form = await request.formData();
    const userId = String(form.get('user_id') ?? '').trim();
    const roleName = String(form.get('role_name') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim();
    if (!userId || !roleName) {
      return fail(422, { user: null, message: '缺少用户或角色' } satisfies AdminAssignmentsActionData);
    }
    if (!reason) {
      return fail(422, { user: null, message: '操作原因必填（写入审计日志）' } satisfies AdminAssignmentsActionData);
    }
    try {
      const result = await authedDeleteBody<{ roles?: string[] }>(
        cookies,
        `/api/v1/admin/users/${encodeURIComponent(userId)}/roles/${encodeURIComponent(roleName)}`,
        { reason },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        const fetched = await fetchUser(cookies, request.headers.get('x-request-id'), userId);
        return {
          user: fetched.user,
          message: fetched.user
            ? `已撤销角色 ${roleName}（剩余 ${fetched.user.roles.length} 个角色）`
            : '角色已撤销'
        } satisfies AdminAssignmentsActionData;
      }
      return fail(result.status, { user: null, message: result.message } satisfies AdminAssignmentsActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { user: null, message: '撤销失败，请稍后重试' } satisfies AdminAssignmentsActionData);
    }
  },

  /**
   * 批量撤销角色（约定 B）：ids = 角色名列表（同一用户），循环调用与单条 revoke
   * 完全相同的端点 DELETE /admin/users/{id}/roles/{role}（body {reason}），
   * 逐条 try/catch 汇总成败；任一成功后回读用户最新投影。
   */
  batchRevoke: async ({ request, cookies }) => {
    const form = await request.formData();
    const userId = String(form.get('user_id') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim();
    if (!userId) {
      return fail(422, { user: null, message: '缺少用户' } satisfies AdminAssignmentsActionData);
    }
    if (!reason) {
      return fail(422, { user: null, message: '操作原因必填（写入审计日志）' } satisfies AdminAssignmentsActionData);
    }
    const ids = parseBatchIds(form);
    if (ids.length === 0) {
      return fail(422, { user: null, message: '未选择任何角色' } satisfies AdminAssignmentsActionData);
    }
    const outcome: BatchOutcome = { okCount: 0, failures: [] };
    for (const roleName of ids) {
      try {
        const result = await authedDeleteBody<{ roles?: string[] }>(
          cookies,
          `/api/v1/admin/users/${encodeURIComponent(userId)}/roles/${encodeURIComponent(roleName)}`,
          { reason },
          request.headers.get('x-request-id')
        );
        if (result.ok) outcome.okCount++;
        else outcome.failures.push({ id: roleName, message: result.message });
      } catch {
        outcome.failures.push({ id: roleName, message: '网络错误' });
      }
    }
    const summary = batchResult(outcome, '批量撤销角色');
    if (!summary.ok) {
      return fail(summary.status, { user: null, message: summary.message } satisfies AdminAssignmentsActionData);
    }
    try {
      const fetched = await fetchUser(cookies, request.headers.get('x-request-id'), userId);
      return {
        user: fetched.user,
        message: `${summary.message}（剩余 ${fetched.user?.roles.length ?? 0} 个角色）`
      } satisfies AdminAssignmentsActionData;
    } catch (e) {
      if (isRedirect(e)) throw e;
      return { user: null, message: summary.message } satisfies AdminAssignmentsActionData;
    }
  }
};
