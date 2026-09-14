// M03-UI-02：/settings 页资料编辑——SSR 表单 + If-Match 乐观并发
//
// - load：服务端取 GET /api/v1/me（User 含 version，M03-PROFILE-04 后
//   后端已返回）；401 → 跳登录；GAP-FIX 增强：GET /me/oauth-grants
//   （OAuth 授权应用列表，失败时非致命降级为空列表）；
// - action `profile`：PATCH /api/v1/me（会话绑定 CSRF + If-Match 版本头，
//   authedPatch 支持 extraHeaders）；
// - action `visibility`（GAP-FIX 资料可见性）：PATCH /api/v1/me 提交
//   profile_visible_to（everyone|registered|nobody，后端已支持）；
// - action `password`（GAP-FIX 修改密码）：POST /api/v1/me/password
//   （后端校验当前密码，成功后撤销其他会话）；
// - action `revoke-oauth`（GAP-FIX OAuth 授权管理）：DELETE
//   /api/v1/me/oauth-grants/{client_id}；
// - 成功 → 返回更新后 Me 投影（form.user 用于保存后投影刷新；use:enhance
//   成功默认 invalidateAll 使 data 也保持新鲜）；
// - 409 version_conflict → fail(409, { conflict: true }) 页面提示刷新重编；
//   400/422 → fail(状态, { message }) 字段错误横幅；版本缺失 → 422。
// - 全部写操作只经服务端代理，浏览器不直接打 /api/v1/me（生产 adapter-node
//   无 /api 代理，M14-ROUTES 验收）。

import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedDelete, authedDeleteBody, authedPatch, authedPost, getAuthed } from '$lib/api/server';
import type { OAuthGrantItem, User } from '$lib/api/types';
import { clampProfileText, PROFILE_TEXT_LIMITS } from '$lib/profile';
import type { PasskeyInfo } from '$lib/mfa/passkey-types';

export interface SettingsPageData {
  user: User | null;
  error: string | null;
  /** GAP-FIX：OAuth 授权应用列表。
   *  可选：旧 fixture/渐进迁移下允许缺失（页面按空列表处理）。 */
  grants?: OAuthGrantItem[];
  /** 个人资料封面（GET /api/v1/users/{id}/profile-cover，失败降级 null）。 */
  cover?: {
    attachment_id: string;
    alt_text?: string;
    position?: string;
    content_url?: string;
  } | null;
  /** 服务端是否配置了 Passkey（未配置时整块隐藏） */
  passkeyEnabled?: boolean;
  passkeys?: PasskeyInfo[];
  passkeysError?: string | null;
}

export interface SettingsFormResult {
  ok?: boolean;
  conflict?: boolean;
  message?: string;
  requestId?: string | null;
  user?: User;
  /** ?/password 修改密码结果（inline 错误定位到具体字段）。 */
  password?: {
    ok: boolean;
    currentError?: string;
    newError?: string;
    message?: string;
  };
  /** ?/visibility 资料可见性保存结果。 */
  visibility?: { ok: boolean; message?: string };
  /** ?/revoke-oauth 撤销授权结果。 */
  revokeOAuth?: { ok: boolean; message?: string };
}

