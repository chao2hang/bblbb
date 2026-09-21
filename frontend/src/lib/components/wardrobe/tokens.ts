// M07-SHOP-06/UI-05：装扮 Token 白名单渲染。
//
// 安全模型（docs/INTERNAL-MARKETPLACE.md §9）：商品/展示 Token 只能是后端注册
// 的有限枚举。前端**不**解释任意 value 为 CSS/HTML——这里把 Token key + 固定
// 枚举值映射到预定义样式/文案，白名单之外的一律不渲染。
//
// 约定：presentation_tokens 为 `{ [slotKey]: value }`（或 badge 数组）。slotKey
// 与展示槽位一致：nickname_color / avatar_frame /
// profile_effect / post_effect / profile_badges。

import type { CosmeticDef, PublicPresentationTokens } from '$lib/api/types';

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

/** 头像框：固定 CSS class（样式在 CosmeticAvatar.svelte scoped 定义），Token 值只做查表。 */
export const AVATAR_FRAMES: Record<string, string> = {
  gold_ring: 'avatar-frame-gold',
  blue_ring: 'avatar-frame-blue',
  glow: 'avatar-frame-glow'
};

/** 内置动效头像框资源（Steam 官方精选 APNG 动图，位于 /cosmetics/frames/ 下） */
export const BUILTIN_APNG_FRAMES: Record<string, { src: string; label: string }> = {
  galaxy_dream: { src: '/cosmetics/frames/galaxy_dream.png', label: '银河梦语' },
  ocean_glaze: { src: '/cosmetics/frames/ocean_glaze.png', label: '碧波琉璃' },
  cloud_blade: { src: '/cosmetics/frames/cloud_blade.png', label: '流云锋影' },
  nether_light: { src: '/cosmetics/frames/nether_light.png', label: '冥咒浮光' },
  fire: { src: '/cosmetics/frames/fire.png', label: '烈焰焚天' },
  lightning: { src: '/cosmetics/frames/lightning.png', label: '极电雷鸣' },
  glitch: { src: '/cosmetics/frames/glitch.png', label: '赛博故障' },
  signal_lost: { src: '/cosmetics/frames/signal_lost.png', label: '信号丢失' },
  ocean_aquarium: { src: '/cosmetics/frames/ocean_aquarium.png', label: '深海水族馆' },
  petal_drift: { src: '/cosmetics/frames/petal_drift.png', label: '樱落花雨' },
  snowy_whiskers: { src: '/cosmetics/frames/snowy_whiskers.png', label: '雪白猫耳' },
  midnight_whiskers: { src: '/cosmetics/frames/midnight_whiskers.png', label: '暗夜猫耳' },
  pink_whiskers: { src: '/cosmetics/frames/pink_whiskers.png', label: '樱粉猫耳' },
  shimmering_stars: { src: '/cosmetics/frames/shimmering_stars.png', label: '璀璨星宿' },
  sky_clouds: { src: '/cosmetics/frames/sky_clouds.png', label: '碧空流云' },
  system_interface: { src: '/cosmetics/frames/system_interface.png', label: '全息终端' },
  steam_china_2026: { src: '/cosmetics/frames/steam_china_2026.png', label: '华夏龙腾' },
  winter_snow: { src: '/cosmetics/frames/winter_snow.png', label: '极地飞雪' },
  fairy_frame: { src: '/cosmetics/frames/fairy_frame.png', label: '森林仙境' },
  rpg_weapons: { src: '/cosmetics/frames/rpg_weapons.png', label: '勇者神兵' },
  ultraviolet: { src: '/cosmetics/frames/ultraviolet.png', label: '荧光极紫' },
  cyan_fire: { src: '/cosmetics/frames/cyan_fire.png', label: '苍蓝烈焰' },
  cyan_lightning: { src: '/cosmetics/frames/cyan_lightning.png', label: '苍雷破空' },
  carbon_weave: { src: '/cosmetics/frames/carbon_weave.png', label: '碳纤矩阵' }
};

/**
 * 彩色头像框环（M07-SHOP-UI-09）：Token 值 → 固定调色板色值。
 * 渲染统一走 `.avatar-frame-ring` class + CSS 变量（`avatarFrameRingVars`），
 * 管理员/用户都不能注入任意 CSS——只有本表的色值会生效。
 */
