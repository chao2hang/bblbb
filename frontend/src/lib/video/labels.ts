// M10-UI：视频状态/Provider/降级原因的中文标签与时长格式化（纯函数）。
//
// 只做展示映射：Provider/状态枚举为封闭集合；`degraded_reason` 为后端稳定
// 码（≤40 字符），未命中映射时原样短展示，绝不复述探测原文。

import type { VideoEmbedProvider, VideoEmbedStatus } from '$lib/api/types';

const STATUS_LABELS: Record<VideoEmbedStatus, string> = {
  pending: '解析中',
  ready: '可嵌入',
  blocked: '已下架',
  error: '解析失败',
  removed: '已移除'
};

const PROVIDER_LABELS: Record<VideoEmbedProvider, string> = {
  direct: '直链视频',
  hls: 'HLS 直播流',
  xigua: '西瓜视频'
};

/** 状态 → 中文标签（未知名原样返回）。 */
export function videoStatusLabel(status: VideoEmbedStatus | undefined): string {
  return status ? (STATUS_LABELS[status] ?? status) : '';
}

/** Provider → 中文标签。 */
export function videoProviderLabel(provider: VideoEmbedProvider | null | undefined): string {
  return provider ? (PROVIDER_LABELS[provider] ?? provider) : '';
}

/** 降级稳定码 → 中文说明（不可嵌入时的原因；未命中原样短展示）。 */
export function videoDegradedLabel(reason: string | null | undefined): string {
  const map: Record<string, string> = {
    no_embed_permission: '来源未授权嵌入，仅可外链打开',
    provider_unavailable: '视频服务商暂不可用，仅可外链打开',
    rate_limited: '来源限流，仅可外链打开',
    taken_down: '来源已下架，仅可外链打开',
    provider_disabled: '该视频来源已停用，仅可外链打开'
  };
  return reason ? (map[reason] ?? reason) : '当前只能以外链方式引用';
}

/** 状态 → 徽章色调（与现有 badge-* 样式对应）。 */
export function videoStatusTone(status: VideoEmbedStatus): string {
  switch (status) {
    case 'ready':
      return 'badge-success';
    case 'pending':
      return 'badge-neutral';
    case 'blocked':
    case 'removed':
      return 'badge-warning';
    case 'error':
      return 'badge-danger';
  }
}

/** 秒 → "mm:ss" / "h:mm:ss"（null/NaN → ''）。 */
export function formatVideoDuration(seconds: number | null | undefined): string {
  if (typeof seconds !== 'number' || !Number.isFinite(seconds) || seconds < 0) return '';
  const total = Math.round(seconds);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const pad = (n: number) => String(n).padStart(2, '0');
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${pad(m)}:${pad(s)}`;
}
