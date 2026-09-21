// M07-SHOP-STUDIO-UX：预设模板库——「方便的构建」核心。
//
// 每个装扮类型内置若干调校好的预设：点选即穿上预览、再微调，把「从零
// 填表单」变成「挑模板改两处」。预设只是 StyleDraft 的参数补丁
// （params 不含 kind/name），applyPreset 与 createDraft 合并，边界值全部
// 落在 validateDraft 合法域内（单测保证）；最终防线仍是服务端 schema。

import { createDraft, type StyleDraft, type StyleKind } from './style-draft';

export interface StylePreset {
  key: string;
  label: string;
  /** 一句话描述效果（卡片副标题）。 */
  hint: string;
  /** 建议样式名（点选时若名称仍为空则带入，可改）。 */
  name: string;
  /** 参数补丁：不含 kind/name，与 createDraft(kind) 合并。 */
  params: Partial<Omit<StyleDraft, 'kind' | 'name'>>;
}

/** 把预设应用到新草稿：kind 以入口为准，name 为空时带入预设建议名。 */
export function applyPreset(kind: StyleKind, preset: StylePreset, name = ''): StyleDraft {
  return { ...createDraft(kind), ...preset.params, kind, name: name || preset.name };
}

export const STYLE_PRESETS: Record<StyleKind, StylePreset[]> = {
  nickname_color: [
    {
      key: 'rainbow-flow',
      label: '彩虹流光',
      hint: '五色渐变缓慢流动，最抢眼',
      name: '彩虹流光',
      params: {
        mode: 'gradient',
        stops: ['#f43f5e', '#f59e0b', '#10b981', '#3b82f6', '#8b5cf6'],
        animate: 'flow',
        durationMs: 8000
      }
    },
    {
      key: 'sakura',
      label: '樱花粉渐变',
      hint: '三段粉色渐变，柔和流动',
      name: '樱花粉渐变',
      params: { mode: 'gradient', stops: ['#ff9a9e', '#fecfef', '#f472b6'], animate: 'flow', durationMs: 5000 }
    },
    {
      key: 'cyber',
      label: '赛博霓虹',
      hint: '青紫粉快循环，电竞感',
      name: '赛博霓虹',
      params: { mode: 'gradient', stops: ['#22d3ee', '#a78bfa', '#f472b6'], animate: 'flow', durationMs: 2800 }
    },
    {
      key: 'sunset',
      label: '落日熔金',
      hint: '橙金红暖色渐变',
      name: '落日熔金',
      params: { mode: 'gradient', stops: ['#f97316', '#f59e0b', '#ef4444'], animate: 'flow', durationMs: 5000 }
    },
    {
      key: 'jade',
      label: '翡翠纯色',
      hint: '干净的单色昵称',
      name: '翡翠',
      params: { mode: 'solid', color: '#10b981' }
    },
    {
      key: 'violet-glow',
      label: '呼吸微光',
      hint: '紫色昵称带呼吸光晕',
      name: '呼吸微光',
      params: { mode: 'glow', color: '#a78bfa', animate: 'breathe', durationMs: 2800 }
    },
    {
      key: 'cyber-glitch',
      label: '赛博故障',
      hint: '霓虹双色渐变与故障抖动',
      name: '赛博故障',
      params: {
        mode: 'gradient',
        stops: ['#00ffff', '#ec4899', '#8b5cf6'],
        animate: 'glitch',
        durationMs: 2400,
        shadowPx: 8
      }
    },
    {
      key: 'blazing-flame',
      label: '烈焰微光',
      hint: '暖橙火焰跳动微光',
      name: '烈焰流火',
      params: {
        mode: 'glow',
        color: '#f97316',
        animate: 'fire',
        durationMs: 2000,
        shadowPx: 12
      }
    }
  ],
  avatar_frame: [
    {
      key: 'steam-galaxy-dream',
      label: '银河梦语 (Steam APNG)',
      hint: 'Steam 动效 APNG 梦幻银河星云框',
      name: '银河梦语',
      params: {
        color: '#8b5cf6',
        shape: 'circle',
        frameScale: 160,
        avatarFrameUrl: '/cosmetics/frames/galaxy_dream.png',
        css: '/* Steam 动效 APNG 头像框微光滤镜 */\nfilter: drop-shadow(0 0 8px rgba(139, 92, 246, 0.45));'
      }
    },
    {
      key: 'steam-ocean-glaze',
      label: '碧波琉璃 (Steam APNG)',
      hint: 'Steam 动效 APNG 碧海琉璃流光框',
      name: '碧波琉璃',
      params: {
        color: '#06b6d4',
        shape: 'circle',
        frameScale: 160,
        avatarFrameUrl: '/cosmetics/frames/ocean_glaze.png',
        css: '/* Steam 动效 APNG 头像框微光滤镜 */\nfilter: drop-shadow(0 0 8px rgba(6, 182, 212, 0.45));'
      }
    },
    {
      key: 'steam-cloud-blade',
      label: '流云锋影 (Steam APNG)',
      hint: 'Steam 动效 APNG 剑气流云风刃框',
      name: '流云锋影',
      params: {
        color: '#f59e0b',
        shape: 'rounded',
        frameScale: 120,
        avatarFrameUrl: '/cosmetics/frames/cloud_blade.png',
        css: '/* Steam 动效 APNG 头像框微光滤镜 */\nfilter: drop-shadow(0 0 8px rgba(245, 158, 11, 0.45));'
      }
    },
    {
      key: 'steam-nether-light',
      label: '冥咒浮光 (Steam APNG)',
      hint: 'Steam 动效 APNG 暗夜咒印浮光框',
      name: '冥咒浮光',
      params: {
        color: '#a855f7',
        shape: 'rounded',
        frameScale: 120,
        avatarFrameUrl: '/cosmetics/frames/nether_light.png',
        css: '/* Steam 动效 APNG 头像框微光滤镜 */\nfilter: drop-shadow(0 0 8px rgba(168, 85, 247, 0.45));'
      }
    },
    {
      key: 'steam-kraken',
      label: '海怪突袭 (Steam APNG)',
      hint: 'Steam 动效 APNG 巨型章鱼金属框',
      name: '海怪突袭',
      params: {
        color: '#64748b',
        shape: 'square',
        frameScale: 120,
        avatarFrameUrl: '/api/v1/steam-assets/frames/5b6d4ea867bd67b4db98f2ba965c1b8ee3a7d11e.png',
        css: '/* Steam 动效 APNG 头像框微光滤镜 */\nfilter: drop-shadow(0 0 6px rgba(100, 116, 139, 0.35));'
      }
    },
    {
      key: 'steam-fire',
      label: '烈焰焚天 (Steam APNG)',
      hint: 'Steam 动效 APNG 升腾烈焰方框',
      name: '烈焰焚天',
      params: {
        color: '#f97316',
        shape: 'square',
        frameScale: 120,
        avatarFrameUrl: '/cosmetics/frames/fire.png',
        css: '/* Steam 动效 APNG 头像框微光滤镜 */\nfilter: drop-shadow(0 0 8px rgba(249, 115, 22, 0.45));'
      }
    }
  ],
  profile_effect: [
    {
      key: 'steam-bg-cyberpunk',
      label: '赛博霓虹城 (Steam 动态背景)',
      hint: 'Steam 动效 赛博夜景流光大背景',
      name: '赛博霓虹城',
      params: {
        baseColor: '#090514',
        accentColor: '#ec4899',
        profileBackgroundUrl: '/api/v1/steam-assets/backgrounds/ab78d1ab87ad1d93029960fea8ac848b31520de0.jpg'
      }
    },
    {
      key: 'steam-bg-heart',
      label: '爱心波波 (Steam 动态背景)',
      hint: 'Steam 动效 动感心动粒子',
      name: '心动时刻',
      params: {
        baseColor: '#1a0510',
        accentColor: '#f43f5e',
        profileBackgroundUrl: '/api/v1/steam-assets/backgrounds/08eecb7e11de4875f51eeef31ba9f79fff27eb40.jpg'
      }
    },
    {
      key: 'aurora-haven',
      label: '极光天幕',
      hint: '天幕绿紫极光，飘拂漫溢',
      name: '极光天幕',
      params: {
        texture: 'aurora',
        baseColor: '#090d16',
        accentColor: '#10b981',
        animate: 'aurora',
        durationMs: 5000
      }
    },
    {
      key: 'matrix-rain',
      label: '数字矩阵',
      hint: '黑客科技全息扫描',
      name: '数字矩阵',
      params: {
        texture: 'matrix',
        baseColor: '#05100a',
        accentColor: '#22c55e',
        animate: 'scanline',
        durationMs: 4000
      }
    },
    {
      key: 'ocean-waves',
      label: '潮汐微浪',
      hint: '深蓝微浪，流体涌动',
      name: '潮汐微浪',
      params: {
        texture: 'waves',
        baseColor: '#0a192f',
        accentColor: '#38bdf8',
        animate: 'flow',
        durationMs: 6000
      }
    },
    {
      key: 'sparkle',
      label: '星芒空间',
      hint: '深底紫星芒，微光闪烁',
      name: '星芒空间',
      params: { texture: 'sparkle', baseColor: '#101827', accentColor: '#8b5cf6', animate: 'breathe', durationMs: 8000 }
    },
    {
      key: 'dark-stars',
      label: '暗夜星空',
      hint: '夜幕蓝星轨，缓慢微光',
      name: '暗夜星空',
      params: { texture: 'dark_stars', baseColor: '#0b1020', accentColor: '#60a5fa', animate: 'breathe', durationMs: 12000 }
    }
  ]
};

/** 类型建议价（金币）：发布面板的价格占位，管理员可改。 */
export const KIND_PRICE_SUGGESTIONS: Record<StyleKind, number> = {
  nickname_color: 200,
  avatar_frame: 300,
  profile_effect: 400
};
