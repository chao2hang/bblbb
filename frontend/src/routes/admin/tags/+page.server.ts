// M03-UI-07：管理标签页——列表（后端裁决）+ 新建/编辑标签表单。
// 标签管理完整 CRUD：create（POST）/ update（PATCH 重命名与描述，If-Match）/
// toggle（PATCH 启停）/ merge（POST 合并）。
// M18-ADMIN-DIALOG：新增批量 ?/batchToggle——循环既有启停端点
// PATCH /api/v1/admin/tags/{id}（is_active/reason + If-Match）。
import { fail, isRedirect, redirect, type Cookies } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPatch, authedPost, getAuthed } from '$lib/api/server';
import { adminListState, type AdminLoadState } from '$lib/admin';
import {
  batchResult,
  emptyBatchSelection,
  parseBatchEntries,
  type BatchOutcome
} from '$lib/admin-batch';
import type { Tag } from '$lib/api/types';

/** 管理端标签行（Tag + 0061 生命周期与版本字段）。 */
export interface AdminTagItem extends Tag {
  is_active?: number | boolean;
  status?: string | null;
  updated_at?: number;
}

async function reloadTags(cookies: Cookies, requestId: string | null): Promise<AdminLoadState<AdminTagItem>> {
  const result = await getAuthed<{ items: AdminTagItem[] }>(cookies, '/api/v1/admin/tags', requestId);
  if (!result.ok && result.status === 401) throw redirect(303, '/login');
  return adminListState(result);
}

export interface AdminTagsPageData {
  loadState: AdminLoadState<AdminTagItem>;
  created?: boolean;
  message?: string;
  requestId?: string | null;
}

export interface AdminTagsActionData {
  loadState?: AdminLoadState<AdminTagItem>;
  message?: string | null;
  requestId?: string | null;
  created?: boolean;
}

