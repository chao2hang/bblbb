// M13-UI-01/PLUGIN-06：管理插件页——能力白名单/列表/安装/启停/设置/调用摘要。
//
// - load：GET /api/v1/admin/plugins + GET /api/v1/admin/plugins/capabilities
//   （脱敏投影；插件 settings 不含 Secret/正文；无 JS 也可用原生表单）；
// - install action：POST /api/v1/admin/plugins（reason + CSRF；默认 disabled）；
// - enable/disable action：POST .../enable|disable（If-Match policy_revision）；
// - settings action：PATCH .../{id}/settings（If-Match + reason + 审计）。
import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPatch, authedPost, getAuthed } from '$lib/api/server';

export interface AdminPluginItem {
  id: string;
  name: string;
  version: string;
  supports: string;
  status: string;
  capabilities: string[];
  subscriptions: string[];
  settings: Record<string, unknown>;
  policy_revision: number;
  created_at: number;
  updated_at: number;
}

export interface PluginCapabilitiesView {
  capabilities: string[];
  events: string[];
  service_interface: { action: string; description: string }[];
  provider_adapters: { provider: string; kind: string; managed: boolean }[];
  v1_execution: string;
  note: string;
}

export type AdminPluginsLoadState = 'ok' | 'forbidden' | 'not_implemented' | 'error';

export interface AdminPluginsPageData {
  state: AdminPluginsLoadState;
  plugins: AdminPluginItem[] | null;
  capabilities: PluginCapabilitiesView | null;
  error: string | null;
}

export interface AdminPluginsActionData {
  message?: string;
  requestId?: string | null;
  conflict?: boolean;
}

export const load: PageServerLoad = async ({ cookies, request }): Promise<AdminPluginsPageData> => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<{ plugins: AdminPluginItem[] }>(
    cookies,
    '/api/v1/admin/plugins',
    requestId
  );
  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    if (result.status === 403) {
      return { state: 'forbidden', plugins: null, capabilities: null, error: result.message };
    }
    if (result.status === 501) {
      return { state: 'not_implemented', plugins: null, capabilities: null, error: result.message };
    }
    return { state: 'error', plugins: null, capabilities: null, error: result.message };
  }
  const capsResult = await getAuthed<PluginCapabilitiesView>(
    cookies,
    '/api/v1/admin/plugins/capabilities',
    requestId
  );
  return {
    state: 'ok',
    plugins: Array.isArray(result.data.plugins) ? result.data.plugins : [],
    capabilities: capsResult.ok ? capsResult.data : null,
    error: null
  };
};

function readCommon(form: FormData): { id: string; reason: string; revision: number } {
  return {
    id: String(form.get('id') ?? '').trim(),
    reason: String(form.get('reason') ?? '').trim(),
    revision: Number(form.get('policy_revision') ?? 0)
  };
}

export const actions: Actions = {
  install: async ({ request, cookies }) => {
    const form = await request.formData();
    const reason = String(form.get('reason') ?? '').trim();
    const id = String(form.get('id') ?? '').trim();
    const name = String(form.get('name') ?? id).trim();
    const capabilitiesRaw = String(form.get('capabilities') ?? '[]');
    const subscriptionsRaw = String(form.get('subscriptions') ?? '[]');
    const schemaRaw = String(form.get('settings_schema') ?? '');
    if (!reason) return fail(422, { message: '操作原因必填（写审计）' });
    if (!/^[a-z0-9-]{1,64}$/.test(id)) {
      return fail(422, { message: '插件 ID 必须是小写字母/数字/连字符（<=64）' });
    }
    let capabilities: string[];
    let subscriptions: string[];
    let settings_schema: unknown;
    try {
      capabilities = JSON.parse(capabilitiesRaw);
      subscriptions = JSON.parse(subscriptionsRaw);
      settings_schema = JSON.parse(schemaRaw);
    } catch {
      return fail(422, { message: 'capabilities/subscriptions/settings_schema 必须是合法 JSON' });
    }
    const body = {
      schema_version: 1,
      id,
      name,
      version: '1.0.0',
      supports: '>=1.0 <2.0',
      kind: 'config',
      subscriptions,
      capabilities,
      settings_schema,
      reason
    };
    try {
      const result = await authedPost<{ plugin: AdminPluginItem }>(
        cookies,
        '/api/v1/admin/plugins',
        body,
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { message: `插件 ${result.data.plugin.id} 已安装（disabled 隔离态）` };
      }
      return fail(result.status, { message: result.message, requestId: result.requestId });
    } catch {
      return fail(503, { message: '安装失败，请稍后重试' });
    }
  },
  enable: async ({ request, cookies }) => {
    const form = await request.formData();
    const { id, reason, revision } = readCommon(form);
    if (!reason) return fail(422, { message: '操作原因必填（写审计）' });
    try {
      const result = await authedPost<{ plugin: AdminPluginItem }>(
        cookies,
        `/api/v1/admin/plugins/${encodeURIComponent(id)}/enable`,
        { reason },
        request.headers.get('x-request-id'),
        { 'If-Match': String(revision) }
      );
      if (result.ok) return { message: `插件 ${id} 已启用（policy v${result.data.plugin.policy_revision}）` };
      if (result.status === 409) {
        return fail(409, { conflict: true, message: `版本冲突：${result.message}` });
      }
      return fail(result.status, { message: result.message, requestId: result.requestId });
    } catch {
      return fail(503, { message: '操作失败，请稍后重试' });
    }
  },
  disable: async ({ request, cookies }) => {
    const form = await request.formData();
    const { id, reason, revision } = readCommon(form);
    if (!reason) return fail(422, { message: '操作原因必填（写审计）' });
    try {
      const result = await authedPost<{ plugin: AdminPluginItem }>(
        cookies,
        `/api/v1/admin/plugins/${encodeURIComponent(id)}/disable`,
        { reason },
        request.headers.get('x-request-id'),
        { 'If-Match': String(revision) }
      );
      if (result.ok) return { message: `插件 ${id} 已停用（不再消费新事件）` };
      if (result.status === 409) {
        return fail(409, { conflict: true, message: `版本冲突：${result.message}` });
      }
      return fail(result.status, { message: result.message, requestId: result.requestId });
    } catch {
      return fail(503, { message: '操作失败，请稍后重试' });
    }
  },
  settings: async ({ request, cookies }) => {
    const form = await request.formData();
    const { id, reason, revision } = readCommon(form);
    const settingsRaw = String(form.get('settings_json') ?? '{}');
    if (!reason) return fail(422, { message: '操作原因必填（写审计）' });
    let settings: Record<string, unknown>;
    try {
      const parsed = JSON.parse(settingsRaw);
      if (!parsed || typeof parsed !== 'object') throw new Error('not object');
      settings = parsed as Record<string, unknown>;
    } catch {
      return fail(422, { message: 'settings 必须是合法 JSON 对象' });
    }
    try {
      const result = await authedPatch<{ plugin: AdminPluginItem }>(
        cookies,
        `/api/v1/admin/plugins/${encodeURIComponent(id)}/settings`,
        { settings, reason },
        { 'If-Match': String(revision) },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { message: `插件 ${id} 设置已保存（policy v${result.data.plugin.policy_revision}）` };
      }
      if (result.status === 409) {
        return fail(409, { conflict: true, message: `版本冲突：${result.message}` });
      }
      return fail(result.status, { message: result.message, requestId: result.requestId });
    } catch {
      return fail(503, { message: '保存失败，请稍后重试' });
    }
  }
};
