// M07-SHOP-STUDIO：装扮工作台的设计草稿模型。
//
// 把「可视化编辑器状态」与「提交用 style JSON」解耦：StyleDraft 是类型安全的
// 编辑状态，draftToStyle 是纯函数 mapper（替代 admin/shop/+page.svelte 旧弹层
// 内联的 styleObject 派生）。所有边界镜像后端 cosmetics.rs validate_style：
// 色值 #rrggbb、时长 800–30000ms、widthPx 1–8、glowPx 0–24、渐变色标 2–5、
// dailyLimit 1–100、quantity 1–100、cooldownSec 0–86400、名称 1–32 字符。
// 前端先行校验（validateDraft）用于分区回显；服务端 schema 校验仍是最终防线。

import type { CosmeticDef, CosmeticDefStyle } from '$lib/api/types';

export type StyleKind =
  | 'nickname_color'
  | 'avatar_frame'
  | 'profile_effect';

export const STYLE_KINDS: readonly StyleKind[] = [
  'nickname_color',
  'avatar_frame',
  'profile_effect'
];

export const STYLE_KIND_LABELS: Record<StyleKind, string> = {
  nickname_color: '彩色昵称 / 特效',
  avatar_frame: 'Steam 动效头像框',
  profile_effect: '个人资料背景 / 封面'
};

/** 与后端 validate_style 对齐的边界常量。 */
export const STYLE_LIMITS = {
  nameMax: 32,
  durationMin: 800,
  durationMax: 30000,
  widthMin: 1,
  widthMax: 8,
  glowMin: 0,
  glowMax: 24,
  glowSpreadMin: 0,
  glowSpreadMax: 20,
  shadowMin: 0,
  shadowMax: 30,
  letterSpacingMin: 0,
  letterSpacingMax: 8,
  angleMin: 0,
  angleMax: 360,
  opacityMin: 10,
  opacityMax: 100,
  codeMax: 8192,
  stopsMin: 2,
  stopsMax: 5,
  dailyLimitMin: 1,
  dailyLimitMax: 100,
  quantityMin: 1,
  quantityMax: 100,
  cooldownMin: 0,
  cooldownMax: 86400
} as const;

export const DURATION_OPTIONS = [800, 1500, 2800, 5000, 8000, 12000, 20000, 30000] as const;

export const NICKNAME_MODES: { value: 'solid' | 'gradient' | 'glow'; label: string }[] = [
  { value: 'solid', label: '纯色' },
  { value: 'gradient', label: '渐变' },
  { value: 'glow', label: '发光' }
];

/** 边框线型定义选项 */
export const BORDER_STYLE_OPTIONS: { value: 'solid' | 'dashed' | 'dotted' | 'double' | 'groove'; label: string }[] = [
  { value: 'solid', label: '实线 (Solid)' },
  { value: 'dashed', label: '虚线 (Dashed)' },
  { value: 'dotted', label: '点线 (Dotted)' },
  { value: 'double', label: '双实线 (Double)' },
  { value: 'groove', label: '立体凹槽 (Groove)' }
];

/** 昵称渐变动画选项 */
export const NICKNAME_GRADIENT_ANIMATIONS = [
  { value: 'flow', label: '平滑流动' },
  { value: 'wave', label: '波浪起伏' },
  { value: 'shimmer', label: '流光扫过' },
  { value: 'glitch', label: '赛博故障' },
  { value: 'rainbow', label: '彩虹轮转' },
  { value: 'pulse', label: '明暗脉动' },
  { value: 'none', label: '静止' }
];

/** 昵称发光动画选项 */
export const NICKNAME_GLOW_ANIMATIONS = [
  { value: 'breathe', label: '呼吸微光' },
  { value: 'flicker', label: '霓虹闪烁' },
  { value: 'fire', label: '火焰微光' },
  { value: 'pulse', label: '快速脉冲' },
  { value: 'glitch', label: '故障抖动' },
  { value: 'none', label: '静止' }
];

