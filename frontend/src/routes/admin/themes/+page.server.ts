// M13-UI-01/03：管理主题页——列表/上传/设默认/编辑 Token/预览/回退/版本冲突。
//
// - load：GET /api/v1/admin/themes（closed Token 投影；秘密/内部字段不进 SSR）；
// - upload action：POST /api/v1/admin/themes/data-packages（reason + CSRF；
//   上传即 disabled 隔离态）；
// - set-default action：PUT /api/v1/admin/themes/default（激活 + 审计）；
// - save-settings action：PATCH /api/v1/admin/themes/{name}/settings（If-Match
//   revision 乐观锁；409 版本冲突提示刷新）；
// - 主题预览：Token 只在 SSR/浏览器端用 applyThemeTokens 应用安全投影。
import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPatch, authedPost, authedPut, getAuthed } from '$lib/api/server';
import { pickActiveTheme, fallbackDefaultTheme } from '$lib/theme/projection';

export interface AdminThemeItem {
  name: string;
  display_name: string;
  kind: string;
  schema_version: number;
  version: string;
  supports: string;
  status: string;
  is_default: boolean;
  revision: number;
  tokens: Record<string, unknown>;
  created_by: string;
  updated_at: number;
}

export type AdminThemesLoadState = 'ok' | 'forbidden' | 'not_implemented' | 'error';

export interface AdminThemesPageData {
  state: AdminThemesLoadState;
  themes: AdminThemeItem[] | null;
  error: string | null;
  /** 预览主题（安全投影；SSR 无主题时用内置 default）。 */
  preview: { name: string; revision: number; tokens: Record<string, unknown> } | null;
}

export interface AdminThemesActionData {
  message?: string;
  requestId?: string | null;
  conflict?: boolean;
  uploaded?: string | null;
}

export const load: PageServerLoad = async ({ cookies, request }): Promise<AdminThemesPageData> => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<{ themes: AdminThemeItem[] }>(
    cookies,
    '/api/v1/admin/themes',
    requestId
  );
  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    if (result.status === 403) {
      return { state: 'forbidden', themes: null, error: result.message, preview: null };
    }
    if (result.status === 501) {
      return { state: 'not_implemented', themes: null, error: result.message, preview: null };
    }
    return { state: 'error', themes: null, error: result.message, preview: null };
  }
  const themes = Array.isArray(result.data.themes) ? result.data.themes : [];
  // 预览 = 站点默认或第一个 active 主题；没有则内置 default。
  const active = themes.find((t) => t.is_default && t.status === 'active') ?? themes[0];
  const picked = active ? pickActiveTheme(active) : null;
  const preview = picked ?? fallbackDefaultTheme();
  return { state: 'ok', themes, error: null, preview };
};

function readThemeForm(form: FormData): { name: string; reason: string } {
  return {
    name: String(form.get('name') ?? '').trim(),
    reason: String(form.get('reason') ?? '').trim()
  };
}

export const actions: Actions = {
  upload: async ({ request, cookies }) => {
    const form = await request.formData();
    const reason = String(form.get('reason') ?? '').trim();
    const name = String(form.get('name') ?? '').trim();
    const displayName = String(form.get('display_name') ?? name).trim();
    const tokensRaw = String(form.get('tokens_json') ?? '');
    if (!reason) return fail(422, { message: '操作原因必填（写审计）' });
    if (!/^[a-z0-9-]{1,64}$/.test(name)) {
      return fail(422, { message: '主题名必须是小写字母/数字/连字符（<=64）' });
    }
    let tokens: Record<string, unknown>;
    try {
      const parsed = JSON.parse(tokensRaw);
      if (!parsed || typeof parsed !== 'object') throw new Error('not object');
      tokens = parsed as Record<string, unknown>;
    } catch {
      return fail(422, { message: 'tokens 必须是合法 JSON 对象' });
    }
    const body = {
      schema_version: 1,
      name,
      display_name: displayName,
      version: '1.0.0',
      supports: '>=1.0 <2.0',
      kind: 'data',
      tokens,
      reason
    };
    try {
      const result = await authedPost<{ theme: AdminThemeItem }>(
        cookies,
        '/api/v1/admin/themes/data-packages',
        body,
        request.headers.get('x-request-id')
      );
      if (result.ok) return { message: `主题 ${result.data.theme.name} 已上传（disabled 隔离态）`, uploaded: result.data.theme.name };
      return fail(result.status, { message: result.message, requestId: result.requestId });
    } catch {
      return fail(503, { message: '上传失败，请稍后重试' });
    }
  },
  'set-default': async ({ request, cookies }) => {
    const form = await request.formData();
    const { name, reason } = readThemeForm(form);
    if (!reason) return fail(422, { message: '操作原因必填（写审计）' });
    if (!name) return fail(422, { message: '主题名缺失' });
    try {
      const result = await authedPut<{ theme: AdminThemeItem }>(
        cookies,
        '/api/v1/admin/themes/default',
        { name, reason },
        request.headers.get('x-request-id')
      );
      if (result.ok) return { message: `主题 ${name} 已设为站点默认并激活` };
      return fail(result.status, { message: result.message, requestId: result.requestId });
    } catch {
      return fail(503, { message: '保存失败，请稍后重试' });
    }
  },
  'save-settings': async ({ request, cookies }) => {
    const form = await request.formData();
    const name = String(form.get('name') ?? '').trim();
    const reason = String(form.get('reason') ?? '').trim();
    const revision = Number(form.get('revision') ?? 0);
    const tokensRaw = String(form.get('tokens_json') ?? '');
    if (!reason) return fail(422, { message: '操作原因必填（写审计）' });
    let tokens: Record<string, unknown>;
    try {
      const parsed = JSON.parse(tokensRaw);
      if (!parsed || typeof parsed !== 'object') throw new Error('not object');
      tokens = parsed as Record<string, unknown>;
    } catch {
      return fail(422, { message: 'tokens 必须是合法 JSON 对象' });
    }
    try {
      const result = await authedPatch<{ theme: AdminThemeItem }>(
        cookies,
        `/api/v1/admin/themes/${encodeURIComponent(name)}/settings`,
        { tokens, reason },
        { 'If-Match': String(revision) },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { message: `主题 ${name} Token 已保存（revision v${result.data.theme.revision}）` };
      }
      if (result.status === 409) {
        return fail(409, { conflict: true, message: `版本冲突：${result.message}（请刷新后重试）` });
      }
      return fail(result.status, { message: result.message, requestId: result.requestId });
    } catch {
      return fail(503, { message: '保存失败，请稍后重试' });
    }
  }
};
