// M07-SHOP-06/UI-05：装扮 Token 白名单渲染。
//
// 安全模型（docs/INTERNAL-MARKETPLACE.md §9）：商品/展示 Token 只能是后端注册
// 的有限枚举。前端**不**解释任意 value 为 CSS/HTML——这里把 Token key + 固定
// 枚举值映射到预定义样式/文案，白名单之外的一律不渲染。
//
// 约定：presentation_tokens 为 `{ [slotKey]: value }`（或 badge 数组）。slotKey
// 与展示槽位一致：nickname_color / avatar_frame /
// profile_effect / post_effect / profile_badges。

import type { PublicPresentationTokens } from '$lib/api/types';

/** 昵称颜色：固定调色板（Token 值必须是其中的 key，白名单之外不渲染）。 */
export const NICKNAME_COLORS: Record<string, string> = {
  blue: '#0969da',
  purple: '#8250df',
  green: '#1a7f37',
  gold: '#bf8700',
  red: '#cf222e',
  teal: '#0e8a16',
  pink: '#da3633'
};

/** 昵称颜色展示名（商城/衣柜行内预览与回退文案）。 */
export const NICKNAME_COLOR_LABELS: Record<string, string> = {
  blue: '蔚蓝',
  purple: '紫韵',
  green: '翠绿',
  gold: '鎏金',
  red: '赤红',
  teal: '青碧',
  pink: '粉黛'
};

/** 颜色特效白名单。class 名称在 CosmeticName.svelte 中固定实现。 */
export const NICKNAME_EFFECTS: Record<string, { label: string; className: string }> = {
  ...Object.fromEntries(Object.keys(NICKNAME_COLORS).map((key) => [key, { label: key, className: `nickname-solid-${key}` }])),
  rainbow: { label: '彩虹流光', className: 'nickname-rainbow' },
  breathing: { label: '呼吸微光', className: 'nickname-breathing' },
  gradient_sunset: { label: '落日渐变', className: 'nickname-gradient-sunset' },
  gradient_ocean: { label: '海湾渐变', className: 'nickname-gradient-ocean' },
  gradient_aurora: { label: '极光渐变', className: 'nickname-gradient-aurora' }
};

export function nicknameEffectClass(token: unknown): string | null {
  return typeof token === 'string' && token in NICKNAME_EFFECTS ? NICKNAME_EFFECTS[token].className : null;
}

/** 头像框：固定 CSS class（样式在 wardrobe 页 scoped 定义），Token 值只做查表。 */
export const AVATAR_FRAMES: Record<string, string> = {
  gold_ring: 'avatar-frame-gold',
  blue_ring: 'avatar-frame-blue',
  glow: 'avatar-frame-glow'
};

/** 头像框展示名。 */
export const AVATAR_FRAME_LABELS: Record<string, string> = {
  gold_ring: '鎏金之环',
  blue_ring: '蔚蓝之环',
  glow: '流光之环'
};

/** 头像挂件（emoji 图标，固定映射，禁止远程资源）。 */
export const AVATAR_ATTACHMENTS: Record<string, string> = {
  cat: '🐱',
  planet: '🪐',
  star: '⭐',
  leaf: '🍀',
  badge_star: '🌟'
};

/** 徽章：固定文案映射（profile_badges 数组值逐个查表）。 */
export const BADGES: Record<string, { label: string; icon: string }> = {
  contributor: { label: '贡献者', icon: '🛠' },
  early_member: { label: '早期成员', icon: '🐣' },
  veteran: { label: '资深用户', icon: '🏆' },
  active: { label: '活跃达人', icon: '🔥' }
};

/** 主页装饰（profile_effect）：固定类名（背景纹理在 CSS 定义）。 */
export const PROFILE_EFFECTS: Record<string, string> = {
  sparkle: 'effect-sparkle',
  dark_stars: 'effect-dark-stars'
};

/** 主页装饰展示名。 */
export const PROFILE_EFFECT_LABELS: Record<string, string> = {
  sparkle: '星芒',
  dark_stars: '暗夜星河'
};

/** 帖子装饰（post_effect）。 */
export const POST_EFFECTS: Record<string, string> = {
  highlight: 'post-highlight',
  thanks: 'post-thanks'
};

/** 帖子装饰展示名。 */
export const POST_EFFECT_LABELS: Record<string, string> = {
  highlight: '高亮',
  thanks: '感谢'
};

/** 可渲染的槽位 key 白名单。 */
export const WARDROBE_SLOT_KEYS = [
  'nickname_color',
  'avatar_frame',
  'profile_effect',
  'post_effect',
  'profile_badges'
] as const;

export type WardrobeSlotKey = (typeof WARDROBE_SLOT_KEYS)[number];

