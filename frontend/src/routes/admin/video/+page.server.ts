// M10-UI-06：管理端视频——逐 Provider 策略配置、测试、停用与审计展示。
//
// - load：GET /api/v1/admin/video/policies（脱敏视图；server load 经
//   pickVideoPolicies 白名单挑选，Secret/内部字段不进 SSR HTML）。
// - save action：PATCH /api/v1/admin/video/policies/{provider}
//   （If-Match 版本 + reason 审计；enabled=false 即停用，立即影响新解析）。
// - test action：POST /api/v1/admin/video/policies/test
//   （按当前表单值测试候选配置；Idempotency-Key；reason 审计）。
// - 后端未实现（501）→ not_implemented 态；403 → forbidden；不影响核心论坛。
import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedPatch, authedPost, getAuthed } from '$lib/api/server';
import { newClientRequestId } from '$lib/api/client';
import { isVideoProvider, pickVideoPolicies } from '$lib/video/projection';
import type {
  VideoProviderPoliciesView,
  VideoProviderTestResult
} from '$lib/api/types';

export type AdminVideoLoadState = 'ok' | 'forbidden' | 'not_implemented' | 'error';

export interface VideoWhitelistConfig {
  enableEmbed: boolean;
  domainWhitelist: string;
  strictMode: string;
  fallbackMode: string;
}

export interface AdminVideoPageData {
  state: AdminVideoLoadState;
  policies: VideoProviderPoliciesView | null;
  error: string | null;
  /** 表单幂等键（SSR 生成，hydration 稳定）。 */
  clientRequestId: string;
  /** 视频白名单与安全配置（持久化存储）。 */
  whitelistConfig?: VideoWhitelistConfig;
}

export interface AdminVideoActionData {
  message?: string;
  error?: string;
  requestId?: string | null;
  conflict?: boolean;
  testResult?: VideoProviderTestResult | null;
  /** 本次操作针对的 Provider（用于把结果展示到对应卡片）。 */
  provider?: string | null;
  whitelistConfig?: VideoWhitelistConfig | null;
}

const DEFAULT_WHITELIST_CONFIG: VideoWhitelistConfig = {
  enableEmbed: true,
  domainWhitelist: 'youtube.com, bilibili.com, v.qq.com, youku.com',
  strictMode: 'strict',
  fallbackMode: 'safe_link'
};

export const load: PageServerLoad = async ({ cookies, request }): Promise<AdminVideoPageData> => {
  const requestId = request.headers.get('x-request-id');
  const clientRequestId = newClientRequestId();

  let whitelistConfig = { ...DEFAULT_WHITELIST_CONFIG };
  const rawWhitelist = cookies.get('bblbb_admin_video_whitelist');
  if (rawWhitelist) {
    try {
      const parsed = JSON.parse(rawWhitelist);
      whitelistConfig = { ...whitelistConfig, ...parsed };
    } catch {
      // 忽略损坏的 cookie
    }
  }

  const result = await getAuthed<unknown>(cookies, '/api/v1/admin/video/policies', requestId);
  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    if (result.status === 403) {
      return { state: 'forbidden', policies: null, error: result.message, clientRequestId, whitelistConfig } satisfies AdminVideoPageData;
    }
    if (result.status === 501) {
      return { state: 'not_implemented', policies: null, error: result.message, clientRequestId, whitelistConfig } satisfies AdminVideoPageData;
    }
    return { state: 'error', policies: null, error: result.message, clientRequestId, whitelistConfig } satisfies AdminVideoPageData;
  }
  return {
    state: 'ok',
    policies: pickVideoPolicies(result.data),
    error: null,
    clientRequestId,
    whitelistConfig
  } satisfies AdminVideoPageData;
};

function boolForm(form: FormData, key: string): boolean | undefined {
  const raw = form.get(key);
  if (raw === null) return undefined;
  return raw === 'on' || raw === 'true' || raw === '1';
}

function numOrNull(raw: unknown): number | null | undefined {
  if (raw === null || raw === undefined) return undefined;
  const s = String(raw).trim();
  if (s === '') return undefined;
  const v = Number(s);
  return Number.isFinite(v) ? v : null;
}

/** 解析 host 列表（逗号/换行分隔）；返回 null 表示格式非法（只允许
 *  域名/IP[:port]，拒绝空格/斜杠/控制字符——VIDEO-PLUGIN.md §3）。 */
