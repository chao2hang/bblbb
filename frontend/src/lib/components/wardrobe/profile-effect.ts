import type { CosmeticDefStyle } from '$lib/api/types';
import steamBackgrounds from '$lib/data/steam-profile-backgrounds.json';

const ANIMATION_CLASSES = new Set(['shimmer', 'aurora', 'flow', 'pulse', 'scanline']);
const TEXTURES = new Set(['sparkle', 'dark_stars', 'grid', 'dots', 'aurora', 'matrix', 'waves', 'snow', 'bubbles']);

function safeColor(value: unknown, fallback: string): string {
  return typeof value === 'string' && /^#[0-9a-fA-F]{6}$/.test(value) ? value : fallback;
}

function boundedNumber(value: unknown, min: number, max: number, fallback: number): number {
  return typeof value === 'number' && Number.isFinite(value)
    ? Math.min(max, Math.max(min, value))
    : fallback;
}

function textureBackground(texture: string, base: string, accent: string): { value: string; size: string } {
  switch (texture) {
    case 'grid':
      return {
        value: `linear-gradient(${accent}33 1px, transparent 1px), linear-gradient(90deg, ${accent}33 1px, transparent 1px), ${base}`,
        size: '24px 24px, 24px 24px, auto'
      };
    case 'dots':
      return {
        value: `radial-gradient(circle, ${accent}66 1.2px, transparent 1.5px), ${base}`,
        size: '18px 18px, auto'
      };
    case 'dark_stars':
      return {
        value: `radial-gradient(circle at 30% 40%, ${accent}99, transparent 55%), radial-gradient(circle at 70% 65%, ${accent}66, transparent 58%), ${base}`,
        size: 'auto'
      };
    case 'aurora':
      return {
        value: `radial-gradient(ellipse at 20% 0%, ${accent}99, transparent 60%), radial-gradient(ellipse at 80% 100%, ${accent}66, transparent 60%), ${base}`,
        size: '150% 150%, 150% 150%, auto'
      };
    case 'matrix':
      return {
        value: `linear-gradient(${accent}33 1px, transparent 1px), linear-gradient(90deg, ${accent}22 1px, transparent 1px), ${base}`,
        size: '24px 24px, 24px 24px, auto'
      };
    case 'waves':
      return {
        value: `radial-gradient(ellipse at 50% 120%, ${accent}99 0 30%, transparent 70%), ${base}`,
        size: 'auto'
      };
    case 'snow':
      return {
        value: `radial-gradient(circle, ${accent}cc 1.2px, transparent 1.6px), radial-gradient(circle, ${accent}88 2px, transparent 2.6px), ${base}`,
        size: '40px 40px, 70px 70px, auto'
      };
    case 'bubbles':
      return {
        value: `radial-gradient(circle at 35% 45%, ${accent}55 12px, transparent 13px), radial-gradient(circle at 75% 65%, ${accent}44 20px, transparent 21px), ${base}`,
        size: '90px 90px, 120px 120px, auto'
      };
    case 'sparkle':
    default:
      return {
        value: `radial-gradient(circle at 28% 30%, ${accent}aa, transparent 55%), radial-gradient(circle at 72% 72%, ${accent}77, transparent 60%), ${base}`,
        size: 'auto'
      };
  }
}

/** Return the fixed class used by the live cover animation layer. */
export function profileEffectClass(
  style: CosmeticDefStyle | null | undefined,
  token?: string | null
): string {
  const animation = style?.animate;
  if (typeof animation === 'string' && ANIMATION_CLASSES.has(animation)) {
    return `profile-cover--${animation}`;
  }
  if (token === 'sparkle' || token === 'dark_stars') return `effect-${token}`;
  return '';
}

/**
 * Convert the validated profile-effect projection into safe presentation CSS.
 * Colors, texture names, duration and opacity are all allowlisted/clamped here;
 * arbitrary style.css/style.js fields are intentionally ignored.
 */
export function profileEffectStyle(style: CosmeticDefStyle | null | undefined): string {
  if (!style || style.mode !== 'profile') return '';

  const base = safeColor(style.baseColor, '#101827');
  const accent = safeColor(style.accentColor, '#8b5cf6');
  const texture = typeof style.texture === 'string' && TEXTURES.has(style.texture)
    ? style.texture
    : 'sparkle';
  const durationMs = Math.round(boundedNumber(style.durationMs, 800, 30_000, 5_000));
  const opacity = boundedNumber(style.opacity, 10, 100, 100) / 100;
  const background = textureBackground(texture, base, accent);

  return [
    `--profile-effect-accent:${accent}`,
    `--profile-effect-opacity:${opacity}`,
    `--profile-effect-duration:${durationMs}ms`,
    `background:${background.value}`,
    `background-size:${background.size}`
  ].join(';');
}

