// M10-UI：视频投影的动态 CSP 计算（VIDEO-PLUGIN.md §5）。
//
// 播放边界：Direct/HLS 由浏览器直连已验证 HTTPS 来源；西瓜视频仅使用官方
// iframe。本模块从后端投影计算页面需要放行的**精确** CSP 指令
// （frame-src/media-src/img-src/connect-src）：
//  - 只放行后端确认的官方来源 host，绝不包含签名播放 URL/内部字段；
//  - xigua → 官方 iframe host 进 frame-src；direct/hls → 媒体 host 进
//    media-src + connect-src；封面 host 进 img-src；
//  - blocked/removed/pending 无任何可放行来源（不渲染播放器）。
//
// 实际响应头由后端按启用 Provider 逐页生成（服务端唯一权威）；本模块是前端
// 的镜像计算：供组件判定"能否内联播放"（canRenderInlinePlayer）与测试断言
// 精确指令，前端绝不自行下发/放宽 CSP 头。

import type { VideoEmbedView } from '$lib/api/types';
import { safeHttpsUrl } from './projection';

export interface VideoCspDirectives {
  'frame-src': string[];
  'media-src': string[];
  'img-src': string[];
  'connect-src': string[];
}

/** 取 https URL 的 host（hostname[:port]）；非 https/userinfo/解析失败 → null。 */
export function hostOf(url: string): string | null {
  try {
    const parsed = new URL(url);
    if (parsed.protocol !== 'https:') return null;
    if (parsed.username !== '' || parsed.password !== '') return null;
    return parsed.host;
  } catch {
    return null;
  }
}

/** 按投影计算需要放行的精确 CSP 指令（frontend 镜像；后端为权威）。 */
export function videoCspDirectives(
  view: Pick<VideoEmbedView, 'provider' | 'status' | 'media_url' | 'official_url' | 'poster_url'>
): VideoCspDirectives {
  const directives: VideoCspDirectives = {
    'frame-src': [],
    'media-src': [],
    'img-src': [],
    'connect-src': []
  };
  // pending/blocked/error/removed：不渲染播放器，不生成任何放行来源。
  if (view.status !== 'ready') return directives;

  if (view.provider === 'xigua') {
    const officialHost = hostOf(safeHttpsUrl(view.official_url) ?? '');
    if (officialHost) directives['frame-src'] = [officialHost];
  } else {
    const mediaHost = hostOf(safeHttpsUrl(view.media_url) ?? '');
    if (mediaHost) {
      directives['media-src'] = [mediaHost];
      directives['connect-src'] = [mediaHost];
    }
  }
  const posterHost = hostOf(safeHttpsUrl(view.poster_url) ?? '');
  if (posterHost) directives['img-src'] = [posterHost];
  return directives;
}

/** 把指令渲染为 CSP 指令串（与后端逐页生成的语义一致；供测试/文档验证）。 */
export function videoCspHeader(directives: VideoCspDirectives): string {
  const parts: string[] = [];
  for (const key of ['frame-src', 'media-src', 'img-src', 'connect-src'] as const) {
    const hosts = directives[key];
    if (hosts.length > 0) parts.push(`${key} ${hosts.join(' ')}`);
  }
  return parts.join('; ');
}

/**
 * 是否可在浏览器内渲染播放器（M10-UI-05 无 JS 降级的关键判定）：
 *  - 仅 status=ready 且存在经 safeHttpsUrl 校验的官方可播放 URL；
 *  - 来源 host 白名单由后端最终裁决（后端只返回白名单内来源的 URL）；
 *    前端只保证 https/无 userinfo，绝不自行放宽或拼接 URL；
 *  - SSR（无 JS）恒为 false → 只渲染安全外链卡片。
 */
export function canRenderInlinePlayer(
  view: Pick<VideoEmbedView, 'status' | 'provider' | 'media_url' | 'official_url'>
): boolean {
  if (view.status !== 'ready') return false;
  if (view.provider === 'xigua') {
    return hostOf(safeHttpsUrl(view.official_url) ?? '') !== null;
  }
  return hostOf(safeHttpsUrl(view.media_url) ?? '') !== null;
}

/**
 * 无 JS / 降级场景的安全外链 URL（M10-UI-05）：
 *  - 仅 https；取 media_url → source_url → official_url 的**首个有效**值：
 *    direct/hls 首选媒体源；xigua 首选规范化页面（iframe embed URL 只在
 *    两者皆缺时兜底，避免把裸播放器当落地页）；
 *  - blocked/removed 恒为 null（M10-UI-04：不渲染视频 URL）。
 */
export function safeExternalHref(
  view: Pick<VideoEmbedView, 'status' | 'media_url' | 'official_url' | 'source_url'>
): string | null {
  if (view.status === 'blocked' || view.status === 'removed') return null;
  return (
    safeHttpsUrl(view.media_url) ??
    safeHttpsUrl(view.source_url) ??
    safeHttpsUrl(view.official_url)
  );
}