export const load: PageServerLoad = async ({ cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<User>(cookies, '/api/v1/me', requestId);
  if (result.ok === false) {
    if (result.status === 401) throw redirect(303, '/login');
    return { user: null, error: result.message, grants: [] } satisfies SettingsPageData;
  }
  // OAuth 授权列表（GAP-FIX）：非致命——接口失败时为空，页面按空态渲染。
  const grantsResult = await getAuthed<{ items?: OAuthGrantItem[] }>(
    cookies,
    '/api/v1/me/oauth-grants',
    requestId
  );
  const grants =
    grantsResult.ok && Array.isArray(grantsResult.data.items) ? grantsResult.data.items : [];

  // 个人资料封面：非致命——接口 204 或失败时为空。
  let cover: SettingsPageData['cover'] = null;
  if (result.data?.id) {
    const coverResult = await getAuthed<{
      attachment_id: string;
      alt_text?: string;
      position?: string;
      content_url?: string;
    }>(cookies, `/api/v1/users/${result.data.id}/profile-cover`, requestId);
    if (coverResult.ok && coverResult.data?.attachment_id) {
      cover = coverResult.data;
    }
  }

  // Passkey 列表（M02-MFA-PK）：passkey_not_configured → 未配置；
  // 其他错误不阻断设置页正常加载。
  let passkeyEnabled = false;
  let passkeys: PasskeyInfo[] = [];
  let passkeysError: string | null = null;
  const passkeyResult = await getAuthed<{ passkeys?: PasskeyInfo[] }>(
    cookies,
    '/api/v1/auth/passkeys',
    requestId
  );
  if (passkeyResult.ok) {
    passkeyEnabled = true;
    passkeys = passkeyResult.data.passkeys ?? [];
  } else if (passkeyResult.code !== 'passkey_not_configured') {
    passkeysError = passkeyResult.message;
    if (passkeyResult.status < 500) passkeyEnabled = true;
  }

  return {
    user: result.data,
    error: null,
    grants,
    cover,
    passkeyEnabled,
    passkeys,
    passkeysError
  } satisfies SettingsPageData;
};

export const actions: Actions = {
  profile: async ({ request, cookies }) => {
    const form = await request.formData();
    const versionRaw = String(form.get('version') ?? '').trim();
    const version = Number(versionRaw);
    if (!Number.isInteger(version) || version < 1) {
      return fail(422, {
        message: '资料版本缺失或无效，请刷新页面后重试'
      } satisfies SettingsFormResult);
    }

    const display_name = clampProfileText(String(form.get('display_name') ?? '').trim(), PROFILE_TEXT_LIMITS.display_name);
    const bio = clampProfileText(String(form.get('bio') ?? '').trim(), PROFILE_TEXT_LIMITS.bio);
    const signature = clampProfileText(String(form.get('signature') ?? '').trim(), PROFILE_TEXT_LIMITS.signature);
    const avatarRaw = form.has('avatar_attachment_id')
      ? String(form.get('avatar_attachment_id') ?? '').trim()
      : undefined;
    const coverRaw = form.has('cover_attachment_id')
      ? String(form.get('cover_attachment_id') ?? '').trim()
      : undefined;

    const patchBody: Record<string, unknown> = {
      display_name: display_name || null,
      bio: bio || null,
      signature: signature || null
    };
    if (avatarRaw !== undefined) {
      patchBody.avatar_attachment_id = avatarRaw || null;
    }

    try {
      const result = await authedPatch<User>(
        cookies,
        '/api/v1/me',
        patchBody,
        { 'If-Match': String(version) },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        if (coverRaw !== undefined) {
          if (coverRaw) {
            await authedPost(
              cookies,
              '/api/v1/me/profile-cover',
              { attachment_id: coverRaw, alt_text: '', position: 'center' },
              request.headers.get('x-request-id')
            );
          } else {
            await authedDeleteBody(
              cookies,
              '/api/v1/me/profile-cover',
              { attachment_id: '00000000-0000-0000-0000-000000000000', alt_text: '', position: '' },
              request.headers.get('x-request-id')
            );
          }
        }
        return { ok: true, user: result.data } satisfies SettingsFormResult;
      }
      if (result.status === 409) {
        return fail(409, {
          conflict: true,
          message: '资料已在其他窗口被修改，请刷新后重新编辑',
          requestId: result.requestId
        } satisfies SettingsFormResult);
      }
      return fail(result.status, {
        message: result.message,
        requestId: result.requestId
      } satisfies SettingsFormResult);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '保存失败，请稍后重试' } satisfies SettingsFormResult);
    }
  },

  // GAP-FIX 资料可见性：PATCH /api/v1/me profile_visible_to（后端已支持，
  // users.rs update_me 接收该字段）。独立于 `profile` action，避免影响
  // 基本资料表单的 If-Match 语义（两个表单各自带 version）。
  visibility: async ({ request, cookies }) => {
    const form = await request.formData();
    const versionRaw = String(form.get('version') ?? '').trim();
    const version = Number(versionRaw);
    const level = String(form.get('profile_visible_to') ?? '').trim();
    if (!['everyone', 'registered', 'nobody'].includes(level)) {
      return fail(422, {
        visibility: { ok: false, message: '可见性取值无效，请刷新后重试' }
      } satisfies SettingsFormResult);
    }
    if (!Number.isInteger(version) || version < 1) {
      return fail(422, {
        visibility: { ok: false, message: '资料版本缺失或无效，请刷新页面后重试' }
      } satisfies SettingsFormResult);
    }
    try {
      const result = await authedPatch<User>(
        cookies,
        '/api/v1/me',
        { profile_visible_to: level },
        { 'If-Match': String(version) },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return {
          ok: true,
          user: result.data,
          visibility: { ok: true, message: '资料可见性已保存' }
        } satisfies SettingsFormResult;
      }
      if (result.status === 409) {
        return fail(409, {
          conflict: true,
          message: '资料已在其他窗口被修改，请刷新后重新编辑',
          requestId: result.requestId
        } satisfies SettingsFormResult);
      }
      return fail(result.status, {
        visibility: { ok: false, message: result.message },
        requestId: result.requestId
      } satisfies SettingsFormResult);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, {
        visibility: { ok: false, message: '保存失败，请稍后重试' }
      } satisfies SettingsFormResult);
    }
  },

  // GAP-FIX 修改密码：POST /api/v1/me/password（当前密码校验 401 → inline
  // 错误；成功后后端撤销其他会话）。
  password: async ({ request, cookies }) => {
    const form = await request.formData();
    const currentPassword = String(form.get('current_password') ?? '');
    const newPassword = String(form.get('new_password') ?? '');
    const confirmPassword = String(form.get('confirm_password') ?? '');

    if (!currentPassword) {
      return fail(422, {
        password: { ok: false, currentError: '请输入当前密码' }
      } satisfies SettingsFormResult);
    }
    if (newPassword.length < 8 || newPassword.length > 128 || !/[A-Za-z]/.test(newPassword) || !/[0-9]/.test(newPassword)) {
      return fail(422, {
        password: { ok: false, newError: '新密码需为 8-128 位且同时包含字母和数字' }
      } satisfies SettingsFormResult);
    }
    if (newPassword !== confirmPassword) {
      return fail(422, {
        password: { ok: false, newError: '两次输入的新密码不一致' }
      } satisfies SettingsFormResult);
    }
    try {
      const result = await authedPost(
        cookies,
        '/api/v1/me/password',
        { current_password: currentPassword, new_password: newPassword },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return {
          password: { ok: true, message: '密码已更新，其他设备已下线' }
        } satisfies SettingsFormResult;
      }
      if (result.status === 401) {
        return fail(401, {
          password: { ok: false, currentError: '当前密码不正确' }
        } satisfies SettingsFormResult);
      }
      return fail(result.status, {
        password: { ok: false, message: result.message },
        requestId: result.requestId
      } satisfies SettingsFormResult);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, {
        password: { ok: false, message: '密码服务暂不可用，请稍后重试' }
      } satisfies SettingsFormResult);
    }
  },

  // 快捷 action：直接保存头像（上传就绪后即时持久化，无需点击底部保存修改）
  'update-avatar': async ({ request, cookies }) => {
    const form = await request.formData();
    const versionRaw = String(form.get('version') ?? '').trim();
    const version = Number(versionRaw);
    const avatarAttachmentId = String(form.get('avatar_attachment_id') ?? '').trim();
    if (!Number.isInteger(version) || version < 1) {
      return fail(422, { message: '版本无效，请刷新页面' } satisfies SettingsFormResult);
    }
    try {
      console.log('[DEBUG update-avatar] calling PATCH /api/v1/me with:', {
        avatar_attachment_id: avatarAttachmentId || null,
        IfMatch: String(version)
      });
      const result = await authedPatch<User>(
        cookies,
        '/api/v1/me',
        { avatar_attachment_id: avatarAttachmentId || null },
        { 'If-Match': String(version) },
        request.headers.get('x-request-id')
      );
      console.log('[DEBUG update-avatar] result:', result);
      if (result.ok) {
        return { ok: true, user: result.data } satisfies SettingsFormResult;
      }
      return fail(result.status, { message: result.message, requestId: result.requestId } satisfies SettingsFormResult);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '保存头像失败，请稍后重试' } satisfies SettingsFormResult);
    }
  },

  // 快捷 action：直接保存封面（上传就绪后即时持久化）
  'update-cover': async ({ request, cookies }) => {
    const form = await request.formData();
    const coverAttachmentId = String(form.get('cover_attachment_id') ?? '').trim();
    try {
      if (coverAttachmentId) {
        const res = await authedPost(
          cookies,
          '/api/v1/me/profile-cover',
          { attachment_id: coverAttachmentId, alt_text: '', position: 'center' },
          request.headers.get('x-request-id')
        );
        if (!res.ok) return fail(res.status, { message: res.message } satisfies SettingsFormResult);
      } else {
        const res = await authedDeleteBody(
          cookies,
          '/api/v1/me/profile-cover',
          { attachment_id: '00000000-0000-0000-0000-000000000000', alt_text: '', position: '' },
          request.headers.get('x-request-id')
        );
        if (!res.ok) return fail(res.status, { message: res.message } satisfies SettingsFormResult);
      }
      // 重新读取 me 刷新用户投影
      const meRes = await getAuthed<User>(cookies, '/api/v1/me', request.headers.get('x-request-id'));
      return { ok: true, user: meRes.ok ? meRes.data : undefined } satisfies SettingsFormResult;
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '保存封面失败，请稍后重试' } satisfies SettingsFormResult);
    }
  },

  // GAP-FIX 撤销 OAuth 授权：DELETE /api/v1/me/oauth-grants/{client_id}。
  'revoke-oauth': async ({ request, cookies }) => {
    const form = await request.formData();
    const clientId = String(form.get('client_id') ?? '').trim();
    if (!clientId) {
      return fail(422, {
        revokeOAuth: { ok: false, message: '缺少应用标识，请刷新后重试' }
      } satisfies SettingsFormResult);
    }
    try {
      const result = await authedDelete(
        cookies,
        `/api/v1/me/oauth-grants/${encodeURIComponent(clientId)}`,
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return {
          revokeOAuth: { ok: true, message: '已撤销授权' }
        } satisfies SettingsFormResult;
      }
      if (result.status === 401) throw redirect(303, '/login');
      return fail(result.status, {
        revokeOAuth: { ok: false, message: result.message },
        requestId: result.requestId
      } satisfies SettingsFormResult);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, {
        revokeOAuth: { ok: false, message: '撤销授权失败，请稍后重试' }
      } satisfies SettingsFormResult);
    }
  },

  // Passkey 撤销：DELETE /api/v1/auth/passkeys/{id}
  passkeyRevoke: async ({ request, cookies }) => {
    const form = await request.formData();
    const id = String(form.get('id') ?? '').trim();
    if (!id) {
      return fail(422, { message: '缺少 Passkey 标识' } satisfies SettingsFormResult);
    }
    try {
      const result = await authedDelete(
        cookies,
        `/api/v1/auth/passkeys/${encodeURIComponent(id)}`,
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { ok: true } satisfies SettingsFormResult;
      }
      if (result.status === 401) throw redirect(303, '/login');
      return fail(result.status, {
        message: result.message,
        requestId: result.requestId
      } satisfies SettingsFormResult);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '撤销 Passkey 失败，请稍后重试' } satisfies SettingsFormResult);
    }
  }
};
