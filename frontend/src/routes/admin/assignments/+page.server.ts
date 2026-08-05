// M03-UI-07：Assignment 管理页——角色列表作为可授予角色锚点（后端裁决）。
// 板块角色 assignment（board_role_assignments，M03-AUTHZ-02/03）的管理端点
// 由 M13-ADMIN 提供；本页先展示后端返回的角色集与契约说明。
import { redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { getAuthed } from '$lib/api/server';
import { adminListState, type AdminLoadState, type AdminRoleItem } from '$lib/admin';

export interface AdminAssignmentsPageData {
  loadState: AdminLoadState<AdminRoleItem>;
}

export const load: PageServerLoad = async ({ cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<{ items: AdminRoleItem[] }>(cookies, '/api/v1/admin/roles', requestId);
  if (!result.ok && result.status === 401) throw redirect(303, '/login');
  return { loadState: adminListState(result) } satisfies AdminAssignmentsPageData;
};
