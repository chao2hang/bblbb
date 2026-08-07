// M09-UI-06：管理端 AI——Provider 脱敏状态、预算、任务重试/取消与 Flag
// 配置。所有写操作要求 reason（审计）；Secret 只写不读。
//
// - load：GET /admin/ai/config（脱敏视图）+ GET /admin/ai/tasks；
// - save action：PATCH /admin/ai/config（If-Match 版本 + reason）；
// - test action：POST /admin/ai/providers/test（固定脱敏探针，不接受正文）；
// - retry/cancel action：POST /admin/ai/tasks/{id}/retry|/cancel；
// - 后端未实现（501）→ not_implemented 态；403 → forbidden；不影响核心论坛。
import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPatch, authedPost, getAuthed } from '$lib/api/server';
import { newClientRequestId } from '$lib/api/client';
import type {
  AiAdminConfig,
  AiAdminProviderConfig,
  AiAdminTaskRow,
  AiProviderTestResult,
  AiTask
} from '$lib/api/types';

export type AdminAiLoadState = 'ok' | 'forbidden' | 'not_implemented' | 'error';

export interface AdminAiPageData {
  state: AdminAiLoadState;
  config: AiAdminConfig | null;
  tasks: AiAdminTaskRow[];
  error: string | null;
  /** 表单幂等键（SSR 生成，hydration 稳定）。 */
  clientRequestId: string;
}

export interface AdminAiActionData {
  ok?: boolean;
  conflict?: boolean;
  message?: string;
  requestId?: string | null;
  testResult?: AiProviderTestResult | null;
}

/** 配置投影白名单：Secret 只保留布尔；任何密钥/内部字段不进 SSR HTML。 */
function pickConfig(raw: unknown): AiAdminConfig | null {
  const r = (raw ?? {}) as Record<string, unknown>;
  if (typeof r !== 'object' || r === null) return null;
  const out: AiAdminConfig = { enabled: r.enabled === true, version: typeof r.version === 'number' ? r.version : 0 };
  if (typeof r.data_mode === 'string') out.data_mode = r.data_mode;
  if (Array.isArray(r.purposes)) {
    out.purposes = r.purposes.filter((p) => typeof p === 'string') as string[];
  }
  if (Array.isArray(r.providers)) {
    out.providers = r.providers
      .map((p): AiAdminProviderConfig | null => pickProvider(p))
      .filter((p): p is AiAdminProviderConfig => p !== null);
  }
  if (r.budgets && typeof r.budgets === 'object') {
    const b = r.budgets as Record<string, unknown>;
    out.budgets = {
      per_user_daily_tokens: typeof b.per_user_daily_tokens === 'number' ? b.per_user_daily_tokens : null,
      per_user_daily_usd: typeof b.per_user_daily_usd === 'number' ? b.per_user_daily_usd : null,
      site_daily_tokens: typeof b.site_daily_tokens === 'number' ? b.site_daily_tokens : null,
      site_daily_usd: typeof b.site_daily_usd === 'number' ? b.site_daily_usd : null
    };
  }
  if (r.flags && typeof r.flags === 'object') {
    const flags: Record<string, boolean> = {};
    for (const [k, v] of Object.entries(r.flags as Record<string, unknown>)) {
      if (typeof v === 'boolean') flags[k] = v;
    }
    if (Object.keys(flags).length > 0) out.flags = flags;
  }
  if (typeof r.ai_crawler_policy === 'string') out.ai_crawler_policy = r.ai_crawler_policy;
  if (typeof r.updated_at === 'number') out.updated_at = r.updated_at;
  return out;
}

function pickProvider(raw: unknown): AiAdminProviderConfig | null {
  if (!raw || typeof raw !== 'object') return null;
  const p = raw as Record<string, unknown>;
  if (typeof p.id !== 'string' || !p.id) return null;
  const out: AiAdminProviderConfig = { id: p.id };
  if (typeof p.name === 'string') out.name = p.name;
  if (typeof p.api_type === 'string') out.api_type = p.api_type;
  if (typeof p.base_url === 'string') out.base_url = p.base_url;
  if (typeof p.model === 'string') out.model = p.model;
  if (typeof p.secret_configured === 'boolean') out.secret_configured = p.secret_configured;
  if (typeof p.available === 'boolean') out.available = p.available;
  if (Array.isArray(p.purposes)) {
    out.purposes = p.purposes.filter((x) => typeof x === 'string') as string[];
  }
  if (typeof p.retention === 'string') out.retention = p.retention;
  if (typeof p.training === 'string') out.training = p.training;
  if (typeof p.region === 'string') out.region = p.region;
  if (typeof p.version === 'number') out.version = p.version;
  return out;
}

