// M05-UI-03：版主案件队列——列表 + 状态筛选。
// M18-ADMIN-BATCH：新增批量状态操作（列表页原「标记处理中/批量关闭/批量驳回」
// 死按钮接通）——全部循环调用与案件详情页 transition action 完全相同的既有端点
// PATCH /api/v1/admin/moderation/cases/{id}（body {status, resolution}；
// resolution 由后端写入审计 AuditEntry，关闭/驳回时承载必填原因）。
import { fail, redirect, type Cookies } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPatch, getAuthed } from '$lib/api/server';
import { parseBatchIds, batchResult, type BatchOutcome } from '$lib/admin-batch';
import type { ModerationCaseItem } from '$lib/api/types';

export interface CasesPageData {
  items: ModerationCaseItem[];
  forbidden?: boolean;
  error?: string;
}

export const load: PageServerLoad = async (
  { url, cookies, request }
): Promise<CasesPageData> => {
  const requestId = request.headers.get('x-request-id');
  const status = url.searchParams.get('status') ?? '';
  const path = status ? `/api/v1/admin/moderation/cases?status=${encodeURIComponent(status)}` : '/api/v1/admin/moderation/cases';
  const result = await getAuthed<{ items: ModerationCaseItem[] }>(cookies, path, requestId);
  if (!result.ok && result.status === 401) throw redirect(303, '/login');
  if (!result.ok && result.status === 403) return { items: [], forbidden: true, error: result.message } satisfies CasesPageData;
  if (!result.ok) return { items: [], error: result.message } satisfies CasesPageData;
  return { items: result.data.items } satisfies CasesPageData;
};

/** 案件状态迁移端点允许的目标状态（与后端 CaseStatus 值域一致）。 */
const CASE_STATUSES = ['open', 'triaged', 'investigating', 'resolved', 'rejected', 'reopened'] as const;

/** 循环执行单条状态迁移（端点/方法/权限与详情页 transition action 完全一致）。 */
async function transitionEach(
  cookies: Cookies,
  ids: string[],
  status: string,
  resolution: string | null,
  requestId: string | null
): Promise<BatchOutcome> {
  const outcome: BatchOutcome = { okCount: 0, failures: [] };
  for (const id of ids) {
    try {
      const r = await authedPatch<{ id: string; status: string }>(
        cookies,
        `/api/v1/admin/moderation/cases/${encodeURIComponent(id)}`,
        { status, resolution },
        {},
        requestId
      );
      if (r.ok) outcome.okCount++;
      else outcome.failures.push({ id, message: r.message });
    } catch {
      outcome.failures.push({ id, message: '网络错误' });
    }
  }
  return outcome;
}

export const actions: Actions = {
  /** 批量标记处理中（设置状态：triaged / investigating；端点不要求原因）。 */
  batchStatus: async ({ request, cookies }) => {
    const form = await request.formData();
    const ids = parseBatchIds(form);
    const status = String(form.get('status') ?? '').trim();
    if (!ids.length) return fail(422, { message: '未选择任何案件' });
    if (!(CASE_STATUSES as readonly string[]).includes(status)) {
      return fail(422, { message: '缺少合法的目标状态' });
    }
    const outcome = await transitionEach(cookies, ids, status, null, request.headers.get('x-request-id'));
    const r = batchResult(outcome, '批量标记处理中');
    return r.ok ? { message: r.message } : fail(r.status, { message: r.message });
  },
  /** 批量关闭（resolved；原因必填，作为 resolution 存档并写审计）。 */
  batchClose: async ({ request, cookies }) => {
    const form = await request.formData();
    const ids = parseBatchIds(form);
    const reason = String(form.get('reason') ?? '').trim();
    if (!ids.length) return fail(422, { message: '未选择任何案件' });
    if (!reason) return fail(422, { message: '关闭原因必填（写审计）' });
    const outcome = await transitionEach(cookies, ids, 'resolved', reason, request.headers.get('x-request-id'));
    const r = batchResult(outcome, '批量关闭案件');
    return r.ok ? { message: r.message } : fail(r.status, { message: r.message });
  },
  /** 批量驳回（rejected；原因必填，作为 resolution 存档并写审计）。 */
  batchReject: async ({ request, cookies }) => {
    const form = await request.formData();
    const ids = parseBatchIds(form);
    const reason = String(form.get('reason') ?? '').trim();
    if (!ids.length) return fail(422, { message: '未选择任何案件' });
    if (!reason) return fail(422, { message: '驳回原因必填（写审计）' });
    const outcome = await transitionEach(cookies, ids, 'rejected', reason, request.headers.get('x-request-id'));
    const r = batchResult(outcome, '批量驳回案件');
    return r.ok ? { message: r.message } : fail(r.status, { message: r.message });
  }
};
