// computeMenuPosition 单测：翻面（flip）/ 右对齐回退（fallback）/ 视口夹紧
// （shift+clamp）三条几何规则的回归防护。jsdom 无布局，故抽纯函数测几何。
import { describe, expect, it } from 'vitest';
import { MENU_EDGE, MENU_GAP, computeMenuPosition } from './row-actions-position';

const rect = (top: number, left: number, height = 28, width = 28) => ({
  top,
  left,
  bottom: top + height,
  right: left + width
});

describe('computeMenuPosition（⋯ 行动菜单定位）', () => {
  it('常规位置：右对齐触发按钮、正下方展开（gap=4）', () => {
    const pos = computeMenuPosition(rect(100, 800), { width: 150, height: 108 }, { width: 1280, height: 720 });
    expect(pos).toEqual({ top: 100 + 28 + MENU_GAP, left: 800 + 28 - 150 });
  });

  it('视口下方放不下且上方放得下 → 翻到触发按钮上方（flip）', () => {
    // 触发按钮 bottom=700，菜单高 108：下方只余 12px，上方余 700px。
    const pos = computeMenuPosition(rect(672, 800), { width: 150, height: 108 }, { width: 1280, height: 720 });
    expect(pos.top).toBe(672 - MENU_GAP - 108);
    expect(pos.left).toBe(800 + 28 - 150);
  });

  it('上下都放不下 → 不翻面，夹紧到视口上缘（配合菜单 max-height 滚动）', () => {
    // 菜单比视口还高：top 夹到 MENU_EDGE。
    const pos = computeMenuPosition(rect(300, 100), { width: 150, height: 900 }, { width: 1280, height: 720 });
    expect(pos.top).toBe(MENU_EDGE);
  });

  it('右对齐会伸出左缘 → 回退为左对齐触发按钮', () => {
    // 触发按钮 left=50，菜单宽 150：右对齐 left=-100 <edge → 改 left=50。
    const pos = computeMenuPosition(rect(100, 50), { width: 150, height: 108 }, { width: 1280, height: 720 });
    expect(pos.left).toBe(50);
  });

  it('左对齐仍伸出右缘 → 夹紧到视口右缘内（shift）', () => {
    // 触发按钮贴右缘（left=1270），菜单宽 150：左对齐 1270 → 夹到 1280-8-150。
    const pos = computeMenuPosition(rect(100, 1270), { width: 150, height: 108 }, { width: 1280, height: 720 });
    expect(pos.left).toBe(1280 - MENU_EDGE - 150);
  });

  it('底部触发：垂直翻面与水平夹紧同时生效', () => {
    const pos = computeMenuPosition(rect(690, 1260), { width: 150, height: 108 }, { width: 1280, height: 720 });
    expect(pos.top).toBe(690 - MENU_GAP - 108); // 上方放得下 → 翻面
    expect(pos.left).toBe(1280 - MENU_EDGE - 150); // 右对齐/左对齐都越界 → 夹紧
  });

  it('贴顶触发：下方放得下 → 正常下方展开（不误翻面）；水平夹紧到右缘', () => {
    // 触发按钮 top=0：上方空间 0，但下方充足 → 保持下方展开 top=32。
    const pos = computeMenuPosition(rect(0, 1260), { width: 150, height: 108 }, { width: 1280, height: 720 });
    expect(pos.top).toBe(0 + 28 + MENU_GAP);
    expect(pos.left).toBe(1280 - MENU_EDGE - 150);
  });

  it('贴顶且下方也放不下 → 夹紧到 maxTop（视口底缘内侧）', () => {
    // 触发按钮 top=0、菜单高 700：下方 712 放不下 700+8 → 上方 0 也放不下 → 夹紧。
    const pos = computeMenuPosition(rect(0, 1260), { width: 150, height: 700 }, { width: 1280, height: 720 });
    expect(pos.top).toBe(720 - MENU_EDGE - 700);
  });

  it('含经典滚动条的口径：viewport 传 clientWidth 时右缘不再溢出', () => {
    // window.innerWidth=1280 但布局视口 1268（12px 滚动条）：夹紧依据传入视口。
    const pos = computeMenuPosition(rect(100, 1240), { width: 150, height: 108 }, { width: 1268, height: 720 });
    expect(pos.left).toBe(1268 - MENU_EDGE - 150);
  });
});
