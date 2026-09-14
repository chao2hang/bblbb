// 手机端半屏弹层（Bottom Sheet）下滑关闭手势 —— 全站浮层标准。
// 规范见 docs/MOBILE-SHEET.md §2.4/§3；样式配套规则在
// lib/styles/mobile.css「§ 手机端半屏弹层」段（.app-sheet-grab/.app-sheet-backdrop）。
//
// 用法：action 绑定在弹层内渲染的抓手容器上，桌面（≥768px）自动 no-op：
//   <div class="app-sheet-grab" aria-hidden="true" use:sheetDrag={{ onClose: closePanel }}></div>
//
// 交互契约（对齐 mobile.css §1218 的 .user-menu sheet 先例）：
// - 仅触屏、且视口命中 SHEET_MEDIA_QUERY 时生效；
// - 向下滑动（delta>0）弹层实时跟随位移，向上回推钳制为 0（不越顶）；
// - 松手位移 > 阈值（默认 72px）回调 onClose（组件负责关闭，卸载即时、无退场动画），
//   否则回弹复位；touchmove preventDefault 防止页面滚动接管手势。
/** 手机端断点唯一常量：CSS（mobile.css）与 JS（本文件）共享同一值。 */
export const SHEET_MEDIA_QUERY = '(max-width: 767px)';

export interface SheetDragParams {
  /** 松手超过阈值时回调；组件负责关闭弹层。 */
  onClose: () => void;
  /** 关闭阈值（px），默认 72。 */
  threshold?: number;
}

/** action 生命周期句柄：模板 `use:` 与程序化调用（UserCard portal）共用。 */
export interface SheetDragHandle {
  update(params: SheetDragParams): void;
  destroy(): void;
}

export function sheetDrag(node: HTMLElement, params: SheetDragParams): SheetDragHandle {
  let onClose = params.onClose;
  let threshold = params.threshold ?? 72;

  let dragging = false;
  let startY = 0;
  let delta = 0;

  function matchesSheetQuery(): boolean {
    // jsdom/无 matchMedia 环境保持关闭（桌面语义），由调用方测试自行 mock。
    return typeof window.matchMedia === 'function' && window.matchMedia(SHEET_MEDIA_QUERY).matches;
  }

  function handleTouchStart(event: TouchEvent): void {
    if (dragging || event.touches.length === 0 || !matchesSheetQuery()) return;
    startY = event.touches[0].clientY;
    delta = 0;
    dragging = true;
    node.style.transition = 'none';
  }

  function handleTouchMove(event: TouchEvent): void {
    if (!dragging || event.touches.length === 0) return;
    delta = event.touches[0].clientY - startY;
    // 底部上滑弹层：向下滑（delta>0）为关闭手势；向上回推不越顶。
    node.style.transform = `translateY(${Math.max(delta, 0)}px)`;
    if (delta > 0) event.preventDefault();
  }

  function handleTouchEnd(): void {
    if (!dragging) return;
    dragging = false;
    if (delta > threshold) {
      node.style.transition = '';
      node.style.transform = '';
      onClose();
    } else {
      // 未过阈值：回弹复位，过渡结束后由后续 touchstart 重置 transition。
      node.style.transition = 'transform 200ms ease-out';
      node.style.transform = '';
    }
    delta = 0;
  }

  node.addEventListener('touchstart', handleTouchStart, { passive: true });
  node.addEventListener('touchmove', handleTouchMove, { passive: false });
  node.addEventListener('touchend', handleTouchEnd);
  node.addEventListener('touchcancel', handleTouchEnd);

  return {
    update(next: SheetDragParams) {
      onClose = next.onClose;
      threshold = next.threshold ?? 72;
    },
    destroy() {
      node.removeEventListener('touchstart', handleTouchStart);
      node.removeEventListener('touchmove', handleTouchMove);
      node.removeEventListener('touchend', handleTouchEnd);
      node.removeEventListener('touchcancel', handleTouchEnd);
    }
  };
}
