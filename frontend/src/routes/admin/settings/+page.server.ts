// GAP-FIX（管理域·系统设置）：/admin/settings 重写——真实设置表单替换外链占位。
// - load：GET /api/v1/admin/settings（settings + version 乐观并发）；
// - save：PATCH /api/v1/admin/settings + If-Match version（功能开关五项 +
//   站点信息四项含公开源；reason 必填报审计）；
// - 401 → /login、403 → forbidden；409 → 冲突态提示刷新。
import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPatch, getAuthed } from '$lib/api/server';
import type { AdminSettingsResult } from '$lib/api/types';

export type AdminSettingsState = 'ok' | 'forbidden' | 'not_implemented' | 'error';

export interface AdminSettingsPageData {
  state: AdminSettingsState;
  settings: AdminSettingsResult['settings'] | null;
  version: number;
  error: string | null;
}

export interface AdminSettingsActionData {
  message?: string;
  requestId?: string | null;
  conflict?: boolean;
}

export const load: PageServerLoad = async ({
  cookies,
  request
}): Promise<AdminSettingsPageData> => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<AdminSettingsResult>(cookies, '/api/v1/admin/settings', requestId);
  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    if (result.status === 403) {
      return { state: 'forbidden', settings: null, version: 0, error: result.message };
    }
    if (result.status === 501) {
      return { state: 'not_implemented', settings: null, version: 0, error: result.message };
    }
    return { state: 'error', settings: null, version: 0, error: result.message };
  }
  return {
    state: 'ok',
    settings: result.data.settings,
    version: result.data.version,
    error: null
  };
};

