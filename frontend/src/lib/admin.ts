// M03-UI-07：管理后台共享状态机——所有授权与可用性裁决来自后端响应，
// 前端只渲染后端决定（401→登录、403→无权限、501→未实现、5xx→错误）。
import type { ServerWriteFailure } from '$lib/api/server';

export type AdminLoadState<T> =
  | { state: 'ok'; items: T[] }
  | { state: 'unauthenticated' }
  | { state: 'forbidden'; message: string }
  | { state: 'not_implemented'; message: string }
  | { state: 'error'; message: string };

/** 把 getAuthed 结果映射为管理页状态（401 由 load 层先行跳转处理）。 */
export function adminListState<T>(
  result: { ok: true; data: { items: T[] } } | ServerWriteFailure
): AdminLoadState<T> {
  if (result.ok) return { state: 'ok', items: result.data.items };
  switch (result.status) {
    case 403:
      return { state: 'forbidden', message: result.message };
    case 501:
      return { state: 'not_implemented', message: result.message };
    default:
      return { state: 'error', message: result.message };
  }
}

/** 同 adminListState，但列表键可指定——部分管理端点返回
 *  { products: [] } / { orders: [] } 等具名数组而非 { items: [] }
 * （视觉检测发现 /admin/shop SSR 500 的根因，M17-GAPFIX-06）。 */
export function adminListStateKeyed<T>(
  result: { ok: true; data: Record<string, unknown> } | ServerWriteFailure,
  key: string
): AdminLoadState<T> {
  if (result.ok) {
    const list = result.data[key];
    return { state: 'ok', items: (Array.isArray(list) ? list : []) as T[] };
  }
  switch (result.status) {
    case 403:
      return { state: 'forbidden', message: result.message };
    case 501:
      return { state: 'not_implemented', message: result.message };
    default:
      return { state: 'error', message: result.message };
  }
}

/** 管理角色行投影（GET /api/v1/admin/roles 契约；M13-ADMIN 落地）。 */
export interface AdminRoleItem {
  id: string;
  name: string;
  scope?: string;
  permissions?: string[];
  /** system 角色权限不可改（后端 PATCH 拒绝）。 */
  is_system?: boolean;
  /** 乐观锁版本 = roles.updated_at。 */
  updated_at?: number;
}

/** 管理页通用状态渲染文案（页面组件复用）。 */
export function adminStateLabel(state: AdminLoadState<never>['state']): string {
  switch (state) {
    case 'forbidden':
      return '无权限：该操作仅限管理员';
    case 'not_implemented':
      return '该管理功能尚未就绪';
    case 'unauthenticated':
      return '请先登录';
    case 'error':
      return '服务暂时不可用';
    default:
      return '';
  }
}
