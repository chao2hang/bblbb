// M14-SEO-01：统一 title/description/canonical/OG/Twitter/JSON-LD 安全生成器。
//
// 安全约束（docs/FRONTEND.md §SEO）：
//   - 所有属性值在渲染层由 Svelte 转义；本模块在构造期再做一层防御：
//     截断长度、拒绝非 http(s) 的 URL（javascript:/data: 等一律丢弃）、
//     canonical/og:url 仅接受绝对 http(s) URL；
//   - JSON-LD 复用 escapeJsonLdScript 把 `</script` 转义为 `<\/script`
//     （合法 JSON 转义，语义不变），配合 JsonLd.svelte 白名单注入；
//   - 隐藏/未发布/审核中/删除/封禁内容统一 noindex（robots meta），
//     配合根 layout 的 Cache-Control: private, no-store（M00-FRONTEND-06）。
//
// 本模块是纯函数库（可在 SSR 与测试中运行），不依赖 Svelte 运行时。

import { escapeJsonLdScript } from '$lib/components/jsonLd';

export interface SeoOg {
  /** og:type：article/website/profile/…（默认 website）。 */
  type?: string;
  /** 绝对 http(s) 图片 URL（不合法则整体丢弃）。 */
  image?: string;
  siteName?: string;
}

export interface SeoTwitter {
  card?: 'summary' | 'summary_large_image' | 'app';
  /** 绝对 http(s) 图片 URL。 */
  image?: string;
}

export interface SeoInput {
  title: string;
  description?: string;
  /** 绝对 canonical URL；缺省时由 Seo.svelte 用 page.url 派生。 */
  canonical?: string;
  /** 隐藏内容 / 非公开状态 → 输出 robots noindex。 */
  noindex?: boolean;
  og?: SeoOg;
  twitter?: SeoTwitter;
  /** 已 JSON 可序列化的对象（Article/Profile/BreadcrumbList/WebSite…）。 */
  jsonLd?: Record<string, unknown> | unknown[] | null;
}

export interface SeoMeta {
  /** 已转义安全的标题文本（≤ 60 字符 + “ — BBLBB” 站点后缀）。 */
  title: string;
  description: string | null;
  canonical: string | null;
  /** robots 指令串；公开可索引页为 null（不输出，允许默认抓取）。 */
  robots: string | null;
  ogType: string;
  ogTitle: string;
  ogDescription: string | null;
  ogUrl: string | null;
  ogImage: string | null;
  ogSiteName: string | null;
  twitterCard: string;
  twitterImage: string | null;
  /** JSON.stringify 后的字符串（JsonLd.svelte 负责转义注入）。 */
  jsonLd: string | null;
}

const TITLE_MAX = 60;
const DESCRIPTION_MAX = 160;
const SITE_NAME = 'BBLBB';

/** 只接受绝对 http(s) URL；其余（javascript:、data:、// 协议相对、相对路径）一律 null。 */
export function safeHttpUrl(value: string | null | undefined): string | null {
  if (!value) return null;
  try {
    const url = new URL(value);
    if (url.protocol !== 'http:' && url.protocol !== 'https:') return null;
    return url.href;
  } catch {
    return null;
  }
}

function truncate(value: string, max: number): string {
  if (value.length <= max) return value;
  return `${value.slice(0, max - 1).trimEnd()}…`;
}

/** 清理可能携带控制字符/HTML 片段的长文本（内容层转义仍由渲染完成）。 */
function sanitizeText(value: string | null | undefined, max: number): string | null {
  if (!value) return null;
  // 去掉控制字符（保留换行不进入 meta 的截断路径）。
  const cleaned = value.replace(/[\u0000-\u0008\u000B\u000C\u000E-\u001F\u007F]/g, ' ');
  const singleLine = cleaned.replace(/\s+/g, ' ').trim();
  return truncate(singleLine, max);
}

export const ROBOTS_HIDDEN = 'noindex, noarchive, nofollow';

export function buildSeo(input: SeoInput): SeoMeta {
  const baseTitle = sanitizeText(input.title, TITLE_MAX) ?? SITE_NAME;
  const title = baseTitle.endsWith(SITE_NAME) ? baseTitle : `${baseTitle} — ${SITE_NAME}`;
  const description = sanitizeText(input.description, DESCRIPTION_MAX);
  const canonical = safeHttpUrl(input.canonical);
  const ogImage = safeHttpUrl(input.og?.image);
  const twitterImage = safeHttpUrl(input.twitter?.image);
  const ogType = input.og?.type ?? 'website';

  let jsonLd: string | null = null;
  if (input.jsonLd) {
    try {
      const serialized = JSON.stringify(input.jsonLd);
      // escapeJsonLdScript 阻止任何字符串字段以 `</script>` 提前闭合标签。
      jsonLd = escapeJsonLdScript(serialized);
    } catch {
      jsonLd = null; // 循环引用/非序列化对象 → 丢弃而非报错
    }
  }

  return {
    title,
    description,
    canonical,
    robots: input.noindex ? ROBOTS_HIDDEN : null,
    ogType,
    ogTitle: baseTitle,
    ogDescription: description,
    ogUrl: canonical,
    ogImage,
    ogSiteName: sanitizeText(input.og?.siteName, 40),
    twitterCard: input.twitter?.card ?? (twitterImage ? 'summary_large_image' : 'summary'),
    twitterImage,
    jsonLd
  };
}

/**
 * 隐藏/未发布/审核中/删除/封禁内容的统一 SEO 输入（noindex）。
 * 与 M14-SEO-03 后端投影策略配套：只有公开已发布且可解锁的内容可索引。
 */
export function hiddenSeo(title: string, description?: string): SeoInput {
  return { title, description, noindex: true };
}
