// M09-UI-03：AI 任务状态页（/ai/tasks/[id]）。
//
// - load：GET /api/v1/ai/tasks/{id}（本人任务）；401 → 登录；404/403 → 安全态；
// - cancel action：POST /api/v1/ai/tasks/{id}/cancel（幂等）；
// - 排队/运行/等待重试/成功/取消/失败状态展示；成功时链接到建议详情；
// - 错误只展示稳定码与脱敏信息，绝不回显 Provider 响应原文或 Prompt。
import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPost, getAuthed } from '$lib/api/server';
import { newClientRequestId } from '$lib/api/client';
import type { AiTask } from '$lib/api/types';

export interface AiTaskPageData {
  task: AiTask | null;
  forbidden: boolean;
  notFound: boolean;
  error: string | null;
  /** 取消表单幂等键（SSR 生成，SSR/客户端 hydration 一致）。 */
  clientRequestId: string;
}

export interface AiTaskActionData {
  ok?: boolean;
  message?: string;
  requestId?: string | null;
}

/** 任务投影白名单：只保留展示字段；任何内部 Prompt/Provider 原文不进输出。 */
function pickTask(raw: unknown): AiTask {
  const r = (raw ?? {}) as Record<string, unknown>;
  const out: AiTask = {
    id: typeof r.id === 'string' ? r.id : '',
    task_type: r.task_type === 'seo' || r.task_type === 'tagging' || r.task_type === 'moderation' ? r.task_type : 'formatting',
    status: pickStatus(r.status),
    created_at: typeof r.created_at === 'number' ? r.created_at : 0
  };
  if (typeof r.source_revision === 'number') out.source_revision = r.source_revision;
  if (typeof r.policy_version === 'number') out.policy_version = r.policy_version;
  if (typeof r.target_id === 'string') out.target_id = r.target_id;
  if (typeof r.error_code === 'string') out.error_code = r.error_code;
  if (typeof r.error_message === 'string') out.error_message = r.error_message;
  if (typeof r.suggestion_id === 'string') out.suggestion_id = r.suggestion_id;
  if (typeof r.poll_url === 'string') out.poll_url = r.poll_url;
  if (typeof r.cancel_url === 'string') out.cancel_url = r.cancel_url;
  if (typeof r.started_at === 'number') out.started_at = r.started_at;
  if (typeof r.finished_at === 'number') out.finished_at = r.finished_at;
  return out;
}

function pickStatus(raw: unknown): AiTask['status'] {
  switch (raw) {
    case 'queued':
    case 'running':
    case 'retry_wait':
    case 'succeeded':
    case 'cancelled':
    case 'dead':
      return raw;
    default:
      return 'queued';
  }
}

export const load: PageServerLoad = async ({ params, cookies, request }): Promise<AiTaskPageData> => {
  const requestId = request.headers.get('x-request-id');
  const clientRequestId = newClientRequestId();
  const result = await getAuthed<unknown>(
    cookies,
    `/api/v1/ai/tasks/${encodeURIComponent(params.id)}`,
    requestId
  );
  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    if (result.status === 403) {
      return { task: null, forbidden: true, notFound: false, error: result.message, clientRequestId } satisfies AiTaskPageData;
    }
    if (result.status === 404) {
      return { task: null, forbidden: false, notFound: true, error: null, clientRequestId } satisfies AiTaskPageData;
    }
    return { task: null, forbidden: false, notFound: false, error: result.message, clientRequestId } satisfies AiTaskPageData;
  }
  return { task: pickTask(result.data), forbidden: false, notFound: false, error: null, clientRequestId } satisfies AiTaskPageData;
};

export const actions: Actions = {
  cancel: async ({ params, request, cookies }) => {
    const form = await request.formData();
    const clientRequestId = String(form.get('client_request_id') ?? '').trim();
    if (clientRequestId.length < 16) {
      return fail(422, { message: '请求标识缺失，请刷新页面后重试' } satisfies AiTaskActionData);
    }
    try {
      const result = await authedPost<{ ok?: boolean }>(
        cookies,
        `/api/v1/ai/tasks/${encodeURIComponent(params.id)}/cancel`,
        {},
        request.headers.get('x-request-id'),
        { 'Idempotency-Key': clientRequestId }
      );
      if (result.ok) {
        return { ok: true, message: '已取消任务' } satisfies AiTaskActionData;
      }
      return fail(result.status, {
        message: result.message,
        requestId: result.requestId
      } satisfies AiTaskActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '取消请求失败，请稍后重试' } satisfies AiTaskActionData);
    }
  }
};
