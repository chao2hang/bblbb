// OAuth 客户端管理页（完整 CRUD）：列表 + 状态切换 + 创建 + 编辑 + 密钥重置。
//
// 后端契约（M13-UI-04 / M11-CONSENT）：
// - GET   /api/v1/admin/oauth-clients            —— 脱敏列表（secret 绝不回显）
// - POST  /api/v1/admin/oauth-clients            —— 创建（返回一次性明文 secret）
// - PATCH /api/v1/admin/oauth-clients/{id}       —— 部分更新（If-Match=version；
//   reset_secret=true 时返回一次性新 secret）；高敏操作需近期登录
//   （403 step_up_required → 前端展示 re-auth 弹窗，M02-MFA-07）。
import { fail, isRedirect, redirect, type Cookies } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPatch, authedPost, getAuthed } from '$lib/api/server';
import { batchResult, parseBatchIds, type BatchOutcome } from '$lib/admin-batch';

export interface AdminOAuthClientItem {
  id: string;
  name: string;
  client_type: string;
  client_id: string;
  status: string;
  version: number;
  redirect_uris: string[];
  post_logout_uris: string[];
  scopes: string[];
  secret_configured: boolean;
  [key: string]: unknown;
}

export type AdminOAuthState = 'ok' | 'forbidden' | 'not_implemented' | 'error';

export interface AdminOAuthPageData {
  state: AdminOAuthState;
  clients: AdminOAuthClientItem[] | null;
  error: string | null;
}

export interface AdminOAuthActionData {
  clients: AdminOAuthClientItem[] | null;
  message?: string | null;
  /** 创建/重置密钥成功时一次性返回的明文 secret（仅本次响应可见）。 */
  secretOnce?: string | null;
  secretClientName?: string | null;
  /** 403 step_up_required → 页面展示重新验证（reauth）表单。 */
  stepUpRequired?: boolean;
}

/** textarea（换行/逗号分隔）→ string[]。 */
function linesToArray(raw: string): string[] {
  return raw
    .split(/[\n,]+/)
    .map((s) => s.trim())
    .filter(Boolean);
}

/** 空格/逗号分隔的 scope 输入 → string[]。 */
function scopesToArray(raw: string): string[] {
  return raw
    .split(/[\s,]+/)
    .map((s) => s.trim())
    .filter(Boolean);
}

async function reloadClients(
  cookies: Cookies,
  requestId: string | null
): Promise<AdminOAuthClientItem[] | null> {
  const result = await getAuthed<{ clients: AdminOAuthClientItem[] }>(
    cookies,
    '/api/v1/admin/oauth-clients',
    requestId
  );
  if (!result.ok && result.status === 401) throw redirect(303, '/login');
  return result.ok ? result.data.clients : null;
}