/** 昵称纯色动画选项 */
export const NICKNAME_SOLID_ANIMATIONS = [
  { value: 'none', label: '静止' },
  { value: 'breathe', label: '轻微呼吸' },
  { value: 'pulse', label: '脉冲跳跃' },
  { value: 'bounce', label: '轻微律动' },
  { value: 'glitch', label: '赛博故障' },
  { value: 'shimmer', label: '光斑流转' }
];

/** 头像框动画选项 */
export const AVATAR_FRAME_ANIMATIONS = [
  { value: 'none', label: '静止' },
  { value: 'pulse', label: '呼吸脉冲' },
  { value: 'spin', label: '光轨旋转' },
  { value: 'ripple', label: '同心水波' },
  { value: 'float', label: '悬浮微动' },
  { value: 'glitch', label: '赛博抖动' },
  { value: 'rainbow', label: '彩虹色变' },
  { value: 'breathe', label: '深层微光' }
];

/** 主页装饰动效选项 */
export const PROFILE_ANIMATIONS = [
  { value: 'none', label: '静止' },
  { value: 'shimmer', label: '微光闪烁' },
  { value: 'aurora', label: '极光飘拂' },
  { value: 'flow', label: '流体漫溢' },
  { value: 'pulse', label: '星芒脉冲' },
  { value: 'scanline', label: '全息扫描' }
];

/** 帖子特效动效选项 */
export const POST_EFFECT_ANIMATIONS = [
  { value: 'none', label: '静止' },
  { value: 'pulse', label: '脉冲' },
  { value: 'float', label: '悬浮' },
  { value: 'bounce', label: '弹跳' },
  { value: 'sparkle', label: '星光' },
  { value: 'glitch', label: '故障' },
  { value: 'rainbow', label: '流光' }
];

/** 徽章与称号动效选项 */
export const BADGE_ANIMATIONS = [
  { value: 'none', label: '静止' },
  { value: 'pulse', label: '脉冲发光' },
  { value: 'shimmer', label: '流光扫过' },
  { value: 'bounce', label: '微弹跳' },
  { value: 'spin', label: '微旋律动' }
];

/** 图标白名单（与后端 SAFE_ICON_TOKENS 一致，Icon.svelte 可直接渲染）。 */
export const ICON_OPTIONS: { value: string; label: string }[] = [
  { value: 'sparkles', label: '闪光' },
  { value: 'award', label: '奖章' },
  { value: 'heart', label: '爱心' },
  { value: 'star', label: '星星' },
  { value: 'palette', label: '调色板' },
  { value: 'trophy', label: '奖杯' },
  { value: 'wand-2', label: '魔法' }
];

export const TEXTURE_OPTIONS: { value: string; label: string }[] = [
  { value: 'sparkle', label: '星芒' },
  { value: 'dark_stars', label: '暗夜星河' },
  { value: 'grid', label: '科技网格' },
  { value: 'dots', label: '点阵空间' },
  { value: 'aurora', label: '极光天幕' },
  { value: 'matrix', label: '代码矩阵' },
  { value: 'waves', label: '潮汐微浪' },
  { value: 'snow', label: '飘雪粒子' },
  { value: 'bubbles', label: '浮动气泡' }
];

export interface StyleDraft {
  kind: StyleKind;
  name: string;
  /** 昵称模式（仅 nickname_color 使用）。 */
  mode: 'solid' | 'gradient' | 'glow';
  color: string;
  stops: string[];
  animate: string;
  durationMs: number;
  widthPx: number;
  glowPx: number;
  shape: 'circle' | 'rounded' | 'square';
  icon: string;
  texture: string;
  baseColor: string;
  accentColor: string;
  notification: 'none' | 'author';
  dailyLimit: number;
  utilityAction: 'rename' | 'withdraw_edit' | 'profile_reset';
  quantity: number;
  cooldownSec: number;
  // 扩展样式与动画定义
  borderStyle: 'solid' | 'dashed' | 'dotted' | 'double' | 'groove';
  glowSpread: number;
  frameScale: number;
  shadowPx: number;
  letterSpacing: number;
  angle: number;
  opacity: number;
  // 开放自定义 CSS & JS
  css: string;
  js: string;
  /** Steam / 外部 APNG 头像框图片直链（实时试穿用） */
  avatarFrameUrl?: string;
  /** Steam / 外部个人资料背景图直链 */
  profileBackgroundUrl?: string;
  /** Steam / 外部动态 WebM 视频直链 */
  profileBackgroundWebm?: string;
}

