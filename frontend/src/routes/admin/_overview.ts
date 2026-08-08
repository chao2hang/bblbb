// M13-UI-01：后台概览类页面共享权限门——以真实后端权限（role.manage）裁决，
// 401→登录、403→无权限、5xx→错误。菜单隐藏不是安全边界，服务端始终裁决。
import { redirect } from '@sveltejs/kit';
import type { Cookies } from '@sveltejs/kit';
import { getAuthed } from '$lib/api/server';

export type AdminOverviewState = 'ok' | 'forbidden' | 'not_implemented' | 'error';

export interface AdminOverviewData {
  state: AdminOverviewState;
  error: string | null;
  title: string;
  links: { href: string; label: string; desc: string }[];
}

/**
 * 概览页 load：先用真实端点（roles 列表）做权限门，再返回标题与导航链接。
 * 链接目标是已实现的管理页；本页不携带任何用户/内容数据。
 */
export async function overviewLoad(
  cookies: Cookies,
  requestId: string | null,
  title: string,
  links: { href: string; label: string; desc: string }[]
): Promise<AdminOverviewData> {
  const result = await getAuthed<unknown>(cookies, '/api/v1/admin/roles', requestId);
  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    if (result.status === 403) return { state: 'forbidden', error: result.message, title, links };
    if (result.status === 501) return { state: 'not_implemented', error: result.message, title, links };
    return { state: 'error', error: result.message, title, links };
  }
  return { state: 'ok', error: null, title, links };
}
