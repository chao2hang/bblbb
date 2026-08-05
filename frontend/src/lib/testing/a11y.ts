/**
 * M00-FRONTEND-07：可访问性基础测试夹具
 *
 * 六大夹具域：
 *  1. 减少动效 / 偏好媒体查询 —— jsdom 未实现 matchMedia，这里提供可切换的 mock。
 *  2. 键盘 —— 基于 @testing-library/user-event 的真实按键（Enter/Space 触发按钮激活）。
 *  3. 焦点 —— activeElement 断言、可聚焦元素枚举、Tab 遍历序。
 *  4. 表单错误关联 —— label[for] + aria-describedby + aria-invalid 的关联断言。
 *  5. 屏幕阅读器 —— role=status / role=alert / aria-live 播报断言。
 *  6. 触屏与指针 —— touch / pointer 事件构造。
 */
import { fireEvent } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { expect } from 'vitest';

/* ============ 1. 媒体查询 / 减少动效 ============ */

export interface MatchMediaMock extends MediaQueryList {
  __setMatches(matches: boolean): void;
}

const mqlInstances = new Set<MatchMediaMock>();
let mediaState: Record<string, boolean> = {};

/** 安装 matchMedia mock；jsdom 默认未实现。可在每个用例前调用以重置状态。 */
export function installMatchMedia(initial: Record<string, boolean> = {}): void {
  mediaState = { '(prefers-reduced-motion: reduce)': false, ...initial };
  mqlInstances.clear();

  window.matchMedia = ((query: string): MediaQueryList => {
    const listeners = new Set<EventListener>();
    const mql = {
      media: query,
      matches: Boolean(mediaState[query]),
      onchange: null,
      addEventListener: (type: string, cb: EventListener) => {
        if (type === 'change') listeners.add(cb);
      },
      removeEventListener: (type: string, cb: EventListener) => {
        if (type === 'change') listeners.delete(cb);
      },
      addListener: (cb: EventListener) => listeners.add(cb),
      removeListener: (cb: EventListener) => listeners.delete(cb),
      dispatchEvent: () => true,
      __setMatches(matches: boolean) {
        (this as { matches: boolean }).matches = matches;
        const ev = { matches, media: query } as MediaQueryListEvent;
        if (this.onchange) this.onchange.call(this, ev);
        listeners.forEach((cb) => cb.call(this, ev));
      }
    } as MatchMediaMock;
    mqlInstances.add(mql);
    return mql as unknown as MediaQueryList;
  }) as typeof window.matchMedia;
}

/** 切换某条媒体查询的匹配状态，并向所有已订阅实例派发 change 事件。 */
export function setMediaMatches(query: string, matches: boolean): void {
  mediaState[query] = matches;
  mqlInstances.forEach((mql) => {
    if (mql.media === query) mql.__setMatches(matches);
  });
}

/** 快捷设置 prefers-reduced-motion。 */
export function setPrefersReducedMotion(reduce: boolean): void {
  setMediaMatches('(prefers-reduced-motion: reduce)', reduce);
}

/** 订阅一条媒体查询；返回取消函数。组件可据此在运行时响应动效偏好。 */
export function watchMediaQuery(query: string): {
  matches: () => boolean;
  onChange: (cb: () => void) => () => void;
} {
  const mql = window.matchMedia(query);
  return {
    matches: () => mql.matches,
    onChange: (cb: () => void) => {
      mql.addEventListener('change', cb as EventListener);
      return () => mql.removeEventListener('change', cb as EventListener);
    }
  };
}

/* ============ 2. 键盘 ============ */

const USER_KEYS: Record<string, string> = {
  Enter: '{Enter}',
  Tab: '{Tab}',
  Escape: '{Escape}',
  ' ': ' ',
  ArrowUp: '{ArrowUp}',
  ArrowDown: '{ArrowDown}',
  ArrowLeft: '{ArrowLeft}',
  ArrowRight: '{ArrowRight}',
  Home: '{Home}',
  End: '{End}',
  PageUp: '{PageUp}',
  PageDown: '{PageDown}',
  Backspace: '{Backspace}',
  Delete: '{Delete}'
};

/** 聚焦目标后按下真实按键（Enter/Space 会触发原生按钮激活语义）。 */
export async function pressKey(target: HTMLElement, key: string): Promise<void> {
  const user = userEvent.setup();
  target.focus();
  await user.keyboard(USER_KEYS[key] ?? key);
}

/** 依次按下多个键（例如 Enter 后 Tab 检查焦点转移）。 */
export async function pressKeys(target: HTMLElement, keys: string[]): Promise<void> {
  const user = userEvent.setup();
  target.focus();
  for (const key of keys) {
    await user.keyboard(USER_KEYS[key] ?? key);
  }
}

/** 派发原始 keydown/keyup（用于验证组件自身的按键处理分支）。 */
export function fireKeyDown(target: Element, key: string, init: KeyboardEventInit = {}): void {
  fireEvent.keyDown(target, { key, ...init });
}

export function fireKeyUp(target: Element, key: string, init: KeyboardEventInit = {}): void {
  fireEvent.keyUp(target, { key, ...init });
}

/* ============ 3. 焦点 ============ */

const FOCUSABLE_SELECTOR = [
  'a[href]',
  'button:not([disabled])',
  'input:not([disabled])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
  'summary',
  '[contenteditable="true"]'
].join(',');

export function getFocused(): HTMLElement | null {
  return document.activeElement instanceof HTMLElement ? document.activeElement : null;
}

