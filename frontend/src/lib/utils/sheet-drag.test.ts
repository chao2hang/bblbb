// 手机端半屏弹层下滑关闭手势（./sheet-drag.ts）单测。
// 复用 src/lib/testing/a11y.ts 夹具（matchMedia mock / 触屏事件构造）。
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fireTouchEvent, installMatchMedia, setMediaMatches } from '../testing/a11y';
import { SHEET_MEDIA_QUERY, sheetDrag } from './sheet-drag';

describe('sheetDrag（Bottom Sheet 下滑关闭手势）', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
    installMatchMedia({ [SHEET_MEDIA_QUERY]: false });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('桌面（断点未命中）：拖拽无效，不产生位移也不回调 onClose', () => {
    const onClose = vi.fn();
    const node = document.createElement('div');
    document.body.appendChild(node);
    const action = sheetDrag(node, { onClose });

    fireTouchEvent(node, 'touchstart', [{ clientX: 0, clientY: 100 }]);
    fireTouchEvent(node, 'touchmove', [{ clientX: 0, clientY: 200 }]);
    fireTouchEvent(node, 'touchend');

    expect(node.style.transform).toBe('');
    expect(onClose).not.toHaveBeenCalled();
    action.destroy?.();
  });

  it('移动端：向下滑动跟手位移，松手超过阈值回调 onClose 并清空位移', () => {
    setMediaMatches(SHEET_MEDIA_QUERY, true);
    const onClose = vi.fn();
    const node = document.createElement('div');
    document.body.appendChild(node);
    const action = sheetDrag(node, { onClose });

    fireTouchEvent(node, 'touchstart', [{ clientX: 0, clientY: 100 }]);
    fireTouchEvent(node, 'touchmove', [{ clientX: 0, clientY: 160 }]);
    expect(node.style.transform).toBe('translateY(60px)');

    fireTouchEvent(node, 'touchmove', [{ clientX: 0, clientY: 220 }]);
    fireTouchEvent(node, 'touchend');

    expect(onClose).toHaveBeenCalledTimes(1);
    expect(node.style.transform).toBe('');
    action.destroy?.();
  });

  it('移动端：向上回推钳制为 0（不越顶），松手未过阈值回弹且不回调', () => {
    setMediaMatches(SHEET_MEDIA_QUERY, true);
    const onClose = vi.fn();
    const node = document.createElement('div');
    document.body.appendChild(node);
    const action = sheetDrag(node, { onClose });

    fireTouchEvent(node, 'touchstart', [{ clientX: 0, clientY: 200 }]);
    fireTouchEvent(node, 'touchmove', [{ clientX: 0, clientY: 150 }]);
    expect(node.style.transform).toBe('translateY(0px)');

    fireTouchEvent(node, 'touchmove', [{ clientX: 0, clientY: 240 }]);
    fireTouchEvent(node, 'touchend');

    expect(onClose).not.toHaveBeenCalled();
    expect(node.style.transform).toBe('');
    action.destroy?.();
  });

  it('update 可热更新 onClose；destroy 后事件解绑', () => {
    setMediaMatches(SHEET_MEDIA_QUERY, true);
    const first = vi.fn();
    const second = vi.fn();
    const node = document.createElement('div');
    document.body.appendChild(node);
    const action = sheetDrag(node, { onClose: first });

    action.update?.({ onClose: second });

    fireTouchEvent(node, 'touchstart', [{ clientX: 0, clientY: 0 }]);
    fireTouchEvent(node, 'touchmove', [{ clientX: 0, clientY: 120 }]);
    fireTouchEvent(node, 'touchend');
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledTimes(1);

    action.destroy?.();
    fireTouchEvent(node, 'touchstart', [{ clientX: 0, clientY: 0 }]);
    fireTouchEvent(node, 'touchmove', [{ clientX: 0, clientY: 120 }]);
    fireTouchEvent(node, 'touchend');
    expect(second).toHaveBeenCalledTimes(1);
  });
});