function hostList(raw: unknown): string[] | null {
  if (raw === null || raw === undefined) return [];
  const s = String(raw).trim();
  if (s === '') return [];
  const items = s
    .split(/[\n,]/)
    .map((x) => x.trim())
    .filter(Boolean);
  for (const item of items) {
    if (/[\s/\u0000]/.test(item)) return null;
  }
  return items;
}

const NUMERIC_FIELDS = [
  'max_duration_seconds',
  'max_bytes',
  'max_redirects',
  'hls_max_depth',
  'hls_max_segments',
  'hls_max_bytes',
  'timeout_ms'
] as const;

/** 从表单构建策略变更（host 列表始终提交；数值仅提交非空字段）。 */
function buildChanges(form: FormData): { changes: Record<string, unknown>; error: string | null } {
  const changes: Record<string, unknown> = {};
  const enabled = boolForm(form, 'enabled');
  if (enabled !== undefined) changes.enabled = enabled;

  const allowedHosts = hostList(form.get('allowed_hosts'));
  if (allowedHosts === null) {
    return { changes: {}, error: 'allowed_hosts 含非法字符（host 只能是域名/IP[:port]）' };
  }
  const embedHosts = hostList(form.get('embed_hosts'));
  if (embedHosts === null) {
    return { changes: {}, error: 'embed_hosts 含非法字符（host 只能是域名/IP[:port]）' };
  }
  const mediaTypes = hostList(form.get('allowed_media_types'));
  if (mediaTypes === null) {
    return { changes: {}, error: 'allowed_media_types 含非法字符' };
  }
  changes.allowed_hosts = allowedHosts;
  changes.allow_hosts = allowedHosts;
  changes.embed_hosts = embedHosts;
  changes.allowed_media_types = mediaTypes;

  for (const field of NUMERIC_FIELDS) {
    const value = numOrNull(form.get(field));
    if (value !== undefined) changes[field] = value;
    else if (form.get(field) !== null) changes[field] = null; // 显式清空
  }
  // 契约对齐：映射至后端 video/state.rs 实际期望字段
  if (changes.max_bytes !== undefined) {
    changes.max_response_bytes = changes.max_bytes;
  }
  if (changes.hls_max_depth !== undefined) {
    changes.max_playlist_depth = changes.hls_max_depth;
  }
  if (changes.hls_max_segments !== undefined) {
    changes.max_segments = changes.hls_max_segments;
  }
  if (typeof changes.max_duration_seconds === 'number') {
    changes.max_duration_ms = changes.max_duration_seconds * 1000;
  } else if (changes.max_duration_seconds === null) {
    changes.max_duration_ms = null;
  }
  return { changes, error: null };
}

/** 读取公共表单字段并校验（provider/reason/expected_version）。 */
function readCommon(form: FormData): { provider: string; reason: string; expectedVersion: number } {
  const provider = String(form.get('provider') ?? '').trim();
  const reason = String(form.get('reason') ?? '').trim();
  const expectedVersion = Number(form.get('expected_version') ?? 0);
  return { provider, reason, expectedVersion };
}