export const AVATAR_FRAME_COLORS: Record<string, string> = {
  crimson: '#dc2645',
  emerald: '#0f9d58',
  sapphire: '#1f6feb',
  violet: '#8250df',
  amber: '#d97706',
  rose_gold: '#e08c9b',
  silver: '#9aa4b2',
  jade: '#14b8a6'
};

/** 把色值转换为彩色环的两个 CSS 变量（边框实色 + 半透明光晕）。 */
export function avatarFrameRingVars(hex: string): string {
  const raw = hex.replace('#', '');
  if (!/^[0-9a-fA-F]{6}$/.test(raw)) return '';
  const r = parseInt(raw.slice(0, 2), 16);
  const g = parseInt(raw.slice(2, 4), 16);
  const b = parseInt(raw.slice(4, 6), 16);
  return `--avatar-frame-color:#${raw};--avatar-frame-glow:rgba(${r},${g},${b},0.45);`;
}

/** 彩色环 Token →（class, 内联样式）。白名单之外返回 null。 */
export function avatarFrameRing(
  token: unknown
): { className: string; style: string } | null {
  if (typeof token !== 'string' || !(token in AVATAR_FRAME_COLORS)) return null;
  const style = avatarFrameRingVars(AVATAR_FRAME_COLORS[token]);
  return style ? { className: 'avatar-frame-ring', style } : null;
}

