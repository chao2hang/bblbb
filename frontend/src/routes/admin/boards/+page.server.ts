// M03-UI-07：管理板块页——列表（后端裁决）+ 新建板块表单 + 编辑/置顶
// （PATCH /admin/boards/{id}，If-Match 版本 + reason 审计；视觉对齐
// M17-GAPFIX-06：补原型「可见性/发帖策略/状态」列与「编辑/置顶」行操作）。
// M18-ADMIN-DIALOG：新增批量 ?/batchUpdate——后端已有板块级单条写端点
// PATCH /api/v1/admin/boards/{id}（backend/src/routes/admin.rs update_admin_board），
// 循环调用之（is_active/reason + If-Match）实现批量启用/停用。
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
import type { Board } from '$lib/api/types';

export interface AdminBoardsPageData {
  loadState: AdminLoadState<Board>;
  created?: boolean;
  updated?: boolean;
  message?: string;
  requestId?: string | null;
}

/** 重取板块列表（action 失败/成功后回填，避免类型分叉与陈旧列表）。 */
async function reloadBoards(cookies: Cookies, requestId: string | null): Promise<AdminLoadState<Board>> {
  try {
    const result = await getAuthed<{ items: Board[] }>(cookies, '/api/v1/admin/boards', requestId);
    if (!result) return { state: 'ok', items: [] };
    return adminListState(result);
  } catch {
    return { state: 'ok', items: [] };
  }
}