export function createDraft(kind: StyleKind = 'nickname_color'): StyleDraft {
  return {
    kind,
    name: '',
    mode: 'solid',
    color: '#f472b6',
    stops: ['#f472b6', '#c084fc'],
    animate: 'flow',
    durationMs: 5000,
    widthPx: 3,
    glowPx: 10,
    shape: 'circle',
    icon: 'sparkles',
    texture: 'sparkle',
    baseColor: '#101827',
    accentColor: '#8b5cf6',
    notification: 'author',
    dailyLimit: 10,
    utilityAction: 'rename',
    quantity: 1,
    cooldownSec: 0,
    borderStyle: 'solid',
    glowSpread: 0,
    frameScale: 100,
    shadowPx: 0,
    letterSpacing: 0,
    angle: 90,
    opacity: 100,
    css: '',
    js: '',
    avatarFrameUrl: '',
    profileBackgroundUrl: '',
    profileBackgroundWebm: ''
  };
}

const HEX = /^#[0-9a-fA-F]{6}$/;

function isHex(v: unknown): v is string {
  return typeof v === 'string' && HEX.test(v);
}

/** 从已有样式库定义还原草稿（编辑场景；非法字段回退默认值）。 */
export function draftFromDef(def: CosmeticDef): StyleDraft {
  const draft = createDraft(STYLE_KINDS.includes(def.kind as StyleKind) ? (def.kind as StyleKind) : 'nickname_color');
  const s = def.style ?? {};
  draft.name = def.name;
  draft.mode = s.mode === 'gradient' || s.mode === 'glow' ? s.mode : 'solid';
  draft.color = isHex(s.color) ? s.color : draft.color;
  draft.stops = Array.isArray(s.colors) && s.colors.length >= STYLE_LIMITS.stopsMin ? [...s.colors] : draft.stops;
  draft.animate = typeof s.animate === 'string' ? s.animate : 'none';
  draft.durationMs = typeof s.durationMs === 'number' ? s.durationMs : draft.durationMs;
  draft.widthPx = typeof s.widthPx === 'number' ? s.widthPx : draft.widthPx;
  draft.glowPx = typeof s.glowPx === 'number' ? s.glowPx : draft.glowPx;
  draft.shape = s.shape === 'rounded' || s.shape === 'square' ? s.shape : 'circle';
  draft.icon = typeof s.icon === 'string' ? s.icon : draft.icon;
  draft.texture = typeof s.texture === 'string' ? s.texture : draft.texture;
  draft.baseColor = isHex(s.baseColor) ? s.baseColor : draft.baseColor;
  draft.accentColor = isHex(s.accentColor) ? s.accentColor : draft.color;
  draft.notification = s.notification === 'none' ? 'none' : 'author';
  draft.dailyLimit = typeof s.dailyLimit === 'number' ? s.dailyLimit : draft.dailyLimit;
  draft.utilityAction = s.action === 'withdraw_edit' || s.action === 'profile_reset' ? s.action : 'rename';
  draft.quantity = typeof s.quantity === 'number' ? s.quantity : draft.quantity;
  draft.cooldownSec = typeof s.cooldownSec === 'number' ? s.cooldownSec : draft.cooldownSec;
  draft.borderStyle = s.borderStyle && ['solid', 'dashed', 'dotted', 'double', 'groove'].includes(s.borderStyle) ? s.borderStyle : 'solid';
  draft.glowSpread = typeof s.glowSpread === 'number' ? s.glowSpread : 0;
  draft.frameScale = typeof s.frameScale === 'number' ? s.frameScale : 100;
  draft.shadowPx = typeof s.shadowPx === 'number' ? s.shadowPx : 0;
  draft.letterSpacing = typeof s.letterSpacing === 'number' ? s.letterSpacing : 0;
  draft.angle = typeof s.angle === 'number' ? s.angle : 90;
  draft.opacity = typeof s.opacity === 'number' ? s.opacity : 100;
  draft.css = typeof s.css === 'string' ? s.css : '';
  draft.js = typeof s.js === 'string' ? s.js : '';
  return draft;
}