function pickTask(raw: unknown): AiAdminTaskRow | null {
  if (!raw || typeof raw !== 'object') return null;
  const r = raw as Record<string, unknown>;
  if (typeof r.id !== 'string' || !r.id) return null;
  const status = (['queued', 'running', 'retry_wait', 'succeeded', 'cancelled', 'dead'] as const).includes(r.status as AiTask['status'])
    ? (r.status as AiTask['status'])
    : 'queued';
  const taskType = (['formatting', 'seo', 'tagging', 'moderation'] as const).includes(r.task_type as AiTask['task_type'])
    ? (r.task_type as AiTask['task_type'])
    : 'formatting';
  const out: AiAdminTaskRow = {
    id: r.id,
    task_type: taskType,
    status,
    created_at: typeof r.created_at === 'number' ? r.created_at : 0
  };
  if (typeof r.user_id === 'string') out.user_id = r.user_id;
  if (typeof r.provider === 'string') out.provider = r.provider;
  if (typeof r.purpose === 'string') out.purpose = r.purpose;
  if (typeof r.error_code === 'string') out.error_code = r.error_code;
  if (typeof r.error_message === 'string') out.error_message = r.error_message;
  if (typeof r.source_revision === 'number') out.source_revision = r.source_revision;
  if (typeof r.finished_at === 'number') out.finished_at = r.finished_at;
  return out;
}

export const load: PageServerLoad = async ({ cookies, request }): Promise<AdminAiPageData> => {
  const requestId = request.headers.get('x-request-id');
  const clientRequestId = newClientRequestId();
  const configResult = await getAuthed<unknown>(cookies, '/api/v1/admin/ai/config', requestId);
  if (!configResult.ok) {
    if (configResult.status === 401) throw redirect(303, '/login');
    if (configResult.status === 403) {
      return { state: 'forbidden', config: null, tasks: [], error: configResult.message, clientRequestId } satisfies AdminAiPageData;
    }
    if (configResult.status === 501) {
      return { state: 'not_implemented', config: null, tasks: [], error: configResult.message, clientRequestId } satisfies AdminAiPageData;
    }
    return { state: 'error', config: null, tasks: [], error: configResult.message, clientRequestId } satisfies AdminAiPageData;
  }
  const config = pickConfig(configResult.data);
  const tasksResult = await getAuthed<{ items?: unknown[] }>(cookies, '/api/v1/admin/ai/tasks', requestId);
  const tasks: AiAdminTaskRow[] = [];
  if (tasksResult.ok && Array.isArray(tasksResult.data?.items)) {
    for (const t of tasksResult.data.items) {
      const picked = pickTask(t);
      if (picked) tasks.push(picked);
    }
  }
  return { state: 'ok', config, tasks, error: null, clientRequestId } satisfies AdminAiPageData;
};

function boolForm(form: FormData, key: string): boolean {
  const raw = form.get(key);
  return raw === 'on' || raw === 'true' || raw === '1';
}

function numOrNull(raw: unknown): number | null | undefined {
  if (raw === null || raw === undefined) return undefined;
  const s = String(raw).trim();
  if (s === '') return undefined;
  const v = Number(s);
  return Number.isFinite(v) ? v : null;
}

/** 从表单构建配置变更（只提交变化字段；Secret 输入不回显）。 */
function buildConfigChanges(form: FormData): Record<string, unknown> {
  const changes: Record<string, unknown> = {};
  changes.enabled = boolForm(form, 'enabled');
  const dataMode = String(form.get('data_mode') ?? '').trim();
  if (dataMode) changes.data_mode = dataMode;
  const flags: Record<string, boolean> = {};
  for (const f of ['formatting', 'seo', 'tagging', 'moderation']) {
    flags[f] = boolForm(form, `flag_${f}`);
  }
  changes.flags = flags;
  const budgets: Record<string, number | null> = {};
  const perUserTokens = numOrNull(form.get('budget_per_user_daily_tokens'));
  const siteTokens = numOrNull(form.get('budget_site_daily_tokens'));
  if (perUserTokens !== undefined) budgets.per_user_daily_tokens = perUserTokens;
  if (siteTokens !== undefined) budgets.site_daily_tokens = siteTokens;
  if (Object.keys(budgets).length > 0) changes.budgets = budgets;
  return changes;
}

