#!/usr/bin/env node
// 从 lucide-static（成熟图标库，ISC）生成板块图标目录：
//   frontend/src/lib/components/ui/icon-catalog.generated.ts
//
// - ICON_CATEGORIES：中文分类 → 精选图标名（板块图标挑选来源，IconPicker 用）；
// - catalogIcons：图标名 → SVG 内部标记（与 icons.ts 同格式，parseIconNodes 兼容）。
//
// 规则：
// - 每个名字必须在 node_modules/lucide-static/icons/<name>.svg 存在，否则报错
//   退出（防 typo 混进选择器后渲染空白）；
// - 图标标记只允许 path/circle/ellipse/rect/line/polyline/polygon
//   （Icon.svelte 编译期已知元素），出现其他标签 → 报错退出；
// - 与手写表（icons.ts HAND_ICONS）重名时手写表优先：生成文件仍收录该图标，
//   由 icons.ts 的展开顺序（...catalogIcons, ...HAND_ICONS）保证覆盖；
// - 输出确定性：同名同版本下重复运行零 diff（--check 模式用于 CI/检查）。
//
// 用法：npm run generate:icons [-- --check]
// 依赖：lucide-static（devDependencies，图标来源即后端原型的提取源）。

import { existsSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const FRONTEND = resolve(HERE, '..');
const ICONS_DIR = join(FRONTEND, 'node_modules', 'lucide-static', 'icons');
const OUT_FILE = join(FRONTEND, 'src', 'lib', 'components', 'ui', 'icon-catalog.generated.ts');
const CHECK = process.argv.includes('--check');

/** 精选板块图标目录（中文分类；名字 = lucide-static 图标文件名）。
 *  选型原则：覆盖社区常见板块主题、单色描边在小尺寸可辨认、
 *  避开同名家族变体（-2/-big 等）除非语义更明确。 */
const CATEGORIES = [
  {
    label: '通用',
    icons: [
      'message-square', 'message-circle', 'inbox', 'mail', 'send', 'bell', 'bookmark',
      'star', 'heart', 'flag', 'pin', 'tag', 'hash', 'link', 'paperclip', 'search',
      'eye', 'clock', 'calendar', 'archive', 'folder', 'file-text', 'book-open', 'pen-line',
    ],
  },
  {
    label: '技术开发',
    icons: [
      'code', 'terminal', 'braces', 'cpu', 'database', 'server', 'git-branch',
      'git-merge', 'git-pull-request', 'bug', 'wrench', 'hammer', 'settings', 'cog',
      'puzzle', 'package', 'box', 'boxes', 'layers', 'workflow', 'binary', 'hard-drive',
      'monitor', 'laptop', 'smartphone', 'keyboard', 'wifi', 'plug', 'bot', 'command',
    ],
  },
  {
    label: '创意设计',
    icons: [
      'palette', 'brush', 'paint-bucket', 'pen-tool', 'pencil', 'ruler', 'scissors',
      'feather', 'shapes', 'droplet', 'eraser', 'highlighter', 'image', 'camera',
      'aperture', 'frame', 'crop', 'wand-sparkles', 'sparkles',
    ],
  },
  {
    label: '社区互动',
    icons: [
      'users', 'user', 'user-plus', 'user-check', 'handshake', 'heart-handshake',
      'megaphone', 'thumbs-up', 'smile', 'party-popper', 'cake', 'gift', 'crown',
      'medal', 'trophy', 'award', 'badge-check', 'shield', 'shield-check', 'globe',
      'hand-heart', 'hand-helping',
    ],
  },
  {
    label: '商务职场',
    icons: [
      'briefcase', 'building', 'building-2', 'landmark', 'banknote', 'coins',
      'credit-card', 'wallet', 'piggy-bank', 'hand-coins', 'gem', 'diamond',
      'shopping-bag', 'shopping-cart', 'store', 'chart-line', 'chart-column',
      'chart-pie', 'trending-up', 'trending-down',
    ],
  },
  {
    label: '生活娱乐',
    icons: [
      'coffee', 'cup-soda', 'beer', 'pizza', 'cookie', 'utensils', 'ice-cream-cone',
      'music', 'headphones', 'radio', 'tv', 'clapperboard', 'film', 'ticket', 'mic',
      'guitar', 'drum', 'gamepad-2', 'dices', 'balloon', 'shirt', 'castle',
    ],
  },
  {
    label: '自然旅行',
    icons: [
      'leaf', 'sprout', 'tree-pine', 'flower-2', 'sun', 'moon', 'cloud', 'cloud-sun',
      'cloud-rain', 'cloud-lightning', 'snowflake', 'wind', 'rainbow', 'umbrella',
      'tent', 'mountain', 'waves', 'map', 'map-pin', 'navigation', 'compass', 'plane',
      'car', 'bus', 'train-front', 'bike', 'ship', 'anchor', 'sailboat', 'rocket',
      'telescope', 'bird', 'cat', 'dog', 'fish', 'rabbit',
    ],
  },
  {
    label: '学习教育',
    icons: [
      'graduation-cap', 'school', 'book', 'book-marked', 'library', 'notebook-pen',
      'lightbulb', 'brain', 'brain-circuit', 'atom', 'flask-conical', 'microscope',
      'calculator', 'sigma', 'languages',
    ],
  },
  {
    label: '安全管控',
    icons: [
      'lock', 'unlock', 'key', 'key-round', 'shield-alert', 'shield-off', 'ban',
      'power', 'triangle-alert', 'circle-alert', 'octagon-alert', 'eye-off',
      'fingerprint', 'scan-face',
    ],
  },
  {
    label: '状态符号',
    icons: [
      'check', 'circle-check', 'x', 'circle-x', 'minus', 'plus', 'info', 'play',
      'zap', 'flame', 'activity', 'heart-pulse', 'gauge', 'refresh-cw', 'rotate-cw',
      'loader-circle', 'hourglass', 'alarm-clock',
    ],
  },
];

// Icon.svelte 编译期已知 SVG 元素（parseIconNodes 的 tag 联合类型）。
const ALLOWED_TAGS = new Set([
  'path', 'circle', 'ellipse', 'rect', 'line', 'polyline', 'polygon',
]);
const NAME_RE = /^[a-z0-9-]+$/;

function extractInnerMarkup(svgSource, name) {
  // 去掉许可注释与 <svg> 外壳，取内部元素并折叠为单行自闭合格式。
  const inner = svgSource
    .replace(/<!--[\s\S]*?-->/g, '')
    .replace(/[\s\S]*?<svg[^>]*>/, '')
    .replace(/<\/svg>[\s\S]*$/, '')
    .replace(/\s+/g, ' ')
    .trim();
  if (!inner) throw new Error(`icon "${name}": 空的 SVG 内部标记`);
  for (const tag of inner.match(/<([a-z-]+)/g) ?? []) {
    const t = tag.slice(1);
    if (!ALLOWED_TAGS.has(t)) {
      throw new Error(
        `icon "${name}": 使用了不受支持的 <${t}>（parseIconNodes 仅支持 ${[...ALLOWED_TAGS].join('/')}），请在 Icon.svelte/icons.ts 扩展或从目录剔除该图标`
      );
    }
  }
  return inner;
}

if (!existsSync(ICONS_DIR)) {
  console.error(`generate:icons — 未找到 lucide-static（先 npm install）：${ICONS_DIR}`);
  process.exit(1);
}

const seen = new Map(); // name -> category label（查重）
for (const { label, icons } of CATEGORIES) {
  for (const name of icons) {
    if (!NAME_RE.test(name)) throw new Error(`icon "${name}"（${label}）：名字必须匹配 ${NAME_RE}`);
    if (seen.has(name)) throw new Error(`icon "${name}" 在 ${seen.get(name)} 与 ${label} 重复`);
    seen.set(name, label);
  }
}

const catalogIcons = {};
for (const { label, icons } of CATEGORIES) {
  for (const name of icons) {
    const file = join(ICONS_DIR, `${name}.svg`);
    if (!existsSync(file)) {
      console.error(`generate:icons — 图标不存在：${name}（分类「${label}」）。
  已安装 lucide-static 中最接近的候选：`);
      const prefix = name.split('-')[0];
      const candidates = readdirSync(ICONS_DIR)
        .filter((f) => f.startsWith(prefix))
        .slice(0, 8);
      for (const c of candidates) console.error(`    ${c.replace(/\.svg$/, '')}`);
      process.exit(1);
    }
    catalogIcons[name] = extractInnerMarkup(readFileSync(file, 'utf8'), name);
  }
}

// 与手写表重名提示（生成文件照常收录，icons.ts 展开顺序保证手写优先）。
const iconsTs = readFileSync(join(FRONTEND, 'src', 'lib', 'components', 'ui', 'icons.ts'), 'utf8');
const handKeys = new Set([...iconsTs.matchAll(/^  "([a-z0-9-]+)":/gm)].map((m) => m[1]));
const overlapped = [...seen.keys()].filter((n) => handKeys.has(n));
if (overlapped.length) {
  console.log(`generate:icons — 与手写表重名（手写优先）：${overlapped.join(', ')}`);
}

const total = seen.size;
const header = `/* 由 scripts/generate-icon-catalog.mjs 从 lucide-static 生成 — 禁止手改。
 *
 * 板块图标目录（M18）：ICON_CATEGORIES 为中文分类精选集（共 ${total} 图标），
 * 供 IconPicker（板块图标选择）浏览与搜索；catalogIcons 为渲染标记表，
 * 与 icons.ts 合并后作为 Icon.svelte 的 allowlist（手写键优先）。
 * 图标源：lucide-static（ISC），选集与调整改生成脚本后重新运行
 * \`npm run generate:icons\`。
 */

export interface IconCatalogEntry {
  name: string;
}

export interface IconCategory {
  /** 中文分类名（picker 分组与 optgroup 标签）。 */
  label: string;
  icons: IconCatalogEntry[];
}

/** 分类精选图标目录（板块图标挑选来源）。 */
export const ICON_CATEGORIES: IconCategory[] = [
`;

const body = CATEGORIES.map(({ label, icons }) => {
  const lines = icons.map((n) => `    { name: '${n}' },`).join('\n');
  return `  {\n    label: '${label}',\n    icons: [\n${lines}\n    ]\n  }`;
}).join(',\n');

const footer = `
];

/** 目录内全部图标的渲染标记（name → <tag .../> 序列，parseIconNodes 兼容）。 */
export const catalogIcons: Record<string, string> = {
`;
const iconLines = Object.entries(catalogIcons)
  .map(([n, markup]) => `  '${n}': ${JSON.stringify(markup)},`)
  .join('\n');
const tail = `
};
`;

const output = header + body + ',\n' + footer + iconLines + tail;

if (CHECK) {
  const current = existsSync(OUT_FILE) ? readFileSync(OUT_FILE, 'utf8') : '';
  if (current !== output) {
    console.error('generate:icons — 生成文件与当前 lucide-static 选集不一致（先运行 npm run generate:icons）');
    process.exit(1);
  }
  console.log(`generate:icons --check OK（${total} 图标 / ${CATEGORIES.length} 分类）`);
} else {
  writeFileSync(OUT_FILE, output);
  console.log(`generate:icons — 写入 ${OUT_FILE}（${total} 图标 / ${CATEGORIES.length} 分类）`);
}
