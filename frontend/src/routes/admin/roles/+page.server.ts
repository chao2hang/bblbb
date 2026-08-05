// M03-UI-07：管理角色页——角色列表（后端裁决，role.manage 权限门）。
import { redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { getAuthed } from '$lib/api/server';
import { adminListState, type AdminLoadState, type AdminRoleItem } from '$lib/admin';

export interface AdminRolesPageData {
  loadState: AdminLoadState<AdminRoleItem>;
}

export const load: PageServerLoad = async ({ cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<{ items: AdminRoleItem[] }>(cookies, '/api/v1/admin/roles', requestId);
  if (!result.ok && result.status === 401) throw redirect(303, '/login');
  return { loadState: adminListState(result) } satisfies AdminRolesPageData;
};
