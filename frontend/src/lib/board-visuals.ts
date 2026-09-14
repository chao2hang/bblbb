// BBLBB 板块视觉标识 — 持久化图标优先 + slug → 图标/颜色映射
//
// M18 起 boards 表持久化 icon 字段（lucide 图标名，管理端从图标库选择）：
// 板块数据自带 icon 时**优先使用持久化值**；未设置（NULL/未知名）时回退
// 本表 slug 映射，再回退默认视觉——保持既有板块视觉不回退。
//
// 真实后端 boards.icon 由迁移 0071 提供（backend/src/boards/validation.rs
// 校验 [a-z0-9-] 且 ≤64）；契约 Board schema 同步包含 icon（openapi/openapi.yaml）。
// 颜色仍取自原型 board.color 色板（slug 映射）：
//   #0088CC 蓝 / #B85C38 赭 / #12A89D 青 / #652D90 紫 / #F1592A 橙 / #808281 灰

export interface BoardVisuals {
  icon: string;
  color: string;
}

const BOARD_VISUALS: Record<string, BoardVisuals> = {
  // 生产种子板块（migrations/{sqlite,mysql,mariadb}/0005_seed_boards.sql）
  general: { icon: 'message-circle', color: '#F1592A' },
  tech: { icon: 'code', color: '#0088CC' },
  creative: { icon: 'palette', color: '#652D90' },
  help: { icon: 'search', color: '#12A89D' },
  news: { icon: 'book-open', color: '#B85C38' },
  // 原型板块（prototype/js/mock.js），兼容历史 slug
  'tech-essay': { icon: 'book-open', color: '#0088CC' },
  rust: { icon: 'cog', color: '#B85C38' },
  'web-dev': { icon: 'globe', color: '#12A89D' },
  opensource: { icon: 'git-branch', color: '#652D90' },
  chat: { icon: 'message-circle', color: '#F1592A' },
  meta: { icon: 'settings', color: '#808281' },
};

const DEFAULT_VISUALS: BoardVisuals = {
  icon: 'message-square',
  color: 'var(--color-accent)',
};

/**
 * 返回板块的图标与主题色。
 * 图标优先级：持久化 `icon`（boards.icon，管理端设置）→ slug 映射 → 默认；
 * 颜色恒取 slug 映射（持久化暂不含颜色）。未收录 slug 使用默认视觉。
 */
export function boardVisuals(slug: string, icon?: string | null): BoardVisuals {
  const base = BOARD_VISUALS[slug] ?? DEFAULT_VISUALS;
  return {
    icon: icon || base.icon,
    color: base.color,
  };
}