export const actions: Actions = {
  /** 创建客户端（secret 仅创建时返回一次）。 */
  create: async ({ request, cookies }) => {
    const form = await request.formData();
    const name = String(form.get('name') ?? '').trim();
    const clientType = String(form.get('client_type') ?? 'confidential').trim();
    const redirectUris = linesToArray(String(form.get('redirect_uris') ?? ''));
    const postLogoutUris = linesToArray(String(form.get('post_logout_uris') ?? ''));
    const scopes = scopesToArray(String(form.get('scopes') ?? ''));
    const reason = String(form.get('reason') ?? '').trim();

    if (!name) return fail(422, { clients: null, message: '应用名称必填' } satisfies AdminOAuthActionData);
    if (!['confidential', 'public'].includes(clientType)) {
      return fail(422, { clients: null, message: '客户端类型无效' } satisfies AdminOAuthActionData);
    }
    if (redirectUris.length === 0) {
      return fail(422, { clients: null, message: '至少填写一个 redirect_uri' } satisfies AdminOAuthActionData);
    }
    if (!reason) {
      return fail(422, { clients: null, message: '操作原因必填（写审计）' } satisfies AdminOAuthActionData);
    }
    try {
      const result = await authedPost<{ client: AdminOAuthClientItem; secret?: string }>(
        cookies,
        '/api/v1/admin/oauth-clients',
        {
          name,
          client_type: clientType,
          redirect_uris: redirectUris,
          post_logout_uris: postLogoutUris,
          scopes,
          reason
        },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return {
          clients: await reloadClients(cookies, request.headers.get('x-request-id')),
          message: `客户端「${name}」已创建`,
          secretOnce: result.data.secret ?? null,
          secretClientName: name
        } satisfies AdminOAuthActionData;
      }
      if (result.code === 'step_up_required') {
        return fail(403, {
          clients: null,
          message: '此操作需要重新验证身份，请输入密码重新验证后重试',
          stepUpRequired: true
        } satisfies AdminOAuthActionData);
      }
      return fail(result.status, {
        clients: await reloadClients(cookies, request.headers.get('x-request-id')),
        message: result.message
      } satisfies AdminOAuthActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { clients: null, message: '创建失败，请稍后重试' } satisfies AdminOAuthActionData);
    }
  },

  /** 编辑客户端（name/redirect_uris/post_logout_uris/scopes；If-Match version）。 */
  edit: async ({ request, cookies }) => {
    const form = await request.formData();
    const id = String(form.get('id') ?? '').trim();
    const version = Number(form.get('version') ?? 0);
    const name = String(form.get('name') ?? '').trim();
    const redirectUris = linesToArray(String(form.get('redirect_uris') ?? ''));
    const postLogoutUris = linesToArray(String(form.get('post_logout_uris') ?? ''));
    const scopes = scopesToArray(String(form.get('scopes') ?? ''));
    const reason = String(form.get('reason') ?? '').trim();

    if (!id) return fail(422, { clients: null, message: '缺少客户端标识' } satisfies AdminOAuthActionData);
    if (!Number.isInteger(version) || version < 1) {
      return fail(409, { clients: null, message: '版本缺失或无效，请刷新后重试' } satisfies AdminOAuthActionData);
    }
    if (!name) return fail(422, { clients: null, message: '应用名称必填' } satisfies AdminOAuthActionData);
    if (redirectUris.length === 0) {
      return fail(422, { clients: null, message: '至少填写一个 redirect_uri' } satisfies AdminOAuthActionData);
    }
    if (!reason) return fail(422, { clients: null, message: '操作原因必填（写审计）' } satisfies AdminOAuthActionData);
    try {
      const result = await authedPatch<unknown>(
        cookies,
        `/api/v1/admin/oauth-clients/${encodeURIComponent(id)}`,
        {
          name,
          redirect_uris: redirectUris,
          post_logout_uris: postLogoutUris,
          scopes,
          reason
        },
        { 'If-Match': String(version) },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return {
          clients: await reloadClients(cookies, request.headers.get('x-request-id')),
          message: '客户端信息已保存'
        } satisfies AdminOAuthActionData;
      }
      if (result.code === 'step_up_required') {
        return fail(403, {
          clients: null,
          message: '此操作需要重新验证身份，请输入密码重新验证后重试',
          stepUpRequired: true
        } satisfies AdminOAuthActionData);
      }
      return fail(result.status, {
        clients: await reloadClients(cookies, request.headers.get('x-request-id')),
        message: result.status === 409 ? `版本冲突：${result.message}，请刷新后重试` : result.message
      } satisfies AdminOAuthActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { clients: null, message: '保存失败，请稍后重试' } satisfies AdminOAuthActionData);
    }
  },

  /** 重置 Client Secret（仅 Confidential；新 secret 仅返回一次）。 */
  rotateSecret: async ({ request, cookies }) => {
    const form = await request.formData();
    const id = String(form.get('id') ?? '').trim();
    const name = String(form.get('name') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim();
    if (!id) return fail(422, { clients: null, message: '缺少客户端标识' } satisfies AdminOAuthActionData);
    if (!reason) return fail(422, { clients: null, message: '操作原因必填（写审计）' } satisfies AdminOAuthActionData);
    try {
      const result = await authedPatch<{ client: AdminOAuthClientItem; secret?: string }>(
        cookies,
        `/api/v1/admin/oauth-clients/${encodeURIComponent(id)}`,
        { reset_secret: true, reason },
        {},
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return {
          clients: await reloadClients(cookies, request.headers.get('x-request-id')),
          message: `客户端「${name || id}」的 Secret 已重置，旧凭证立即失效`,
          secretOnce: result.data.secret ?? null,
          secretClientName: name || id
        } satisfies AdminOAuthActionData;
      }
      if (result.code === 'step_up_required') {
        return fail(403, {
          clients: null,
          message: '此操作需要重新验证身份，请输入密码重新验证后重试',
          stepUpRequired: true
        } satisfies AdminOAuthActionData);
      }
      return fail(result.status, {
        clients: await reloadClients(cookies, request.headers.get('x-request-id')),
        message: result.message
      } satisfies AdminOAuthActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { clients: null, message: '重置失败，请稍后重试' } satisfies AdminOAuthActionData);
    }
  },

  /** 禁用/启用：PATCH status（部分更新，其余字段保持）。 */
  toggle: async ({ request, cookies }) => {
    const form = await request.formData();
    const id = String(form.get('id') ?? '').trim();
    const status = String(form.get('status') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim();
    if (!id || !status) return fail(422, { clients: null, message: '缺少参数' } satisfies AdminOAuthActionData);
    if (!reason) return fail(422, { clients: null, message: '操作原因必填（写审计）' } satisfies AdminOAuthActionData);
    try {
      const result = await authedPatch<unknown>(
        cookies,
        `/api/v1/admin/oauth-clients/${encodeURIComponent(id)}`,
        { status, reason },
        {},
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return {
          clients: await reloadClients(cookies, request.headers.get('x-request-id')),
          message: `客户端已${status === 'active' ? '启用' : '禁用'}`
        } satisfies AdminOAuthActionData;
      }
      if (result.code === 'step_up_required') {
        return fail(403, {
          clients: null,
          message: '此操作需要重新验证身份，请输入密码重新验证后重试',
          stepUpRequired: true
        } satisfies AdminOAuthActionData);
      }
      return fail(result.status, {
        clients: await reloadClients(cookies, request.headers.get('x-request-id')),
        message: result.message
      } satisfies AdminOAuthActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { clients: null, message: '操作失败，请稍后重试' } satisfies AdminOAuthActionData);
    }
  },

  /**
   * 批量启用/禁用（约定 B）：循环调用与单条 toggle 完全相同的后端端点
   * （PATCH /api/v1/admin/oauth-clients/{id}，body {status, reason}，无 If-Match——
   * 与单条 toggle 一致），逐条汇总成败；命中 step_up_required 即中止（后续必然同样失败），
   * 交由页面 reauth 弹窗处理。
   */
  batchToggle: async ({ request, cookies }) => {
    const form = await request.formData();
    const ids = parseBatchIds(form);
    const status = String(form.get('status') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim();
    if (ids.length === 0) {
      return fail(422, { clients: null, message: '未选择任何条目' } satisfies AdminOAuthActionData);
    }
    if (!['active', 'disabled'].includes(status)) {
      return fail(422, { clients: null, message: '目标状态无效' } satisfies AdminOAuthActionData);
    }
    if (!reason) {
      return fail(422, { clients: null, message: '操作原因必填（写审计）' } satisfies AdminOAuthActionData);
    }
    const requestId = request.headers.get('x-request-id');
    const outcome: BatchOutcome = { okCount: 0, failures: [] };
    let stepUp = false;
    for (const id of ids) {
      try {
        const result = await authedPatch<unknown>(
          cookies,
          `/api/v1/admin/oauth-clients/${encodeURIComponent(id)}`,
          { status, reason },
          {},
          requestId
        );
        if (result.ok) {
          outcome.okCount++;
        } else if (result.code === 'step_up_required') {
          stepUp = true;
          break;
        } else {
          outcome.failures.push({ id, message: result.message });
        }
      } catch {
        outcome.failures.push({ id, message: '网络错误' });
      }
    }
    if (stepUp) {
      return fail(403, {
        clients: null,
        message: '此操作需要重新验证身份，请输入密码重新验证后重试',
        stepUpRequired: true
      } satisfies AdminOAuthActionData);
    }
    const clients = await reloadClients(cookies, requestId);
    const r = batchResult(outcome, `批量${status === 'active' ? '启用' : '禁用'}客户端`);
    return r.ok
      ? { clients, message: r.message } satisfies AdminOAuthActionData
      : fail(r.status, { clients, message: r.message } satisfies AdminOAuthActionData);
  },

  /** 重新验证身份（step-up 窗口过期后；与 storage 页同款交互）。 */
  reauth: async ({ request, cookies }) => {
    const form = await request.formData();
    const password = String(form.get('password') ?? '');
    if (!password) {
      return fail(422, { clients: null, message: '请输入当前密码' } satisfies AdminOAuthActionData);
    }
    try {
      const result = await authedPost(
        cookies,
        '/api/v1/auth/re-auth',
        { password },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return {
          clients: null,
          message: '已重新验证身份，请重试刚才的操作'
        } satisfies AdminOAuthActionData;
      }
      return fail(result.status, { clients: null, message: result.message } satisfies AdminOAuthActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { clients: null, message: '验证失败，请稍后重试' } satisfies AdminOAuthActionData);
    }
  }
};

export const load: PageServerLoad = async ({ cookies, request }): Promise<AdminOAuthPageData> => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<{ clients: AdminOAuthClientItem[] }>(
    cookies,
    '/api/v1/admin/oauth-clients',
    requestId
  );
  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    if (result.status === 403) return { state: 'forbidden', clients: null, error: result.message };
    if (result.status === 501) return { state: 'not_implemented', clients: null, error: result.message };
    return { state: 'error', clients: null, error: result.message };
  }
  return { state: 'ok', clients: result.data.clients, error: null };
};
