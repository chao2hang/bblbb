// 站点上传类型策略镜像（与管理后台「文件存储 → 上传类型」配置联动）。
//
// 后端能力白名单（backend/src/storage/upload.rs ALLOWED_MEDIA_TYPES）是安全
// 下限；管理后台按类目（image/pdf/text/office/av）开关生成子集，并经
// GET /api/v1/attachments 的 `quota.allowed_media_types` 投影到前端。
// 本模块负责：扩展名归一化（浏览器 MIME 不可靠）→ 媒体类型 → 策略校验。

/** 类目显示名（管理后台/错误提示用）。 */
export const UPLOAD_CATEGORY_LABELS: Record<string, string> = {
  image: '图片（JPG/PNG/WebP/GIF/AVIF）',
  pdf: 'PDF 文档',
  text: '文本文件（TXT/MD/CSV/JSON/XML/LOG）',
  office: 'Office 文档（DOCX/XLSX/PPTX）',
  av: '音视频（MP4/M4V/MP3/WebM）'
};

/** 类目 → 成员媒体类型（与后端 category_for_media_type 镜像）。 */
export const CATEGORY_MEDIA_TYPES: Record<string, string[]> = {
  image: ['image/jpeg', 'image/png', 'image/webp', 'image/gif', 'image/avif'],
  pdf: ['application/pdf'],
  text: [
    'text/plain',
    'text/csv',
    'text/markdown',
    'application/json',
    'application/xml',
    'text/xml'
  ],
  office: [
    'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
    'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
    'application/vnd.openxmlformats-officedocument.presentationml.presentation'
  ],
  av: ['video/mp4', 'audio/mpeg', 'video/webm']
};

/** 后端能力白名单（全类目并集）。 */
export const ALL_UPLOAD_MEDIA_TYPES: string[] = Object.values(CATEGORY_MEDIA_TYPES).flat();

/** 媒体类型 → 类目。 */
export function categoryForMediaType(mediaType: string): string | null {
  for (const [category, types] of Object.entries(CATEGORY_MEDIA_TYPES)) {
    if (types.includes(mediaType)) return category;
  }
  return null;
}

/** 扩展名 → 媒体类型（权威归一化；浏览器 file.type 仅作兜底）。 */
const EXTENSION_MEDIA_TYPES: Record<string, string> = Object.fromEntries(
  Object.entries({
    jpg: 'image/jpeg',
    jpeg: 'image/jpeg',
    jpe: 'image/jpeg',
    png: 'image/png',
    webp: 'image/webp',
    gif: 'image/gif',
    avif: 'image/avif',
    pdf: 'application/pdf',
    txt: 'text/plain',
    text: 'text/plain',
    log: 'text/plain',
    md: 'text/plain',
    markdown: 'text/plain',
    csv: 'text/csv',
    json: 'application/json',
    xml: 'application/xml',
    docx: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
    xlsx: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
    pptx: 'application/vnd.openxmlformats-officedocument.presentationml.presentation',
    mp4: 'video/mp4',
    m4v: 'video/mp4',
    mp3: 'audio/mpeg',
    webm: 'video/webm'
  })
);

/** 按文件名扩展名解析媒体类型；未知扩展名返回 null。 */
export function extensionMediaType(filename: string): string | null {
  const lower = filename.toLowerCase();
  const dot = lower.lastIndexOf('.');
  if (dot <= 0) return null;
  return EXTENSION_MEDIA_TYPES[lower.slice(dot + 1)] ?? null;
}

/** 媒体类型是否在策略内（`allowed` 为 null/undefined = 未获取，按全能力白名单；空数组 = 站点未开放）。 */
export function isAllowedMediaType(mediaType: string, allowed?: string[] | null): boolean {
  if (allowed !== null && allowed !== undefined && allowed.length === 0) return false;
  const list = allowed && allowed.length > 0 ? allowed : ALL_UPLOAD_MEDIA_TYPES;
  return list.includes(mediaType);
}

/**
 * 解析上传声明的媒体类型：扩展名归一化优先（权威），浏览器 file.type 仅在
 * 归一化失败且在策略内时兜底。返回 null = 站点策略不支持该文件。
 */
export function resolveUploadMediaType(file: File, allowed?: string[] | null): string | null {
  const byExt = extensionMediaType(file.name);
  if (byExt) return isAllowedMediaType(byExt, allowed) ? byExt : null;
  const browserType = (file.type || '').toLowerCase();
  if (browserType && browserType !== 'application/octet-stream') {
    return isAllowedMediaType(browserType, allowed) ? browserType : null;
  }
  return null;
}

/** 友好的支持类型提示（按当前策略生成；显式空数组 = 站点未开放任何类型）。 */
export function uploadTypeHint(allowed?: string[] | null): string {
  if (allowed && allowed.length === 0) return '当前站点未开放任何上传类型';
  const list = allowed && allowed.length > 0 ? allowed : ALL_UPLOAD_MEDIA_TYPES;
  const categories = [...new Set(list.map((t) => categoryForMediaType(t)).filter(Boolean))] as string[];
  if (categories.length === 0) return '当前站点未开放任何上传类型';
  return categories.map((c) => UPLOAD_CATEGORY_LABELS[c] ?? c).join('、');
}

// ── 策略加载（模块级缓存 60s）────────────────────────────────────────────

let cachedAllowed: string[] | null = null;
let cachedAt = 0;
let inflight: Promise<string[] | null> | null = null;
const CACHE_TTL_MS = 60_000;

/** 策略当前缓存值（可能为 null = 未加载/过期，按全量白名单处理）。 */
export function cachedUploadPolicy(): string[] | null {
  if (cachedAllowed && Date.now() - cachedAt < CACHE_TTL_MS) return cachedAllowed;
  return null;
}

/** 拉取站点上传类型策略（GET /attachments 的 quota 投影；失败静默降级）。 */
export async function loadUploadPolicy(fetchFn: typeof fetch = fetch): Promise<string[] | null> {
  const cached = cachedUploadPolicy();
  if (cached) return cached;
  if (inflight) return inflight;
  inflight = (async () => {
    try {
      const res = await fetchFn('/api/v1/attachments', { credentials: 'same-origin' });
      if (!res.ok) return cachedAllowed;
      const data = (await res.json()) as { quota?: { allowed_media_types?: string[] } | null };
      const list = data.quota?.allowed_media_types;
      if (Array.isArray(list) && list.length > 0) {
        cachedAllowed = list;
        cachedAt = Date.now();
      }
      return cachedAllowed;
    } catch {
      return cachedAllowed;
    } finally {
      inflight = null;
    }
  })();
  return inflight;
}

/** 测试用：清空策略缓存。 */
export function resetUploadPolicyCache(): void {
  cachedAllowed = null;
  cachedAt = 0;
  inflight = null;
}