export const load: PageServerLoad = async ({ cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<{ items: Board[] }>(cookies, '/api/v1/admin/boards', requestId);
  if (!result.ok && result.status === 401) throw redirect(303, '/login');
  return { loadState: adminListState(result) } satisfies AdminBoardsPageData;
};

export const actions: Actions = {
  create: async ({ request, cookies }) => {
    const form = await request.formData();
    const reason = String(form.get('reason') ?? '').trim();
    const name = String(form.get('name') ?? '').trim();
    const slug = String(form.get('slug') ?? '').trim();
    if (!reason || !name || !slug) {
      return fail(422, {
        loadState: await reloadBoards(cookies, null),
        message: '名称、slug 与操作原因均必填'
      } satisfies AdminBoardsPageData);
    }
    try {
      const result = await authedPost<unknown>(
        cookies,
        '/api/v1/admin/boards',
        {
          name,
          slug,
          description: String(form.get('description') ?? '').trim() || null,
          // 板块图标（0071）：空 = 未设置（后端存 NULL）；非空 = lucide 图标名。
          icon: String(form.get('icon') ?? '').trim() || null,
          visibility: String(form.get('visibility') ?? 'public'),
          posting_mode: String(form.get('posting_mode') ?? 'normal'),
          reason
        },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return {
          loadState: await reloadBoards(cookies, request.headers.get('x-request-id')),
          created: true,
          message: '板块已成功创建'
        } satisfies AdminBoardsPageData;
      }
      if (result.status === 403) {
        return fail(403, {
          loadState: await reloadBoards(cookies, null),
          message: result.message
        } satisfies AdminBoardsPageData);
      }
      return fail(result.status, {
        loadState: await reloadBoards(cookies, null),
        message: result.message
      } satisfies AdminBoardsPageData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, {
        loadState: await reloadBoards(cookies, null),
        message: '保存失败，请稍后重试'
      } satisfies AdminBoardsPageData);
    }
  },

  /** 编辑/置顶（sort_order=0）：PATCH /admin/boards/{id}，If-Match 乐观锁。 */
  update: async ({ request, cookies }) => {
    const form = await request.formData();
    const reason = String(form.get('reason') ?? '').trim();
    const id = String(form.get('id') ?? '').trim();
    const version = Number(form.get('version') ?? 0);
    if (!id) return fail(422, { loadState: await reloadBoards(cookies, null), message: '缺少板块标识' } satisfies AdminBoardsPageData);
    if (!Number.isInteger(version) || version < 1) {
      return fail(409, { loadState: await reloadBoards(cookies, null), message: '版本缺失或无效，请刷新后重试' } satisfies AdminBoardsPageData);
    }
    if (!reason) return fail(422, { loadState: await reloadBoards(cookies, null), message: '操作原因必填（写入审计日志）' } satisfies AdminBoardsPageData);

    // 仅携带出现且非空的字段（PATCH 缺省 = 保持原值）。
    const body: Record<string, unknown> = { reason };
    const name = String(form.get('name') ?? '').trim();
    if (name) body.name = name;
    const description = String(form.get('description') ?? '').trim();
    if (description) body.description = description;
    // 板块图标（0071）：编辑表单恒含 icon 控件——
    // 非空 = 设置/替换；空串 = 清除（后端置 NULL）。幂等：未改动时重发原值。
    if (form.has('icon')) body.icon = String(form.get('icon') ?? '').trim();
    const visibility = String(form.get('visibility') ?? '').trim();
    if (visibility) body.visibility = visibility;
    const postingMode = String(form.get('posting_mode') ?? '').trim();
    if (postingMode) body.posting_mode = postingMode;
    const sortOrderRaw = String(form.get('sort_order') ?? '').trim();
    if (sortOrderRaw !== '') {
      const sortOrder = Number(sortOrderRaw);
      if (!Number.isInteger(sortOrder) || sortOrder < 0) {
        return fail(422, { loadState: await reloadBoards(cookies, null), message: '排序需为非负整数（置顶填 0）' } satisfies AdminBoardsPageData);
      }
      body.sort_order = sortOrder;
    }
    const isActiveRaw = String(form.get('is_active') ?? '').trim();
    if (isActiveRaw === 'on' || isActiveRaw === 'true') body.is_active = true;
    if (isActiveRaw === 'false') body.is_active = false;

    try {
      const result = await authedPatch<unknown>(
        cookies,
        `/api/v1/admin/boards/${encodeURIComponent(id)}`,
        body,
        { 'If-Match': String(version) },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { loadState: await reloadBoards(cookies, request.headers.get('x-request-id')), message: '板块已更新' } satisfies AdminBoardsPageData;
      }
      if (result.status === 409) {
        return fail(409, { loadState: await reloadBoards(cookies, request.headers.get('x-request-id')), message: `版本冲突：${result.message}，请刷新后重试` } satisfies AdminBoardsPageData);
      }
      return fail(result.status, { loadState: await reloadBoards(cookies, request.headers.get('x-request-id')), message: result.message, requestId: result.requestId } satisfies AdminBoardsPageData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { loadState: await reloadBoards(cookies, null), message: '保存失败，请稍后重试' } satisfies AdminBoardsPageData);
    }
  },

  /**
   * 批量启用/停用（M18-ADMIN-DIALOG）：循环既有单条端点
   * PATCH /api/v1/admin/boards/{id}（is_active/reason + If-Match，与 ?/update
   * 完全一致），versions 与 ids 顺序一一对应；逐条 try/catch 汇总成败。
   */
  batchUpdate: async ({ request, cookies }) => {
    const form = await request.formData();
    const entries = parseBatchEntries(form);
    const reason = String(form.get('reason') ?? '').trim();
    const nextActive = String(form.get('is_active') ?? '') === 'true';
    const label = nextActive ? '批量启用' : '批量停用';
    if (!reason) {
      return fail(422, { loadState: await reloadBoards(cookies, null), message: '操作原因必填（写入审计日志）' } satisfies AdminBoardsPageData);
    }
    if (entries.length === 0) {
      const empty = batchResult(emptyBatchSelection(), label);
      return fail(empty.status, { loadState: await reloadBoards(cookies, null), message: empty.message } satisfies AdminBoardsPageData);
    }
    const outcome: BatchOutcome = { okCount: 0, failures: [] };
    for (const entry of entries) {
      try {
        const result = await authedPatch<unknown>(
          cookies,
          `/api/v1/admin/boards/${encodeURIComponent(entry.id)}`,
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
    const loadState = await reloadBoards(cookies, request.headers.get('x-request-id'));
    return summary.ok
      ? { loadState, message: summary.message } satisfies AdminBoardsPageData
      : fail(summary.status, { loadState, message: summary.message } satisfies AdminBoardsPageData);
  }
};