/** 从原生表单数据还原草稿（无 JS 提交路径；缺省字段回退默认值）。 */
export function draftFromForm(form: FormData): StyleDraft {
  const kindRaw = String(form.get('kind') ?? '');
  const draft = createDraft(STYLE_KINDS.includes(kindRaw as StyleKind) ? (kindRaw as StyleKind) : 'nickname_color');
  draft.name = String(form.get('name') ?? '').trim();
  const mode = String(form.get('mode') ?? '');
  if (mode === 'gradient' || mode === 'glow' || mode === 'solid') draft.mode = mode;
  const color = String(form.get('color') ?? '');
  if (isHex(color)) draft.color = color;
  const stops = form.getAll('stops').map(String).filter(isHex);
  if (stops.length >= STYLE_LIMITS.stopsMin) draft.stops = stops.slice(0, STYLE_LIMITS.stopsMax);
  const animate = String(form.get('animate') ?? '');
  if (animate) draft.animate = animate;
  const num = (key: string, fallback: number): number => {
    const raw = String(form.get(key) ?? '');
    const n = Number(raw);
    return raw !== '' && Number.isFinite(n) ? Math.round(n) : fallback;
  };
  draft.durationMs = num('durationMs', draft.durationMs);
  draft.widthPx = num('widthPx', draft.widthPx);
  draft.glowPx = num('glowPx', draft.glowPx);
  const shape = String(form.get('shape') ?? '');
  if (shape === 'circle' || shape === 'rounded' || shape === 'square') draft.shape = shape;
  const icon = String(form.get('icon') ?? '');
  if (icon) draft.icon = icon;
  const texture = String(form.get('texture') ?? '');
  if (texture) draft.texture = texture;
  const baseColor = String(form.get('baseColor') ?? '');
  if (isHex(baseColor)) draft.baseColor = baseColor;
  const accentColor = String(form.get('accentColor') ?? '');
  if (isHex(accentColor)) draft.accentColor = accentColor;
  draft.notification = String(form.get('notification') ?? '') === 'none' ? 'none' : 'author';
  draft.dailyLimit = num('dailyLimit', draft.dailyLimit);
  const action = String(form.get('utilityAction') ?? '');
  if (action === 'rename' || action === 'withdraw_edit' || action === 'profile_reset') draft.utilityAction = action;
  draft.quantity = num('quantity', draft.quantity);
  draft.cooldownSec = num('cooldownSec', draft.cooldownSec);
  const borderStyle = String(form.get('borderStyle') ?? '');
  if (['solid', 'dashed', 'dotted', 'double', 'groove'].includes(borderStyle)) {
    draft.borderStyle = borderStyle as StyleDraft['borderStyle'];
  }
  draft.glowSpread = num('glowSpread', draft.glowSpread);
  draft.frameScale = num('frameScale', draft.frameScale);
  draft.shadowPx = num('shadowPx', draft.shadowPx);
  draft.letterSpacing = num('letterSpacing', draft.letterSpacing);
  draft.angle = num('angle', draft.angle);
  draft.opacity = num('opacity', draft.opacity);
  draft.css = String(form.get('css') ?? '');
  draft.js = String(form.get('js') ?? '');
  return draft;
}

