// BBLBB 板块视觉标识 — slug → 图标/颜色映射
//
// 真实后端 boards 表未持久化 icon/color 字段（契约 Board schema 亦未包含），
// 为还原原型（prototype/js/mock.js 的 boards 数据）的差异化板块视觉，
// 在此集中维护映射。新增板块时在 BOARD_VISUALS 中补充条目即可。
//
// 颜色取自原型 board.color 色板：
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

/** 返回板块的图标与主题色；未收录的 slug 使用默认视觉。 */
export function boardVisuals(slug: string): BoardVisuals {
  return BOARD_VISUALS[slug] ?? DEFAULT_VISUALS;
}