/** 头像框展示名。 */
export const AVATAR_FRAME_LABELS: Record<string, string> = {
  gold_ring: '鎏金之环',
  blue_ring: '蔚蓝之环',
  glow: '流光之环',
  ...Object.fromEntries(
    Object.entries(BUILTIN_APNG_FRAMES).map(([key, item]) => [key, item.label])
  ),
  ...Object.fromEntries(
    Object.keys(AVATAR_FRAME_COLORS).map((key) => [
      key,
      {
        crimson: '绯红之环',
        emerald: '翠玉之环',
        sapphire: '苍蓝之环',
        violet: '堇紫之环',
        amber: '琥珀之环',
        rose_gold: '玫瑰金之环',
        silver: '霜银之环',
        jade: '青玉之环'
      }[key] ?? key
    ])
  )
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
  'title_prefix',
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
    post_effect: '帖子装饰',
    title_prefix: '称号'
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
 *
 * `defs`（M07-SHOP-UI-10）：装扮样式库定义（GET /shop/cosmetics）；提供时，
 * `nickname.color.<id>` / `avatar.frame.<id>` 的自定义定义按名称解析标签，
 * 未提供的调用方对未知 id 不渲染（与既有行为一致）。
 */
export function projectEntitlementTokens(
  tokens: readonly string[] | null | undefined,
  assetAttachmentId?: string | null,
  slot?: string | null,
  defs?: readonly CosmeticDef[] | null
): EntitlementTokenProjection {
  const defById = defs ? new Map(defs.map((d) => [d.id, d])) : null;
  const visual: PublicPresentationTokens = {};
  const labels: string[] = [];
  for (const raw of tokens ?? []) {
    if (typeof raw !== 'string' || raw.length > 64) continue;
    const valueOf = (prefix: string): string | null => {
      if (!raw.startsWith(prefix)) return null;
      const value = raw.slice(prefix.length);
      return /^[a-zA-Z0-9_-]+$/.test(value) ? value : null;
    };
    // 每个前缀只允许命中一个分支；值必须在本文件的白名单枚举或样式库定义内。
    if (raw.startsWith('nickname.color.')) {
      const v = valueOf('nickname.color.');
      // 昵称颜色槽位同时容纳纯色（NICKNAME_COLORS）、动态特效
      // （rainbow/breathing/gradient_*，见 NICKNAME_EFFECTS）与样式库自定义定义。
      if (v && (v in NICKNAME_COLORS || v in NICKNAME_EFFECTS)) {
        visual.nickname_color = v;
        labels.push(NICKNAME_COLOR_LABELS[v] ?? NICKNAME_EFFECTS[v]?.label ?? v);
      } else if (v && defById?.get(v)?.kind === 'nickname_color') {
        const def = defById.get(v)!;
        visual.nickname_color = v;
        visual.nickname_color_name = def.name;
        visual.nickname_color_style = def.style;
        labels.push(def.name);
      }
    } else if (raw.startsWith('avatar.frame.')) {
      const v = valueOf('avatar.frame.');
      // 头像框槽位同时容纳固定样式（AVATAR_FRAMES）、彩色环（AVATAR_FRAME_COLORS）、
      // 内置 Steam 动效头像框（BUILTIN_APNG_FRAMES）与样式库自定义定义。
      if (v && (v in AVATAR_FRAMES || v in AVATAR_FRAME_COLORS || v in BUILTIN_APNG_FRAMES)) {
        visual.avatar_frame = v;
        labels.push(AVATAR_FRAME_LABELS[v] ?? v);
      } else if (v && defById?.get(v)?.kind === 'avatar_frame') {
        const def = defById.get(v)!;
        visual.avatar_frame = v;
        visual.avatar_frame_name = def.name;
        visual.avatar_frame_style = def.style;
        if (def.style?.url) {
          visual.avatar_frame_url = def.style.url;
        } else if (def.style?.image) {
          visual.avatar_frame_url = `/api/v1/steam-assets/frames/${def.style.image}`;
        }
        labels.push(def.name);
      }
    } else if (raw.startsWith('avatar.attachment.')) {
      const v = valueOf('avatar.attachment.');
      if (v && v in AVATAR_ATTACHMENTS) {
        visual.avatar_attachment = v;
        labels.push(v);
      }
    } else if (raw.startsWith('profile.effect.')) {
      const v = valueOf('profile.effect.');
      if (v && defById?.get(v)?.kind === 'profile_effect') {
        const def = defById.get(v)!;
        visual.profile_effect = v;
        visual.profile_effect_name = def.name;
        visual.profile_effect_style = def.style;
        labels.push(def.name);
      } else if (v && v in PROFILE_EFFECTS) {
        visual.profile_effect = v;
        labels.push(PROFILE_EFFECT_LABELS[v] ?? v);
      } else if (v && defById?.has(v)) {
        const def = defById.get(v)!;
        visual.profile_effect = v;
        visual.profile_effect_name = def.name;
        visual.profile_effect_style = def.style;
        labels.push(def.name);
      }
    } else if (raw.startsWith('post.effect.')) {
      const v = valueOf('post.effect.');
      if (v && v in POST_EFFECTS) {
        visual.post_effect = v;
        labels.push(POST_EFFECT_LABELS[v] ?? v);
      } else if (v && defById?.get(v)?.kind === 'post_effect') {
        const def = defById.get(v)!;
        visual.post_effect = v;
        visual.post_effect_name = def.name;
        visual.post_effect_style = def.style;
        labels.push(def.name);
      }
    } else if (raw.startsWith('badge.')) {
      const v = valueOf('badge.');
      if (v && v in BADGES) {
        visual.profile_badges = [...(visual.profile_badges ?? []), v];
        labels.push(BADGES[v].label);
      } else if (v && defById?.get(v)?.kind === 'cosmetic_badge') {
        const def = defById.get(v)!;
        visual.profile_badges = [...(visual.profile_badges ?? []), v];
        visual.profile_badge_names = [...(visual.profile_badge_names ?? []), def.name];
        visual.profile_badge_styles = [...(visual.profile_badge_styles ?? []), def.style];
        labels.push(def.name);
      }
    } else if (raw.startsWith('title.prefix.')) {
      const v = valueOf('title.prefix.');
      if (v && defById?.get(v)?.kind === 'title_prefix') {
        const def = defById.get(v)!;
        visual.title_prefix = v;
        visual.title_prefix_name = def.name;
        visual.title_prefix_style = def.style;
        labels.push(def.name);
      }
    } else if (raw.startsWith('reaction.pack.')) {
      const v = valueOf('reaction.pack.');
      if (v && defById?.get(v)?.kind === 'reaction_pack') labels.push(defById.get(v)!.name);
    } else if (raw.startsWith('utility.')) {
      const v = valueOf('utility.');
      if (v && defById?.get(v)?.kind === 'utility') labels.push(defById.get(v)!.name);
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