export function expectFocusedOn(element: Element): void {
  expect(element).toHaveFocus();
}

/** 按 DOM 顺序枚举容器内可聚焦元素（排除 disabled）。 */
export function focusableElements(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR));
}

export function focusFirst(container: HTMLElement): HTMLElement | null {
  const first = focusableElements(container)[0];
  first?.focus();
  return first ?? null;
}

/** 从 body 起连续 Tab，返回按 DOM 顺序获得焦点的元素（验证遍历序）。 */
export async function tabOrder(container: HTMLElement): Promise<HTMLElement[]> {
  const order: HTMLElement[] = [];
  const targets = focusableElements(container);
  for (let i = 0; i < targets.length; i++) {
    await userEvent.setup().tab();
    const focused = getFocused();
    if (focused && container.contains(focused)) order.push(focused);
  }
  return order;
}

/* ============ 4. 表单错误关联 ============ */

export interface FormControl {
  input: HTMLElement;
  label: HTMLLabelElement;
}

/** 按 label 文本找到表单控件（label[for] → 控件）。 */
export function getFormControl(container: HTMLElement, labelText: string): FormControl {
  const label = Array.from(container.querySelectorAll<HTMLLabelElement>('label')).find(
    (l) => l.textContent?.trim() === labelText
  );
  if (!label) throw new Error(`getFormControl: 未找到 label「${labelText}」`);
  const id = label.getAttribute('for');
  const input = id ? document.getElementById(id) : null;
  if (!input) throw new Error(`getFormControl: label「${labelText}」的 for=${id} 无对应控件`);
  return { input, label };
}

/**
 * 断言表单错误关联成立：
 *  - 控件带 aria-invalid="true"
 *  - aria-describedby 指向存在的错误元素
 *  - 错误元素文案匹配，且带 role=alert 或 aria-live
 */
export function expectErrorAssociation(control: HTMLElement, errorMessage: string): void {
  expect(control).toHaveAttribute('aria-invalid', 'true');
  const describedBy = control.getAttribute('aria-describedby') ?? '';
  const ids = describedBy.split(/\s+/).filter(Boolean);
  const errorEl = ids
    .map((id) => document.getElementById(id))
    .find((el): el is HTMLElement => el instanceof HTMLElement);
  expect(errorEl, `控件 aria-describedby=${describedBy} 应指向存在的错误元素`).not.toBeUndefined();
  expect(errorEl).toHaveTextContent(errorMessage);
  const sr = errorEl?.getAttribute('role') ?? errorEl?.getAttribute('aria-live');
  expect(sr, '错误提示应带 role=alert 或 aria-live 通知属性').toBeTruthy();
}

/* ============ 5. 屏幕阅读器 ============ */

export function findLiveRegion(
  container: HTMLElement,
  role: 'status' | 'alert' | 'log' = 'status'
): HTMLElement | null {
  const live = role === 'status' ? 'polite' : role === 'alert' ? 'assertive' : 'polite';
  return container.querySelector<HTMLElement>(`[role="${role}"], [aria-live="${live}"]`);
}

/** 断言容器内存在 role=status/alert（或 aria-live）区域并播报指定文案。 */
export function expectSrAnnouncement(container: HTMLElement, text: string, role: 'status' | 'alert' = 'status'): void {
  const region = findLiveRegion(container, role);
  expect(region).not.toBeNull();
  expect(region).toHaveTextContent(text);
}

/* ============ 6. 触屏与指针 ============ */

export interface TouchPoint {
  clientX: number;
  clientY: number;
  identifier?: number;
}

const TOUCH_MAP: Record<string, 'touchStart' | 'touchMove' | 'touchEnd' | 'touchCancel'> = {
  touchstart: 'touchStart',
  touchmove: 'touchMove',
  touchend: 'touchEnd',
  touchcancel: 'touchCancel'
};

export function fireTouchEvent(
  target: Element,
  type: 'touchstart' | 'touchmove' | 'touchend' | 'touchcancel',
  touches: TouchPoint[] = [{ clientX: 0, clientY: 0 }]
): void {
  fireEvent[TOUCH_MAP[type]](target, { touches });
}

export function fireTouchStart(target: Element, touches: TouchPoint[] = [{ clientX: 0, clientY: 0 }]): void {
  fireEvent.touchStart(target, { touches });
}

export function fireTouchEnd(target: Element, touches: TouchPoint[] = []): void {
  fireEvent.touchEnd(target, { touches });
}

function dispatchPointer(target: Element, type: 'pointerdown' | 'pointerup', init: PointerEventInit): void {
  const PointerCtor = (window as unknown as { PointerEvent?: new (t: string, i?: PointerEventInit) => PointerEvent })
    .PointerEvent;
  if (PointerCtor) {
    fireEvent[type === 'pointerdown' ? 'pointerDown' : 'pointerUp'](target, init);
    return;
  }
  // jsdom 尚未实现 PointerEvent：以 MouseEvent 兜底并回填 init 属性，
  // 使 pointerType 等字段可被断言。
  const ev = new MouseEvent(type, { bubbles: true, cancelable: true });
  Object.assign(ev, init);
  target.dispatchEvent(ev);
}

export function firePointerDown(target: Element, init: PointerEventInit = {}): void {
  dispatchPointer(target, 'pointerdown', init);
}

export function firePointerUp(target: Element, init: PointerEventInit = {}): void {
  dispatchPointer(target, 'pointerup', init);
}