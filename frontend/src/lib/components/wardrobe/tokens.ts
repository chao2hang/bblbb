// M07-SHOP-06/UI-05：装扮 Token 白名单渲染。
//
// 安全模型（docs/INTERNAL-MARKETPLACE.md §9）：商品/展示 Token 只能是后端注册
// 的有限枚举。前端**不**解释任意 value 为 CSS/HTML——这里把 Token key + 固定
// 枚举值映射到预定义样式/文案，白名单之外的一律不渲染。
//
// 约定：presentation_tokens 为 `{ [slotKey]: value }`（或 badge 数组）。slotKey
// 与展示槽位一致：nickname_color / nickname_decoration / avatar_frame /
// avatar_attachment / profile_effect / title_prefix / post_effect / profile_badges。

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

/** 昵称装饰（前后缀文本），固定映射。 */
export const NICKNAME_DECORATIONS: Record<string, { prefix: string; suffix: string }> = {
  star: { prefix: '✦', suffix: '✦' },
  diamond: { prefix: '◆', suffix: '◆' },
  flame: { prefix: '🔥', suffix: '' },
  crown: { prefix: '♛', suffix: '' }
};

/** 头像框：固定 CSS class（样式在 wardrobe 页 scoped 定义），Token 值只做查表。 */
export const AVATAR_FRAMES: Record<string, string> = {
  gold_ring: 'avatar-frame-gold',
  blue_ring: 'avatar-frame-blue',
  glow: 'avatar-frame-glow'
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

/** 标题前缀（title_prefix）。 */
export const TITLE_PREFIXES: Record<string, { prefix: string; label: string }> = {
  night_owl: { prefix: '夜猫子', label: '夜猫子' },
  warm_heart: { prefix: '热心居民', label: '热心居民' }
};

/** 主页装饰（profile_effect）：固定类名（背景纹理在 CSS 定义）。 */
export const PROFILE_EFFECTS: Record<string, string> = {
  sparkle: 'effect-sparkle',
  dark_stars: 'effect-dark-stars'
};

/** 帖子装饰（post_effect）。 */
export const POST_EFFECTS: Record<string, string> = {
  highlight: 'post-highlight',
  thanks: 'post-thanks'
};

/** 可渲染的槽位 key 白名单。 */
export const WARDROBE_SLOT_KEYS = [
  'nickname_color',
  'nickname_decoration',
  'avatar_frame',
  'avatar_attachment',
  'profile_effect',
  'title_prefix',
  'post_effect',
  'profile_badges'
] as const;

export type WardrobeSlotKey = (typeof WARDROBE_SLOT_KEYS)[number];

/** 把槽位 key 中文名化（衣柜展示）。 */
export function slotLabel(slot: string): string {
  const map: Record<string, string> = {
    nickname_color: '昵称颜色',
    nickname_decoration: '昵称装饰',
    avatar_frame: '头像框',
    avatar_attachment: '头像挂件',
    profile_effect: '主页装饰',
    title_prefix: '昵称前缀',
    post_effect: '帖子装饰',
    profile_badges: '徽章'
  };
  return map[slot] ?? slot;
}