export const load: PageServerLoad = async ({ cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<{ items: Tag[] }>(cookies, '/api/v1/admin/tags', requestId);
  if (!result.ok && result.status === 401) throw redirect(303, '/login');
  return { loadState: adminListState(result) } satisfies AdminTagsPageData;
};

export const actions: Actions = {
  create: async ({ request, cookies }) => {
    const form = await request.formData();
    const reason = String(form.get('reason') ?? '').trim();
    const name = String(form.get('name') ?? '').trim();
    if (!reason || !name) {
      return fail(422, { message: '名称与操作原因均必填', loadState: { state: 'error', message: '名称与操作原因均必填' } } satisfies AdminTagsActionData);
    }
    try {
      const result = await authedPost<unknown>(
        cookies,
        '/api/v1/admin/tags',
        { name, reason },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { message: '标签创建成功', loadState: await reloadTags(cookies, request.headers.get('x-request-id')), created: true } satisfies AdminTagsActionData;
      }
      if (result.status === 403) {
        return fail(403, { message: result.message, loadState: { state: 'forbidden', message: result.message } } satisfies AdminTagsActionData);
      }
      return fail(result.status, { message: result.message, loadState: { state: 'error', message: result.message } } satisfies AdminTagsActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '保存失败，请稍后重试', loadState: { state: 'error', message: '保存失败，请稍后重试' } } satisfies AdminTagsActionData);
    }
  },

  /** 重命名/描述编辑：PATCH /admin/tags/{id} {name?, description?}，If-Match。 */
  update: async ({ request, cookies }) => {
    const form = await request.formData();
    const id = String(form.get('id') ?? '').trim();
    const version = Number(form.get('version') ?? 0);
    const name = String(form.get('name') ?? '').trim();
    const description = String(form.get('description') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim();
    if (!id) return fail(422, { message: '缺少标签标识' } satisfies AdminTagsActionData);
    if (!Number.isInteger(version) || version < 1) {
      return fail(409, { message: '版本缺失或无效，请刷新后重试' } satisfies AdminTagsActionData);
    }
    if (!name) return fail(422, { message: '标签名称必填' } satisfies AdminTagsActionData);
    if (!reason) return fail(422, { message: '操作原因必填（写审计）' } satisfies AdminTagsActionData);
    try {
      const result = await authedPatch<unknown>(
        cookies,
        `/api/v1/admin/tags/${encodeURIComponent(id)}`,
        // description 传空串即清空（后端 None = 未提供保持不变）。
        { name, description, reason },
        { 'If-Match': String(version) },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { loadState: await reloadTags(cookies, request.headers.get('x-request-id')), message: '标签已更新' } satisfies AdminTagsActionData;
      }
      if (result.status === 409) {
        return fail(409, { loadState: await reloadTags(cookies, request.headers.get('x-request-id')), message: `版本冲突：${result.message}，请刷新后重试` } satisfies AdminTagsActionData);
      }
      return fail(result.status, { loadState: await reloadTags(cookies, request.headers.get('x-request-id')), message: result.message } satisfies AdminTagsActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '更新失败，请稍后重试' } satisfies AdminTagsActionData);
    }
  },

  /** 停用/启用（M17-GAPFIX-07）：PATCH /admin/tags/{id} {is_active}，If-Match。 */
  toggle: async ({ request, cookies }) => {
    const form = await request.formData();
    const id = String(form.get('id') ?? '').trim();
    const version = Number(form.get('version') ?? 0);
    const reason = String(form.get('reason') ?? '').trim();
    const nextActive = String(form.get('is_active') ?? '') === 'true';
    if (!id) return fail(422, { message: '缺少标签标识' } satisfies AdminTagsActionData);
    if (!Number.isInteger(version) || version < 1) {
      return fail(409, { message: '版本缺失或无效，请刷新后重试' } satisfies AdminTagsActionData);
    }
    if (!reason) return fail(422, { message: '操作原因必填（写审计）' } satisfies AdminTagsActionData);
    try {
      const result = await authedPatch<unknown>(
        cookies,
        `/api/v1/admin/tags/${encodeURIComponent(id)}`,
        { is_active: nextActive, reason },
        { 'If-Match': String(version) },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { loadState: await reloadTags(cookies, request.headers.get('x-request-id')), message: `标签已${nextActive ? '启用' : '停用'}` } satisfies AdminTagsActionData;
      }
      if (result.status === 409) {
        return fail(409, { loadState: await reloadTags(cookies, request.headers.get('x-request-id')), message: `版本冲突：${result.message}，请刷新后重试` } satisfies AdminTagsActionData);
      }
      return fail(result.status, { loadState: await reloadTags(cookies, request.headers.get('x-request-id')), message: result.message } satisfies AdminTagsActionData);
    } catch {
      return fail(503, { message: '操作失败，请稍后重试' } satisfies AdminTagsActionData);
    }
  },

  /** 合并（M17-GAPFIX-07）：POST /admin/tags/{id}/merge {target_id, reason}
   * ——源关联转移至目标、源标记 merged 并停用。 */
  merge: async ({ request, cookies }) => {
    const form = await request.formData();
    const id = String(form.get('id') ?? '').trim();
    const targetId = String(form.get('target_id') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim();
    if (!id) return fail(422, { message: '缺少源标签标识' } satisfies AdminTagsActionData);
    if (!targetId) return fail(422, { message: '请选择并入的目标标签' } satisfies AdminTagsActionData);
    if (targetId === id) return fail(422, { message: '目标标签不能与源标签相同' } satisfies AdminTagsActionData);
    if (!reason) return fail(422, { message: '操作原因必填（写审计）' } satisfies AdminTagsActionData);
    try {
      const result = await authedPost<unknown>(
        cookies,
        `/api/v1/admin/tags/${encodeURIComponent(id)}/merge`,
        { target_id: targetId, reason },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { loadState: await reloadTags(cookies, request.headers.get('x-request-id')), message: '标签已合并（关联已转移，源标签停用）' } satisfies AdminTagsActionData;
      }
      return fail(result.status, { loadState: await reloadTags(cookies, request.headers.get('x-request-id')), message: result.message } satisfies AdminTagsActionData);
    } catch {
      return fail(503, { message: '合并失败，请稍后重试' } satisfies AdminTagsActionData);
    }
  },

  /**
   * 批量启用/停用（M18-ADMIN-DIALOG）：循环既有单条启停端点
   * PATCH /api/v1/admin/tags/{id}（is_active/reason + If-Match，与 ?/toggle
   * 完全一致），versions 与 ids 顺序一一对应；逐条 try/catch 汇总成败。
   */
  batchToggle: async ({ request, cookies }) => {
    const form = await request.formData();
    const entries = parseBatchEntries(form);
    const reason = String(form.get('reason') ?? '').trim();
    const nextActive = String(form.get('is_active') ?? '') === 'true';
    const label = nextActive ? '批量启用' : '批量停用';
    if (!reason) return fail(422, { message: '操作原因必填（写审计）' } satisfies AdminTagsActionData);
    if (entries.length === 0) {
      const empty = batchResult(emptyBatchSelection(), label);
      return fail(empty.status, { message: empty.message } satisfies AdminTagsActionData);
    }
    const outcome: BatchOutcome = { okCount: 0, failures: [] };
    for (const entry of entries) {
      try {
        const result = await authedPatch<unknown>(
          cookies,
          `/api/v1/admin/tags/${encodeURIComponent(entry.id)}`,
          { is_active: nextActive, reason },
          entry.version ? { 'If-Match': entry.version } : {},
          request.headers.get('x-request-id')
        );
        if (result.ok) {
          outcome.okCount++;
        } else if (result.status === 409) {
          outcome.failures.push({ id: entry.id, message: `版本冲突：${result.message}` });
        } else {
          outcome.failures.push({ id: entry.id, message: result.message });
        }
      } catch {
        outcome.failures.push({ id: entry.id, message: '网络错误' });
      }
    }
    const summary = batchResult(outcome, label);
    const loadState = await reloadTags(cookies, request.headers.get('x-request-id'));
    return summary.ok
      ? { loadState, message: summary.message } satisfies AdminTagsActionData
      : fail(summary.status, { loadState, message: summary.message } satisfies AdminTagsActionData);
  }
};