export const actions: Actions = {
  save: async ({ request, cookies }) => {
    const form = await request.formData();
    const reason = String(form.get('reason') ?? '').trim();
    const expectedVersion = Number(form.get('expected_version') ?? 0);
    if (!reason) {
      return fail(422, { message: '操作原因必填（写审计）' } satisfies AdminAiActionData);
    }
    if (!Number.isInteger(expectedVersion) || expectedVersion < 1) {
      return fail(422, { message: '配置版本缺失或无效，请刷新后重试' } satisfies AdminAiActionData);
    }
    const changes = buildConfigChanges(form);
    try {
      const result = await authedPatch<AiAdminConfig>(
        cookies,
        '/api/v1/admin/ai/config',
        { ...changes, expected_version: expectedVersion, reason },
        { 'If-Match': String(expectedVersion) },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { ok: true, message: 'AI 配置已保存（写审计）' } satisfies AdminAiActionData;
      }
      if (result.status === 409) {
        return fail(409, { conflict: true, message: `版本冲突：${result.message}` } satisfies AdminAiActionData);
      }
      return fail(result.status, { message: result.message, requestId: result.requestId } satisfies AdminAiActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '保存失败，请稍后重试' } satisfies AdminAiActionData);
    }
  },
  test: async ({ request, cookies }) => {
    const form = await request.formData();
    const candidate: Record<string, unknown> = {
      data_mode: String(form.get('data_mode') ?? '').trim(),
      flags: {
        formatting: boolForm(form, 'flag_formatting'),
        seo: boolForm(form, 'flag_seo'),
        tagging: boolForm(form, 'flag_tagging'),
        moderation: boolForm(form, 'flag_moderation')
      }
    };
    try {
      const result = await authedPost<AiProviderTestResult>(
        cookies,
        '/api/v1/admin/ai/providers/test',
        candidate,
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { testResult: result.data } satisfies AdminAiActionData;
      }
      return fail(result.status, {
        message: result.message,
        requestId: result.requestId,
        testResult: null
      } satisfies AdminAiActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '测试连接失败，请稍后重试' } satisfies AdminAiActionData);
    }
  },
  retry: async ({ request, cookies }) => {
    const form = await request.formData();
    const taskId = String(form.get('task_id') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim();
    if (!taskId) return fail(422, { message: '缺少任务标识' } satisfies AdminAiActionData);
    if (!reason) return fail(422, { message: '操作原因必填（写审计）' } satisfies AdminAiActionData);
    try {
      const result = await authedPost<{ ok?: boolean }>(
        cookies,
        `/api/v1/admin/ai/tasks/${encodeURIComponent(taskId)}/retry`,
        { reason },
        request.headers.get('x-request-id')
      );
      if (result.ok) return { ok: true, message: '任务已重新入队' } satisfies AdminAiActionData;
      return fail(result.status, { message: result.message, requestId: result.requestId } satisfies AdminAiActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '重试失败，请稍后重试' } satisfies AdminAiActionData);
    }
  },
  cancel: async ({ request, cookies }) => {
    const form = await request.formData();
    const taskId = String(form.get('task_id') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim();
    if (!taskId) return fail(422, { message: '缺少任务标识' } satisfies AdminAiActionData);
    if (!reason) return fail(422, { message: '操作原因必填（写审计）' } satisfies AdminAiActionData);
    try {
      const result = await authedPost<{ ok?: boolean }>(
        cookies,
        `/api/v1/admin/ai/tasks/${encodeURIComponent(taskId)}/cancel`,
        { reason },
        request.headers.get('x-request-id')
      );
      if (result.ok) return { ok: true, message: '已取消任务' } satisfies AdminAiActionData;
      return fail(result.status, { message: result.message, requestId: result.requestId } satisfies AdminAiActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '取消失败，请稍后重试' } satisfies AdminAiActionData);
    }
  }
};