type SteamBackgroundEntry = {
  id: string;
  defid: number;
  appid: number;
  name: string;
  image: string;
  webm?: string | null;
  mp4?: string | null;
};

export type ProfileEffectMediaStyle = CosmeticDefStyle & {
  image?: string | null;
  url?: string | null;
  webm?: string | null;
  mp4?: string | null;
};

export interface ProfileCoverMedia {
  /** Poster image URL（本地打包资源优先）。 */
  image: string | null;
  /** 本地海报缺失时的 CDN 静态回退。 */
  fallbackSrc: string | null;
  /** 本地 WebM 全景动画。 */
  webm: string | null;
  /** 本地 MP4 全景动画。 */
  mp4: string | null;
}

const STEAM_CDN_ITEMS = 'https://shared.fastly.steamstatic.com/community_assets/images/items';

function fileBasename(value: string): string {
  return value.split(/[/?#]/).pop() ?? value;
}

function localPanorama(filename: string): string {
  return `/api/v1/steam-assets/backgrounds/${filename}`;
}

function isHttpUrl(value: string): boolean {
  return value.startsWith('http://') || value.startsWith('https://');
}

/**
 * Resolve an equipped profile effect into cover media.
 *
 * Steam panorama backgrounds are bundled under /cosmetics/backgrounds; custom
 * definitions often carry only colors, so the catalog is also matched by the
 * equipped effect name (and legacy ids) to recover the poster/video files.
 * Unknown effects fall back to whatever media the style itself carries.
 */
export function resolveSteamPanoramaMedia(
  style: ProfileEffectMediaStyle | null | undefined,
  effectId?: string | null,
  effectName?: string | null
): ProfileCoverMedia | null {
  const token = typeof effectId === 'string' && effectId ? effectId : null;
  const name = typeof effectName === 'string' ? effectName.trim().toLowerCase() : '';
  const styleImage =
    typeof style?.image === 'string' && style.image ? style.image : null;
  const styleUrl = typeof style?.url === 'string' && style.url ? style.url : null;

  const catalog = (steamBackgrounds as SteamBackgroundEntry[]).find(
    (item) =>
      item.id === token ||
      String(item.defid) === token ||
      (!!styleImage && fileBasename(styleImage) === fileBasename(item.image)) ||
      (!!name && item.name.trim().toLowerCase() === name)
  );

  if (catalog) {
    const styleWebm =
      typeof style?.webm === 'string' && style.webm ? style.webm : null;
    const styleMp4 =
      typeof style?.mp4 === 'string' && style.mp4 ? style.mp4 : null;
    const image =
      styleImage && isHttpUrl(styleImage)
        ? styleImage
        : localPanorama(styleImage ?? catalog.image);
    return {
      image,
      fallbackSrc: `${STEAM_CDN_ITEMS}/${catalog.appid}/${catalog.image}`,
      webm: styleWebm && isHttpUrl(styleWebm)
        ? styleWebm
        : catalog.webm
          ? (isHttpUrl(catalog.webm) ? catalog.webm : localPanorama(catalog.webm))
          : null,
      mp4: styleMp4 && isHttpUrl(styleMp4)
        ? styleMp4
        : catalog.mp4
          ? (isHttpUrl(catalog.mp4) ? catalog.mp4 : localPanorama(catalog.mp4))
          : null
    };
  }

  const image = styleImage
    ? isHttpUrl(styleImage)
      ? styleImage
      : localPanorama(styleImage)
    : styleUrl && (isHttpUrl(styleUrl) || styleUrl.startsWith('/'))
      ? styleUrl
      : null;
  const webm = style?.webm
    ? isHttpUrl(style.webm) || style.webm.startsWith('/')
      ? style.webm
      : localPanorama(style.webm)
    : null;
  const mp4 = style?.mp4
    ? isHttpUrl(style.mp4) || style.mp4.startsWith('/')
      ? style.mp4
      : localPanorama(style.mp4)
    : null;
  if (!image && !webm && !mp4) return null;
  return { image, fallbackSrc: null, webm, mp4 };
}

