// M10-UI：视频投影白名单与安全渲染决策（前端私有域逻辑）。
//
// 后端已按 docs/VIDEO-PLUGIN.md §3/§4 对受限内容省略 URL/播放器字段；
// 本模块对未知形状的接口响应做**防御性挑选**（字段缺失一律容忍）：
//  - 只复制渲染/创建必需字段；Provider Secret、签名播放 URL、Cookie/授权
//    头、HLS 密钥、任意内部字段一律丢弃；
//  - blocked/removed 状态即使后端误返回 URL 也强制丢弃（M10-UI-04 双保险，
//    前端绝不猜测或拼接 URL）；
//  - 渲染 URL 必须为 https 且不含 userinfo（VIDEO-PLUGIN.md §3）。
//
// 后端收敛出具体 schema 后，本模块塌缩为浅拷贝/契约类型转换，接口不变。

import type {
  VideoEmbedProvider,
  VideoEmbedStatus,
  VideoEmbedView,
  VideoProviderPoliciesView,
  VideoProviderPolicyView,
  VideoProviderStatusView,
  VideoResolveResult,
  VideoTargetType
} from '$lib/api/types';

const PROVIDERS: ReadonlySet<string> = new Set(['direct', 'hls', 'xigua']);
const STATUSES: ReadonlySet<string> = new Set(['pending', 'ready', 'blocked', 'error', 'removed']);
const TARGET_TYPES: ReadonlySet<string> = new Set(['post', 'comment']);
const LIMIT_HOSTS = 32;

export function isVideoProvider(value: unknown): value is VideoEmbedProvider {
  return typeof value === 'string' && PROVIDERS.has(value);
}

export function isVideoStatus(value: unknown): value is VideoEmbedStatus {
  return typeof value === 'string' && STATUSES.has(value);
}

export function isVideoTargetType(value: unknown): value is VideoTargetType {
  return typeof value === 'string' && TARGET_TYPES.has(value);
}

function str(raw: unknown): string | undefined {
  if (typeof raw !== 'string') return undefined;
  const s = raw.trim();
  return s.length > 0 ? s : undefined;
}

function num(raw: unknown): number | undefined {
  if (typeof raw !== 'number' || !Number.isFinite(raw)) return undefined;
  return raw;
}

function bool(raw: unknown): boolean | undefined {
  return typeof raw === 'boolean' ? raw : undefined;
}

/** 仅接受 https、无 userinfo 的 URL；其余返回 null（前端绝不渲染其它 scheme）。 */
export function safeHttpsUrl(raw: unknown): string | null {
  const s = str(raw);
  if (!s) return null;
  try {
    const url = new URL(s);
    if (url.protocol !== 'https:') return null;
    if (url.username !== '' || url.password !== '') return null;
    return url.toString();
  } catch {
    return null;
  }
}

function strList(raw: unknown): string[] {
  if (!Array.isArray(raw)) return [];
  const out: string[] = [];
  for (const item of raw) {
    const s = str(item);
    if (!s) continue;
    // 只允许 host 形状（域名/IP[:port]），拒绝路径/查询/空格/斜杠注入。
    if (/[\s/\u0000]/.test(s)) continue;
    if (out.length >= LIMIT_HOSTS) break;
    out.push(s);
  }
  return out;
}

function pickProviderStatus(raw: unknown): VideoProviderStatusView | null {
  if (!raw || typeof raw !== 'object') return null;
  const r = raw as Record<string, unknown>;
  const provider = str(r.provider);
  if (!provider) return null;
  const out: VideoProviderStatusView = { provider };
  const enabled = bool(r.enabled);
  const available = bool(r.available);
  if (enabled !== undefined) out.enabled = enabled;
  if (available !== undefined) out.available = available;
  return out;
}

/** 受限/已删除状态 → 强制丢弃全部渲染 URL（M10-UI-04：不渲染视频 URL 或
 *  播放器配置）。blocked/removed 之外的状态按后端投影原样保留。 */
function isSuppressedStatus(status: VideoEmbedStatus): boolean {
  return status === 'blocked' || status === 'removed';
}

/** GET /api/v1/video-embeds/{id} 投影白名单。 */
export function pickVideoEmbedView(raw: unknown): VideoEmbedView | null {
  if (!raw || typeof raw !== 'object') return null;
  const r = raw as Record<string, unknown>;
  if (typeof r.id !== 'string' || !r.id) return null;
  const provider = r.provider;
  const status = r.status;
  if (!isVideoProvider(provider) || !isVideoStatus(status)) return null;

  const out: VideoEmbedView = {
    id: r.id,
    provider,
    status,
    version: num(r.version) ?? 0,
    created_at: num(r.created_at) ?? 0,
    updated_at: num(r.updated_at) ?? 0
  };
  const suppressed = isSuppressedStatus(status);
  const mediaUrl = suppressed ? null : safeHttpsUrl(r.media_url);
  const officialUrl = suppressed ? null : safeHttpsUrl(r.official_url);
  const sourceUrl = suppressed ? null : safeHttpsUrl(r.source_url);
  const posterUrl = suppressed ? null : safeHttpsUrl(r.poster_url);
  const captionUrl = suppressed ? null : safeHttpsUrl(r.caption_url);
  const title = str(r.title);
  const mediaType = str(r.media_type);
  const duration = num(r.duration_seconds);
  const policyVersion = num(r.policy_version);
  const lastChecked = num(r.last_checked_at);

  if (mediaUrl) out.media_url = mediaUrl;
  if (officialUrl) out.official_url = officialUrl;
  if (sourceUrl) out.source_url = sourceUrl;
  if (posterUrl) out.poster_url = posterUrl;
  if (captionUrl) out.caption_url = captionUrl;
  if (title) out.title = title;
  if (mediaType) out.media_type = mediaType;
  if (duration !== undefined) out.duration_seconds = duration;
  if (policyVersion !== undefined) out.policy_version = policyVersion;
  if (lastChecked !== undefined) out.last_checked_at = lastChecked;
  return out;
}