/** 把槽位 key 中文名化（衣柜展示）。 */
export function slotLabel(slot: string): string {
  const map: Record<string, string> = {
    nickname_color: '昵称颜色',
    avatar_frame: '头像框',
    profile_badges: '徽章',
    profile_badge: '徽章', // 兼容早期种子数据的槽位名。
    profile_effect: '主页装饰',
    post_effect: '帖子装饰'
  };
  return map[slot] ?? slot;
}

/** 规整槽位 key：`profile_badge`（早期种子数据）并入 `profile_badges`。 */
export function normalizeSlot(slot: string | null | undefined): string {
  return slot === 'profile_badge' ? 'profile_badges' : slot ?? '';
}

/** 后端注册 Token 前缀 → 展示值剥离（与后端 SAFE_TOKEN_PREFIXES 对齐）。 */
const TOKEN_PREFIXES = [
  'nickname.color.',
  'avatar.frame.',
  'avatar.attachment.',
  'profile.effect.',
  'post.effect.',
  'badge.',
  'reaction.pack.'
] as const;

/** 权益 projection：把后端 Token 字符串数组投影为前端白名单可视化 Token 与中文标签。 */
export interface EntitlementTokenProjection {
  /** 可直接传给 CosmeticAvatar/CosmeticName 的白名单 Token（未知值不写入）。 */
  visual: PublicPresentationTokens;
  /** 各 Token 的中文展示名（未注册的 Token 跳过，顺序与输入一致）。 */
  labels: string[];
}

/**
 * 把权益携带的 Token 字符串（`nickname.color.gold`、`badge.contributor`…）
 * 投影为前端白名单 Token + 中文标签。
 *
 * 安全模型与 tokens.ts 其余部分一致：只有注册枚举值才会进入 `visual`，
 * 未知/未授权的 Token 一律不渲染；`asset_attachment_id` 仅在对应头像槽位
 * （avatar_frame / avatar_attachment）时写入，且由服务端保证是本人可读的
 * ready 公共 PNG 附件。
 */
export function projectEntitlementTokens(
  tokens: readonly string[] | null | undefined,
  assetAttachmentId?: string | null,
  slot?: string | null
): EntitlementTokenProjection {
  const visual: PublicPresentationTokens = {};
  const labels: string[] = [];
  for (const raw of tokens ?? []) {
    if (typeof raw !== 'string' || raw.length > 64) continue;
    const valueOf = (prefix: string): string | null => {
      if (!raw.startsWith(prefix)) return null;
      const value = raw.slice(prefix.length);
      return /^[a-z0-9_]+$/.test(value) ? value : null;
    };
    // 每个前缀只允许命中一个分支；值必须在本文件的白名单枚举内。
    if (raw.startsWith('nickname.color.')) {
      const v = valueOf('nickname.color.');
      // 昵称颜色槽位同时容纳纯色（NICKNAME_COLORS）与动态特效
      // （rainbow/breathing/gradient_*，见 NICKNAME_EFFECTS）。
      if (v && (v in NICKNAME_COLORS || v in NICKNAME_EFFECTS)) {
        visual.nickname_color = v;
        labels.push(NICKNAME_COLOR_LABELS[v] ?? NICKNAME_EFFECTS[v]?.label ?? v);
      }
    } else if (raw.startsWith('avatar.frame.')) {
      const v = valueOf('avatar.frame.');
      if (v && v in AVATAR_FRAMES) {
        visual.avatar_frame = v;
        labels.push(AVATAR_FRAME_LABELS[v] ?? v);
      }
    } else if (raw.startsWith('avatar.attachment.')) {
      const v = valueOf('avatar.attachment.');
      if (v && v in AVATAR_ATTACHMENTS) {
        visual.avatar_attachment = v;
        labels.push(v);
      }
    } else if (raw.startsWith('profile.effect.')) {
      const v = valueOf('profile.effect.');
      if (v && v in PROFILE_EFFECTS) {
        visual.profile_effect = v;
        labels.push(PROFILE_EFFECT_LABELS[v] ?? v);
      }
    } else if (raw.startsWith('post.effect.')) {
      const v = valueOf('post.effect.');
      if (v && v in POST_EFFECTS) {
        visual.post_effect = v;
        labels.push(POST_EFFECT_LABELS[v] ?? v);
      }
    } else if (raw.startsWith('badge.')) {
      const v = valueOf('badge.');
      if (v && v in BADGES) {
        visual.profile_badges = [...(visual.profile_badges ?? []), v];
        labels.push(BADGES[v].label);
      }
    } else {
      continue;
    }
  }
  const asset = typeof assetAttachmentId === 'string' ? assetAttachmentId : null;
  if (asset && /^[A-Za-z0-9-]{1,64}$/.test(asset)) {
    if (slot === 'avatar_frame') visual.avatar_frame_attachment_id = asset;
  }
  return { visual, labels };
}