/** 草稿 → 提交用结构化样式（字段映射与后端 validate_style 的 schema 一一对应）。 */
export function draftToStyle(draft: StyleDraft): CosmeticDefStyle {
  let style: CosmeticDefStyle;
  switch (draft.kind) {
    case 'nickname_color':
      if (draft.mode === 'solid') {
        style = { mode: 'solid', color: draft.color };
        if (['breathe', 'pulse', 'bounce', 'glitch', 'shimmer', 'wave'].includes(draft.animate)) {
          style.animate = draft.animate;
          style.durationMs = draft.durationMs;
        }
      } else if (draft.mode === 'gradient') {
        style = {
          mode: 'gradient',
          colors: [...draft.stops],
          animate: ['flow', 'wave', 'shimmer', 'glitch', 'rainbow', 'pulse'].includes(draft.animate) ? draft.animate : 'none',
          durationMs: draft.durationMs
        };
        if (draft.angle !== 90 && draft.angle !== undefined) {
          style.angle = draft.angle;
        }
      } else {
        style = {
          mode: 'glow',
          color: draft.color,
          animate: draft.animate === 'none' ? 'none' : ['flicker', 'fire', 'glitch'].includes(draft.animate) ? draft.animate : 'breathe',
          durationMs: draft.durationMs
        };
      }
      if (draft.shadowPx > 0) style.shadowPx = draft.shadowPx;
      if (draft.letterSpacing > 0) style.letterSpacing = draft.letterSpacing;
      break;
    case 'avatar_frame':
      style = {
        mode: 'ring',
        color: draft.color,
        widthPx: draft.widthPx,
        glowPx: draft.glowPx,
        shape: draft.shape,
        animate: ['pulse', 'spin', 'ripple', 'float', 'glitch', 'rainbow', 'breathe'].includes(draft.animate) ? draft.animate : 'none',
        durationMs: draft.durationMs
      };
      if (draft.borderStyle && draft.borderStyle !== 'solid') {
        style.borderStyle = draft.borderStyle;
      }
      if (draft.glowSpread > 0) {
        style.glowSpread = draft.glowSpread;
      }
      if (draft.frameScale && draft.frameScale !== 100) {
        style.frameScale = draft.frameScale;
      }
      if (draft.avatarFrameUrl) {
        (style as Record<string, unknown>).url = draft.avatarFrameUrl;
      }
      if (isHex(draft.accentColor) && draft.accentColor !== draft.color && draft.accentColor !== '#8b5cf6') {
        style.accentColor = draft.accentColor;
      }
      break;
    case 'profile_effect':
      style = {
        mode: 'profile',
        texture: draft.texture,
        baseColor: draft.baseColor,
        accentColor: draft.accentColor,
        animate: draft.animate === 'breathe' || draft.animate === 'shimmer' ? 'shimmer' : ['aurora', 'scanline'].includes(draft.animate) ? draft.animate : 'none',
        durationMs: draft.durationMs
      };
      if (draft.opacity !== 100 && draft.opacity !== undefined) {
        style.opacity = draft.opacity;
      }
      if (draft.profileBackgroundUrl) {
        (style as Record<string, unknown>).url = draft.profileBackgroundUrl;
      }
      if (draft.profileBackgroundWebm) {
        (style as Record<string, unknown>).webm = draft.profileBackgroundWebm;
      }
      break;
    default:
      style = { mode: 'solid', color: draft.color };
      break;
  }

  // 开放自定义 CSS 和 JS
  if (draft.css && draft.css.trim()) {
    style.css = draft.css.trim();
  }
  if (draft.js && draft.js.trim()) {
    style.js = draft.js.trim();
  }

  return style;
}

export type StyleDraftErrors = Partial<
  Record<
    | 'name'
    | 'color'
    | 'stops'
    | 'durationMs'
    | 'widthPx'
    | 'glowPx'
    | 'dailyLimit'
    | 'quantity'
    | 'cooldownSec'
    | 'css'
    | 'js',
    string
  >
>;

function inRange(v: number, min: number, max: number): boolean {
  return Number.isFinite(v) && v >= min && v <= max;
}

