// M18-ADMIN-OPS：RowActionsMenu（「⋯」行动作下拉菜单）的**纯定位函数**。
//
// 抽成纯函数的原因：组件里只做「测量 + 调用」，翻面/移位/夹紧的几何决策全部
// 在这里单测覆盖（jsdom 没有布局，组件层测不了几何）。
//
// 术语（这个交互模式业界通名：Dropdown Menu / Overflow Menu，表格行场景
// 常称 Row Actions Menu，触发按钮俗称 kebab menu）：
// - 下方优先展开，放不下翻到上方（flip）；
// - 右对齐触发按钮，被左缘裁剪时改左对齐，最后在视口内水平移位（shift）；
// - 两个方向都夹紧在视口内（clamp），配合菜单自身 max-height 滚动。

/** 与触发按钮的间距（px）。 */
export const MENU_GAP = 4;
/** 菜单与视口边缘的最小间距（px），防止贴边/出血。 */
export const MENU_EDGE = 8;

export interface Rect {
  top: number;
  left: number;
  bottom: number;
  right: number;
}

export interface Size {
  width: number;
  height: number;
}

export interface Viewport {
  width: number;
  height: number;
}

/**
 * 计算 fixed 菜单的左上角坐标。
 *
 * `viewport` 必须传 `documentElement.clientWidth/Height`（布局视口，**不含**
 * 经典滚动条宽度）——用 `window.innerWidth` 会让菜单相对触发按钮左偏一个
 * 滚动条宽度（约 12px，实测复现过）。
 */
export function computeMenuPosition(
  trigger: Rect,
  menu: Size,
  viewport: Viewport
): { top: number; left: number } {
  const gap = MENU_GAP;
  const edge = MENU_EDGE;

  // ── 垂直：下方优先；下方放不下且上方放得下 → 翻到上方；最后夹紧 ──
  let top = trigger.bottom + gap;
  const fitsBelow = top + menu.height <= viewport.height - edge;
  const fitsAbove = trigger.top - gap - menu.height >= edge;
  if (!fitsBelow && fitsAbove) {
    top = trigger.top - gap - menu.height;
  }
  const maxTop = Math.max(edge, viewport.height - edge - menu.height);
  top = Math.max(edge, Math.min(top, maxTop));

  // ── 水平：右对齐触发按钮；左缘放不下改左对齐；最后夹紧（shift） ──
  let left = trigger.right - menu.width;
  if (left < edge) {
    left = trigger.left;
  }
  const maxLeft = Math.max(edge, viewport.width - edge - menu.width);
  left = Math.max(edge, Math.min(left, maxLeft));

  return { top: Math.round(top), left: Math.round(left) };
}
