// M09-UI-01/03：AI 能力与同意管理（/ai）。
//
// - load：GET /api/v1/ai/capabilities（认证）；401 → 登录；409 feature_disabled
//   或 501 not_implemented → disabled 态（默认关闭说明）；403 → 无权限态；
// - revoke action：DELETE /api/v1/ai/consent（按 purpose 撤回，撤回后停止新任务）；
// - Provider 状态脱敏：Secret 只显示 secret_configured 布尔，绝不出现在
//   SSR HTML / hydration payload。
import { fail, isRedirect, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { authedDeleteBody, getAuthed } from '$lib/api/server';
import type {
  AiCapabilities,
  AiConsentView,
  AiProviderStatus
} from '$lib/api/types';

export type AiPageState = 'disabled' | 'forbidden' | 'ok';

export interface AiPageData {
  state: AiPageState;
  /** 未启用时给用户的关闭说明（Feature Flag 默认关闭）。 */
  disabledMessage: string | null;
  capabilities: AiCapabilities | null;
  error: string | null;
}

export interface AiPageActionData {
  message?: string;
  requestId?: string | null;
}

/** 能力投影白名单：只保留展示字段；任何 Secret/内部字段不进输出。 */
function pickCapabilities(raw: unknown): AiCapabilities {
  const r = (raw ?? {}) as Record<string, unknown>;
  const out: AiCapabilities = { enabled: r.enabled === true };
  if (typeof r.data_mode === 'string') out.data_mode = r.data_mode;
  if (Array.isArray(r.purposes)) {
    out.purposes = r.purposes.filter((p) => typeof p === 'string') as string[];
  }
  if (r.synchronous === true) out.synchronous = true;
  if (r.admin_forbidden === true) out.admin_forbidden = true;
  if (Array.isArray(r.providers)) {
    out.providers = r.providers
      .map((p): AiProviderStatus | null => pickProvider(p))
      .filter((p): p is AiProviderStatus => p !== null);
  }
  if (Array.isArray(r.consents)) {
    out.consents = r.consents
      .map((c): AiConsentView | null => pickConsent(c))
      .filter((c): c is AiConsentView => c !== null);
  }
  return out;
}

function pickProvider(raw: unknown): AiProviderStatus | null {
  if (!raw || typeof raw !== 'object') return null;
  const p = raw as Record<string, unknown>;
  if (typeof p.id !== 'string' || !p.id) return null;
  const out: AiProviderStatus = { id: p.id };
  if (typeof p.name === 'string') out.name = p.name;
  if (typeof p.secret_configured === 'boolean') out.secret_configured = p.secret_configured;
  if (typeof p.available === 'boolean') out.available = p.available;
  if (Array.isArray(p.purposes)) {
    out.purposes = p.purposes.filter((x) => typeof x === 'string') as string[];
  }
  if (typeof p.model === 'string') out.model = p.model;
  if (typeof p.retention === 'string') out.retention = p.retention;
  if (typeof p.training === 'string') out.training = p.training;
  if (typeof p.region === 'string') out.region = p.region;
  return out;
}

function pickConsent(raw: unknown): AiConsentView | null {
  if (!raw || typeof raw !== 'object') return null;
  const c = raw as Record<string, unknown>;
  if (typeof c.provider_id !== 'string' || typeof c.purpose !== 'string') return null;
  const out: AiConsentView = {
    provider_id: c.provider_id,
    purpose: c.purpose,
    data_mode: typeof c.data_mode === 'string' ? c.data_mode : 'full_with_consent',
    disclosure_version: typeof c.disclosure_version === 'number' ? c.disclosure_version : 0
  };
  if (typeof c.provider_name === 'string') out.provider_name = c.provider_name;
  if (typeof c.disclosure_hash === 'string') out.disclosure_hash = c.disclosure_hash;
  if (typeof c.granted_at === 'number') out.granted_at = c.granted_at;
  if (typeof c.revoked_at === 'number') out.revoked_at = c.revoked_at;
  return out;
}

export const load: PageServerLoad = async ({ cookies, request }): Promise<AiPageData> => {
  const requestId = request.headers.get('x-request-id');
  const result = await getAuthed<unknown>(cookies, '/api/v1/ai/capabilities', requestId);
  if (!result.ok) {
    if (result.status === 401) throw redirect(303, '/login');
    if (result.status === 403) {
      return { state: 'forbidden', disabledMessage: null, capabilities: null, error: result.message } satisfies AiPageData;
    }
    // 409 feature_disabled / 501 not_implemented / 其它 → 关闭态降级。
    return {
      state: 'disabled',
      disabledMessage:
        'AI 能力当前未开放（默认关闭）。你的内容不会被发送给任何外部 AI 提供商，普通发帖与审核不受影响。',
      capabilities: null,
      error: result.message
    } satisfies AiPageData;
  }
  return {
    state: 'ok',
    disabledMessage: null,
    capabilities: pickCapabilities(result.data),
    error: null
  } satisfies AiPageData;
};

export const actions: Actions = {
  revoke: async ({ request, cookies }) => {
    const form = await request.formData();
    const providerId = String(form.get('provider_id') ?? '').trim();
    const purpose = String(form.get('purpose') ?? '').trim();
    const disclosureVersion = Number(form.get('disclosure_version') ?? 0);
    const disclosureHashValue = String(form.get('disclosure_hash') ?? '').trim();
    if (!providerId || !purpose) {
      return fail(422, { message: '缺少同意记录标识' } satisfies AiPageActionData);
    }
    if (!Number.isInteger(disclosureVersion) || disclosureVersion < 1) {
      return fail(422, { message: '同意版本缺失或无效，请刷新后重试' } satisfies AiPageActionData);
    }
    try {
      const result = await authedDeleteBody<{ ok?: boolean }>(
        cookies,
        '/api/v1/ai/consent',
        {
          provider_id: providerId,
          purpose,
          data_mode: 'full_with_consent',
          disclosure_version: disclosureVersion,
          disclosure_hash: disclosureHashValue
        },
        request.headers.get('x-request-id'),
        { 'Idempotency-Key': `ai-revoke-${providerId}-${purpose}-${disclosureVersion}` }
      );
      if (result.ok) {
        return { message: '已撤回该用途的 AI 同意' } satisfies AiPageActionData;
      }
      return fail(result.status, {
        message: result.message,
        requestId: result.requestId
      } satisfies AiPageActionData);
    } catch (e) {
      if (isRedirect(e)) throw e;
      return fail(503, { message: '撤回失败，请稍后重试' } satisfies AiPageActionData);
    }
  }
};
