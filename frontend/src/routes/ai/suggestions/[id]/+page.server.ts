// M09-UI-04/05：AI 建议详情（/ai/suggestions/[id]）。
//
// - load：GET /api/v1/ai/suggestions/{id}（本人任务生成的建议；moderation 只对
//   有目标审核权限者可见）；401 → 登录；403 → 无权限态；404 → 安全态；
// - accept action：POST /api/v1/ai/suggestions/{id}/accept，字段级采纳
//   （expected_base_version/If-Match；409 version_conflict → 冲突态提示重载）；
// - moderation 建议：只渲染公开合规摘要，内部 Prompt/举报信号由后端隐去，
//   前端绝不渲染任何内部字段（M09-UI-05）。
import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPost, getAuthed } from '$lib/api/server';
import type { AiSuggestion, AiSuggestionField, AiSuggestionType } from '$lib/api/types';

export interface AiSuggestionPageData {
  suggestion: AiSuggestion | null;
  forbidden: boolean;
  notFound: boolean;
  error: string | null;
}

export interface AiSuggestionActionData {
  ok?: boolean;
  conflict?: boolean;
  message?: string;
  requestId?: string | null;
}

function pickType(raw: unknown): AiSuggestionType {
  switch (raw) {
    case 'formatting':
    case 'seo':
    case 'tagging':
    case 'moderation':
      return raw;
    default:
      return 'formatting';
  }
}

/** 建议投影白名单：任何内部 Prompt/举报/证据字段不进 SSR HTML。 */
function pickSuggestion(raw: unknown): AiSuggestion {
  const r = (raw ?? {}) as Record<string, unknown>;
  const type = pickType(r.type);
  const out: AiSuggestion = {
    id: typeof r.id === 'string' ? r.id : '',
    type,
    status: pickStatus(r.status),
    base_version: typeof r.base_version === 'number' ? r.base_version : 1,
    fields: [],
    created_at: typeof r.created_at === 'number' ? r.created_at : 0
  };
  if (typeof r.target_id === 'string') out.target_id = r.target_id;
  if (typeof r.diff === 'string') out.diff = r.diff;
  if (typeof r.policy_version === 'number') out.policy_version = r.policy_version;
  if (Array.isArray(r.fields)) {
    out.fields = r.fields
      .map((f): AiSuggestionField | null => pickField(f))
      .filter((f): f is AiSuggestionField => f !== null);
  }
  if (r.moderation && typeof r.moderation === 'object') {
    const m = r.moderation as Record<string, unknown>;
    out.moderation = {
      target_type: typeof m.target_type === 'string' ? m.target_type : 'post',
      summary: typeof m.summary === 'string' ? m.summary : null
    };
  }
  return out;
}

function pickField(raw: unknown): AiSuggestionField | null {
  if (!raw || typeof raw !== 'object') return null;
  const f = raw as Record<string, unknown>;
  if (typeof f.field !== 'string' || typeof f.proposed !== 'string') return null;
  const out: AiSuggestionField = { field: f.field, proposed: f.proposed };
  if (typeof f.current === 'string') out.current = f.current;
  if (typeof f.reason === 'string') out.reason = f.reason;
  if (typeof f.selectable === 'boolean') out.selectable = f.selectable;
  return out;
}

function pickStatus(raw: unknown): AiSuggestion['status'] {
  switch (raw) {
    case 'pending':
    case 'accepted':
    case 'rejected':
    case 'expired':
    case 'superseded':
      return raw;
    default:
      return 'pending';
  }
}

export const load: PageServerLoad = async ({ params, cookies, request }): Promise<AiSuggestionPageData> => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<unknown>(
    cookies,
    `/api/v1/ai/suggestions/${encodeURIComponent(params.id)}`,
    requestId
  );
  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    if (result.status === 403) {
      return { suggestion: null, forbidden: true, notFound: false, error: result.message } satisfies AiSuggestionPageData;
    }
    if (result.status === 404) {
      return { suggestion: null, forbidden: false, notFound: true, error: null } satisfies AiSuggestionPageData;
    }
    return { suggestion: null, forbidden: false, notFound: false, error: result.message } satisfies AiSuggestionPageData;
  }
  return { suggestion: pickSuggestion(result.data), forbidden: false, notFound: false, error: null } satisfies AiSuggestionPageData;
};

export const actions: Actions = {
  accept: async ({ params, request, cookies }) => {
    const form = await request.formData();
    const baseVersion = Number(form.get('expected_base_version') ?? 0);
    const fieldRaw = String(form.get('selected_field') ?? '').trim();
    if (!Number.isInteger(baseVersion) || baseVersion < 1) {
      return fail(422, { message: '建议版本缺失或无效，请刷新后重试' } satisfies AiSuggestionActionData);
    }
    const selected_fields = fieldRaw ? [fieldRaw] : [];
    try {
      const result = await authedPost<{ ok?: boolean }>(
        cookies,
        `/api/v1/ai/suggestions/${encodeURIComponent(params.id)}/accept`,
        { expected_base_version: baseVersion, selected_fields },
        request.headers.get('x-request-id'),
        { 'If-Match': String(baseVersion), 'Idempotency-Key': `ai-accept-${params.id}-${baseVersion}` }
      );
      if (result.ok) {
        return { ok: true, message: '已采纳' } satisfies AiSuggestionActionData;
      }
      if (result.status === 409) {
        return fail(409, {
          conflict: true,
          message: '内容已更新，建议已过期。加载最新建议后再采纳。',
          requestId: result.requestId
        } satisfies AiSuggestionActionData);
      }
      return fail(result.status, {
        message: result.message,
        requestId: result.requestId
      } satisfies AiSuggestionActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '采纳失败，请稍后重试' } satisfies AiSuggestionActionData);
    }
  }
};
