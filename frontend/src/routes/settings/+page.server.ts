// M03-UI-02：/settings 页资料编辑——SSR 表单 + If-Match 乐观并发
//
// - load：服务端取 GET /api/v1/me（User 含 version，M03-PROFILE-04 后
//   后端已返回）；401 → 跳登录；
// - action `profile`：PATCH /api/v1/me（会话绑定 CSRF + If-Match 版本头，
//   authedPatch 支持 extraHeaders）；
// - 成功 → 返回更新后 Me 投影（form.user 用于保存后投影刷新；use:enhance
//   成功默认 invalidateAll 使 data 也保持新鲜）；
// - 409 version_conflict → fail(409, { conflict: true }) 页面提示刷新重编；
//   400/422 → fail(状态, { message }) 字段错误横幅；版本缺失 → 422。
// - 全部写操作只经服务端代理，浏览器不直接打 /api/v1/me（生产 adapter-node
//   无 /api 代理，M14-ROUTES 验收）。

import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPatch, getAuthed } from '$lib/api/server';
import type { User } from '$lib/api/types';
import { clampProfileText, PROFILE_TEXT_LIMITS } from '$lib/profile';

export interface SettingsPageData {
  user: User | null;
  error: string | null;
}

export interface SettingsFormResult {
  ok?: boolean;
  conflict?: boolean;
  message?: string;
  requestId?: string | null;
  user?: User;
}

export const load: PageServerLoad = async ({ cookies, request }) => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<User>(cookies, '/api/v1/me', requestId);
  if (result.ok === false) {
    if (result.status === 401) throw redirect(303, '/login');
    return { user: null, error: result.message } satisfies SettingsPageData;
  }
  return { user: result.data, error: null } satisfies SettingsPageData;
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

    try {
      const result = await authedPatch<User>(
        cookies,
        '/api/v1/me',
        {
          display_name: display_name || null,
          bio: bio || null,
          signature: signature || null
        },
        { 'If-Match': String(version) },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
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
  }
};