/** POST /api/v1/video-embeds/resolve 响应投影白名单。
 *
 * `embeddable`：后端显式 false 为 false；后端未给该字段时按是否携带官方
 * 播放 URL（media_url/official_url）推断——两者都缺 → 只降级为外链卡片。
 * `degraded_reason` 只接受短稳定码（≤40 字符），绝不回显探测详情/原文。 */
export function pickVideoResolve(raw: unknown): VideoResolveResult | null {
  if (!raw || typeof raw !== 'object') return null;
  const r = raw as Record<string, unknown>;
  if (typeof r.resolution_id !== 'string' || !r.resolution_id) return null;

  const provider = isVideoProvider(r.provider) ? r.provider : null;
  const mediaUrl = safeHttpsUrl(r.media_url);
  const officialUrl = safeHttpsUrl(r.official_url);
  const sourceUrl = safeHttpsUrl(r.source_url);
  const posterUrl = safeHttpsUrl(r.poster_url);
  const hasPlayableUrl = Boolean(mediaUrl ?? officialUrl);
  const explicitlyDisabled = r.embeddable === false;
  const degradedReason = str(r.degraded_reason);

  const out: VideoResolveResult = {
    resolution_id: r.resolution_id,
    provider,
    embeddable: !explicitlyDisabled && (r.embeddable === true || hasPlayableUrl),
    degraded_reason:
      degradedReason && degradedReason.length <= 40 ? degradedReason : null
  };
  const title = str(r.title);
  const mediaType = str(r.media_type);
  const duration = num(r.duration_seconds);
  const policyVersion = num(r.policy_version);
  const checkedAt = num(r.checked_at);
  const providerStatus = pickProviderStatus(r.provider_status);

  if (mediaUrl) out.media_url = mediaUrl;
  if (officialUrl) out.official_url = officialUrl;
  if (sourceUrl) out.source_url = sourceUrl;
  if (posterUrl) out.poster_url = posterUrl;
  if (title) out.title = title;
  if (mediaType) out.media_type = mediaType;
  if (duration !== undefined) out.duration_seconds = duration;
  if (policyVersion !== undefined) out.policy_version = policyVersion;
  if (checkedAt !== undefined) out.checked_at = checkedAt;
  if (providerStatus) out.provider_status = providerStatus;
  return out;
}

/** GET /api/v1/admin/video/policies 投影白名单（Secret/内部字段不进 SSR）。 */
export function pickVideoPolicies(raw: unknown): VideoProviderPoliciesView {
  const r = (raw ?? {}) as Record<string, unknown>;
  const out: VideoProviderPoliciesView = { items: [] };
  const enabled = bool(r.enabled);
  const version = num(r.version);
  if (enabled !== undefined) out.enabled = enabled;
  if (version !== undefined) out.version = version;
  if (Array.isArray(r.items)) {
    for (const item of r.items) {
      const picked = pickVideoPolicy(item);
      if (picked) out.items.push(picked);
    }
  } else if (Array.isArray(r.policies)) {
    for (const item of r.policies) {
      const picked = pickVideoPolicy(item);
      if (picked) out.items.push(picked);
    }
  }
  return out;
}

/** 单个 Provider 策略投影。 */
export function pickVideoPolicy(raw: unknown): VideoProviderPolicyView | null {
  if (!raw || typeof raw !== 'object') return null;
  const r = raw as Record<string, unknown>;
  if (!isVideoProvider(r.provider)) return null;
  const out: VideoProviderPolicyView = {
    provider: r.provider,
    enabled: bool(r.enabled) ?? false,
    allowed_hosts: strList(r.allowed_hosts),
    embed_hosts: strList(r.embed_hosts),
    allowed_media_types: strList(r.allowed_media_types),
    policy_version: num(r.policy_version) ?? num(r.version) ?? 0
  };
  const updatedAt = num(r.updated_at);
  if (updatedAt !== undefined) out.updated_at = updatedAt;
  const optionalNumbers: Array<[string, string]> = [
    ['max_duration_seconds', 'max_duration_seconds'],
    ['max_bytes', 'max_bytes'],
    ['max_redirects', 'max_redirects'],
    ['hls_max_depth', 'hls_max_depth'],
    ['hls_max_segments', 'hls_max_segments'],
    ['hls_max_bytes', 'hls_max_bytes'],
    ['timeout_ms', 'timeout_ms']
  ];
  for (const [from, to] of optionalNumbers) {
    const v = num(r[from]);
    if (v !== undefined) (out as unknown as Record<string, unknown>)[to] = v;
  }
  return out;
}