export const actions: Actions = {
  save: async ({ request, cookies }) => {
    const form = await request.formData();
    const { provider, reason, expectedVersion } = readCommon(form);
    if (!isVideoProvider(provider)) {
      return fail(422, { message: 'Provider 标识无效', provider } satisfies AdminVideoActionData);
    }
    if (!reason) {
      return fail(422, { message: '操作原因必填（写审计）', provider } satisfies AdminVideoActionData);
    }
    if (!Number.isInteger(expectedVersion) || expectedVersion < 1) {
      return fail(422, { message: '策略版本缺失或无效，请刷新后重试', provider } satisfies AdminVideoActionData);
    }
    const { changes, error: buildError } = buildChanges(form);
    if (buildError) {
      return fail(422, { message: buildError, provider } satisfies AdminVideoActionData);
    }
    try {
      const result = await authedPatch<unknown>(
        cookies,
        `/api/v1/admin/video/policies/${encodeURIComponent(provider)}`,
        { ...changes, expected_version: expectedVersion, reason },
        { 'If-Match': String(expectedVersion) },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return {
          message: '视频 Provider 策略已保存（写审计），立即影响新解析',
          provider
        } satisfies AdminVideoActionData;
      }
      if (result.status === 409) {
        return fail(409, { conflict: true, message: `版本冲突：${result.message}`, provider } satisfies AdminVideoActionData);
      }
      return fail(result.status, { message: result.message, requestId: result.requestId, provider } satisfies AdminVideoActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '保存失败，请稍后重试', provider } satisfies AdminVideoActionData);
    }
  },
  test: async ({ request, cookies }) => {
    const form = await request.formData();
    const { provider, reason } = readCommon(form);
    if (!isVideoProvider(provider)) {
      return fail(422, { message: 'Provider 标识无效', provider } satisfies AdminVideoActionData);
    }
    if (!reason) {
      return fail(422, { message: '操作原因必填（写审计）', provider } satisfies AdminVideoActionData);
    }
    const { changes, error: buildError } = buildChanges(form);
    if (buildError) {
      return fail(422, { message: buildError, provider } satisfies AdminVideoActionData);
    }
    const clientRequestId = newClientRequestId();
    try {
      const result = await authedPost<VideoProviderTestResult>(
        cookies,
        '/api/v1/admin/video/policies/test',
        { provider, ...changes, reason, client_request_id: clientRequestId },
        request.headers.get('x-request-id'),
        { 'Idempotency-Key': `video-test-${clientRequestId}` }
      );
      if (result.ok) {
        return { testResult: result.data, provider } satisfies AdminVideoActionData;
      }
      return fail(result.status, {
        message: result.message,
        requestId: result.requestId,
        testResult: null,
        provider
      } satisfies AdminVideoActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '测试失败，请稍后重试', provider } satisfies AdminVideoActionData);
    }
  },
  'save-whitelist': async ({ request, cookies }) => {
    const form = await request.formData();
    const enableEmbed = form.get('enable_embed') === 'on' || form.get('enable_embed') === 'true' || form.has('enable_embed');
    const domainWhitelist = String(form.get('domain_whitelist') ?? '').trim();
    const strictMode = String(form.get('strict_mode') ?? 'strict').trim();
    const fallbackMode = String(form.get('fallback_mode') ?? 'safe_link').trim();

    const domains = domainWhitelist
      ? domainWhitelist.split(/[\n,]/).map((d) => d.trim()).filter(Boolean)
      : [];

    for (const d of domains) {
      if (/[\s/\u0000]/.test(d) || d.includes('://')) {
        return fail(422, {
          message: `域名「${d}」格式不正确，不能包含空格、斜杠或协议前缀（请仅填写域名，如 example.com）`,
          whitelistConfig: { enableEmbed, domainWhitelist, strictMode, fallbackMode }
        } satisfies AdminVideoActionData);
      }
    }

    const config: VideoWhitelistConfig = {
      enableEmbed,
      domainWhitelist,
      strictMode,
      fallbackMode
    };

    cookies.set('bblbb_admin_video_whitelist', JSON.stringify(config), {
      path: '/admin/video',
      maxAge: 60 * 60 * 24 * 30,
      httpOnly: false
    });

    return {
      message: '视频白名单与安全配置已保存',
      whitelistConfig: config
    } satisfies AdminVideoActionData;
  },
  'reset-default': async ({ request, cookies }) => {
    const form = await request.formData();
    const { provider, reason } = readCommon(form);
    if (!isVideoProvider(provider)) {
      return fail(422, { message: 'Provider 标识无效', provider } satisfies AdminVideoActionData);
    }
    const auditReason = reason || '恢复 Provider 默认策略';
    try {
      const curr = await getAuthed<{ policy: { version: number } }>(
        cookies,
        `/api/v1/admin/video/policies/${encodeURIComponent(provider)}`,
        request.headers.get('x-request-id')
      );
      const version = curr.ok ? (curr.data?.policy?.version ?? 1) : 1;
      const defaultChanges = {
        enabled: true,
        allow_hosts: (provider as string) === 'bilibili' ? ['bilibili.com'] : (provider as string) === 'youtube' ? ['youtube.com', 'youtu.be'] : [],
        reason: auditReason,
        expected_version: version
      };
      const result = await authedPatch(
        cookies,
        `/api/v1/admin/video/policies/${encodeURIComponent(provider)}`,
        defaultChanges,
        { 'If-Match': String(version) },
        request.headers.get('x-request-id')
      );
      if (result.ok) {
        return { message: `Provider「${provider}」已恢复默认策略（写审计）`, provider } satisfies AdminVideoActionData;
      }
      return fail(result.status, { message: result.message, provider } satisfies AdminVideoActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '恢复默认策略失败，请稍后重试', provider } satisfies AdminVideoActionData);
    }
  }
};