export const actions: Actions = {
  save: async ({ request, cookies }) => {
    const form = await request.formData();
    const version = Number(form.get('version') ?? 0);
    const siteName = String(form.get('site_name') ?? '').trim();
    const defaultLang = String(form.get('default_lang') ?? '').trim();
    const publicSource = String(form.get('public_source') ?? '').trim();
    const apiRateLimit = Number(form.get('api_rate_limit') ?? NaN);
    const reason = String(form.get('reason') ?? '').trim();

    // 站点文案（0065；空串 = 前端内置通用文案兜底）。
    const siteDescription = String(form.get('site_description') ?? '').trim();
    const loginEyebrow = String(form.get('login_eyebrow') ?? '').trim();
    const loginTitle = String(form.get('login_title') ?? '').trim();
    const loginSubtitle = String(form.get('login_subtitle') ?? '').trim();
    const registerEyebrow = String(form.get('register_eyebrow') ?? '').trim();
    const registerTitle = String(form.get('register_title') ?? '').trim();
    const registerSubtitle = String(form.get('register_subtitle') ?? '').trim();

    // SMTP 设置字段
    const smtpEnabled = form.has('smtp_enabled');
    const smtpHost = String(form.get('smtp_host') ?? '').trim();
    const smtpPort = Number(form.get('smtp_port') ?? 587);
    const smtpUser = String(form.get('smtp_user') ?? '').trim();
    const smtpPass = form.get('smtp_pass');
    const smtpFromEmail = String(form.get('smtp_from_email') ?? '').trim();
    const smtpFromName = String(form.get('smtp_from_name') ?? '').trim();
    const smtpEncryption = String(form.get('smtp_encryption') ?? 'starttls').trim();

    // 第三方 OAuth 登录设置字段
    const googleAuthEnabled = form.has('google_auth_enabled');
    const googleClientId = String(form.get('google_client_id') ?? '').trim();
    const googleClientSecret = form.get('google_client_secret');

    const githubAuthEnabled = form.has('github_auth_enabled');
    const githubClientId = String(form.get('github_client_id') ?? '').trim();
    const githubClientSecret = form.get('github_client_secret');

    // 校验语义与原型 sysValidate 一致（后端另有同语义硬校验兜底）。
    if (!siteName) return fail(422, { message: '站点名称必填' });
    if (!defaultLang) return fail(422, { message: '默认语言必填' });
    if (!/^https?:\/\/\S+\.\S+/.test(publicSource) || publicSource.length > 200) {
      return fail(422, { message: '公开源需为有效的 http(s):// 地址（≤200 字符）' });
    }
    if (!Number.isInteger(apiRateLimit) || apiRateLimit < 1 || apiRateLimit > 10000) {
      return fail(422, { message: 'API 限流需为 1 - 10000 的整数' });
    }
    if (!Number.isInteger(smtpPort) || smtpPort < 1 || smtpPort > 65535) {
      return fail(422, { message: 'SMTP 端口需为 1 - 65535 的整数' });
    }
    if (smtpFromEmail && (!smtpFromEmail.includes('@') || /\s/.test(smtpFromEmail))) {
      return fail(422, { message: '发件人邮箱格式不正确' });
    }
    if (!['none', 'starttls', 'tls'].includes(smtpEncryption)) {
      return fail(422, { message: 'SMTP 加密模式需为 none, starttls 或 tls' });
    }
    // 站点文案长度上限与后端 PATCH 校验一致（maxlength 双保险）。
    const copyLimits: Array<[string, string, number]> = [
      ['站点描述', siteDescription, 200],
      ['登录页眉题', loginEyebrow, 60],
      ['登录页标题', loginTitle, 100],
      ['登录页说明', loginSubtitle, 200],
      ['注册页眉题', registerEyebrow, 60],
      ['注册页标题', registerTitle, 100],
      ['注册页说明', registerSubtitle, 200]
    ];
    for (const [label, value, max] of copyLimits) {
      if ([...value].length > max) {
        return fail(422, { message: `${label}不能超过 ${max} 个字符` });
      }
    }
    // 管理写操作必填原因（后端 required_reason 同策略，缺失必 400）。
    if (!reason) return fail(422, { message: '操作原因必填（写入审计日志）' });

    const patch: AdminSettingsResult['settings'] & { reason?: string } = {
      open_registration: form.has('open_registration'),
      email_verification: form.has('email_verification'),
      anonymous_replies: form.has('anonymous_replies'),
      public_rss: form.has('public_rss'),
      maintenance_mode: form.has('maintenance_mode'),
      site_name: siteName,
      default_lang: defaultLang,
      public_source: publicSource,
      api_rate_limit: apiRateLimit,
      site_description: siteDescription,
      login_eyebrow: loginEyebrow,
      login_title: loginTitle,
      login_subtitle: loginSubtitle,
      register_eyebrow: registerEyebrow,
      register_title: registerTitle,
      register_subtitle: registerSubtitle,
      smtp_enabled: smtpEnabled,
      smtp_host: smtpHost,
      smtp_port: smtpPort,
      smtp_user: smtpUser,
      smtp_from_email: smtpFromEmail,
      smtp_from_name: smtpFromName,
      smtp_encryption: smtpEncryption,
      google_auth_enabled: googleAuthEnabled,
      google_client_id: googleClientId,
      github_auth_enabled: githubAuthEnabled,
      github_client_id: githubClientId,
      reason
    };

    if (typeof smtpPass === 'string' && smtpPass.length > 0) {
      patch.smtp_pass = smtpPass;
    }
    if (typeof googleClientSecret === 'string' && googleClientSecret.length > 0) {
      patch.google_client_secret = googleClientSecret;
    }
    if (typeof githubClientSecret === 'string' && githubClientSecret.length > 0) {
      patch.github_client_secret = githubClientSecret;
    }

    try {
      const result = await authedPatch<AdminSettingsResult>(
        cookies,
        '/api/v1/admin/settings',
        patch,
        { 'If-Match': String(version) },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { message: '设置已保存' };
      }
      if (result.status === 409) {
        return fail(409, {
          conflict: true,
          message: `版本冲突：${result.message}（设置已被其他人修改，请刷新后重试）`
        });
      }
      return fail(result.status, { message: result.message, requestId: result.requestId });
    } catch {
      return fail(503, { message: '保存失败，请稍后重试' });
    }
  }
};