/** 前端预检（与后端 validate_style 同边界）：返回字段级错误，空对象 = 通过。 */
export function validateDraft(draft: StyleDraft): StyleDraftErrors {
  const errors: StyleDraftErrors = {};
  const name = draft.name.trim();
  if (!name || [...name].length > STYLE_LIMITS.nameMax) {
    errors.name = `名称需为 1–${STYLE_LIMITS.nameMax} 个字符`;
  }
  const style = draftToStyle(draft);
  if ('color' in style && style.color !== undefined && !isHex(style.color)) {
    errors.color = '颜色需为 #rrggbb 格式';
  }
  if (style.mode === 'gradient') {
    const stops = style.colors ?? [];
    if (stops.length < STYLE_LIMITS.stopsMin || stops.length > STYLE_LIMITS.stopsMax) {
      errors.stops = `渐变色标需 ${STYLE_LIMITS.stopsMin}–${STYLE_LIMITS.stopsMax} 个`;
    } else if (!stops.every(isHex)) {
      errors.stops = '色标需为 #rrggbb 格式';
    }
  }
  if ('durationMs' in style && style.durationMs !== undefined && !inRange(style.durationMs, STYLE_LIMITS.durationMin, STYLE_LIMITS.durationMax)) {
    errors.durationMs = `动画速度需在 ${STYLE_LIMITS.durationMin}–${STYLE_LIMITS.durationMax}ms 之间`;
  }
  if ('widthPx' in style && style.widthPx !== undefined && !inRange(style.widthPx, STYLE_LIMITS.widthMin, STYLE_LIMITS.widthMax)) {
    errors.widthPx = `边框粗细需在 ${STYLE_LIMITS.widthMin}–${STYLE_LIMITS.widthMax}px 之间`;
  }
  if ('glowPx' in style && style.glowPx !== undefined && !inRange(style.glowPx, STYLE_LIMITS.glowMin, STYLE_LIMITS.glowMax)) {
    errors.glowPx = `光晕强度需在 ${STYLE_LIMITS.glowMin}–${STYLE_LIMITS.glowMax}px 之间`;
  }
  if ('dailyLimit' in style && style.dailyLimit !== undefined && !inRange(style.dailyLimit, STYLE_LIMITS.dailyLimitMin, STYLE_LIMITS.dailyLimitMax)) {
    errors.dailyLimit = `每日上限需在 ${STYLE_LIMITS.dailyLimitMin}–${STYLE_LIMITS.dailyLimitMax} 之间`;
  }
  if ('quantity' in style && style.quantity !== undefined && !inRange(style.quantity, STYLE_LIMITS.quantityMin, STYLE_LIMITS.quantityMax)) {
    errors.quantity = `可使用次数需在 ${STYLE_LIMITS.quantityMin}–${STYLE_LIMITS.quantityMax} 之间`;
  }
  if ('cooldownSec' in style && style.cooldownSec !== undefined && !inRange(style.cooldownSec, STYLE_LIMITS.cooldownMin, STYLE_LIMITS.cooldownMax)) {
    errors.cooldownSec = `冷却需在 ${STYLE_LIMITS.cooldownMin}–${STYLE_LIMITS.cooldownMax} 秒之间`;
  }
  if (draft.css && draft.css.length > STYLE_LIMITS.codeMax) {
    errors.css = `自定义 CSS 需 ≤ ${STYLE_LIMITS.codeMax} 字符`;
  }
  if (draft.js && draft.js.length > STYLE_LIMITS.codeMax) {
    errors.js = `自定义 JS 需 ≤ ${STYLE_LIMITS.codeMax} 字符`;
  }
  return errors;
}

/** 草稿 → 多场景预览用的展示 Token（直接喂给 CosmeticName/CosmeticAvatar 等）。 */
export function draftToPresentation(draft: StyleDraft): {
  nickname_color?: string;
  nickname_color_name?: string;
  nickname_color_style?: CosmeticDefStyle;
  avatar_frame?: string;
  avatar_frame_name?: string;
  avatar_frame_style?: CosmeticDefStyle;
  avatar_frame_url?: string;
  profile_effect?: string;
  profile_effect_name?: string;
  profile_effect_style?: CosmeticDefStyle;
} {
  const style = draftToStyle(draft);
  const name = draft.name.trim() || '预览';
  switch (draft.kind) {
    case 'nickname_color':
      return { nickname_color: 'preview', nickname_color_name: name, nickname_color_style: style };
    case 'avatar_frame':
      return {
        avatar_frame: 'preview',
        avatar_frame_name: name,
        avatar_frame_style: style,
        avatar_frame_url: draft.avatarFrameUrl || undefined
      };
    case 'profile_effect':
      return { profile_effect: 'preview', profile_effect_name: name, profile_effect_style: style };
    default:
      return {};
  }
}
