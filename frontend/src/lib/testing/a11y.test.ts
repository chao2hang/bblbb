// M00-FRONTEND-07：六大可访问性夹具的单元测试。
import { describe, expect, it, vi } from 'vitest';
import {
  firePointerDown,
  fireTouchEvent,
  fireTouchStart,
  focusableElements,
  getFormControl,
  installMatchMedia,
  pressKey,
  setPrefersReducedMotion,
  tabOrder,
  watchMediaQuery,
  expectErrorAssociation,
  expectSrAnnouncement,
  getFocused
} from './a11y';

describe('媒体查询 / 减少动效夹具', () => {
  it('matchMedia 默认未匹配，可切换并同步更新', () => {
    installMatchMedia();
    const mql = window.matchMedia('(prefers-reduced-motion: reduce)');
    expect(mql.matches).toBe(false);
    setPrefersReducedMotion(true);
    expect(mql.matches).toBe(true);
    setPrefersReducedMotion(false);
    expect(mql.matches).toBe(false);
  });

  it('watchMediaQuery 在偏好变化时触发 onChange，退订后不再触发', () => {
    installMatchMedia();
    const cb = vi.fn();
    const watcher = watchMediaQuery('(prefers-reduced-motion: reduce)');
    const off = watcher.onChange(cb);
    setPrefersReducedMotion(true);
    expect(cb).toHaveBeenCalledOnce();
    off();
    setPrefersReducedMotion(false);
    expect(cb).toHaveBeenCalledOnce();
  });
});

describe('键盘夹具', () => {
  it('pressKey Enter 激活原生按钮', async () => {
    document.body.innerHTML = '<button id="b">go</button>';
    const btn = document.getElementById('b')!;
    const clicked = vi.fn();
    btn.addEventListener('click', clicked);
    await pressKey(btn, 'Enter');
    expect(clicked).toHaveBeenCalledOnce();
  });

  it('pressKey Space 激活原生按钮', async () => {
    document.body.innerHTML = '<button id="b">go</button>';
    const btn = document.getElementById('b')!;
    const clicked = vi.fn();
    btn.addEventListener('click', clicked);
    await pressKey(btn, ' ');
    expect(clicked).toHaveBeenCalledOnce();
  });
});

describe('焦点夹具', () => {
  it('focusableElements 排除 disabled，按 DOM 顺序返回', () => {
    document.body.innerHTML = `
      <div id="c"><a href="/a">A</a><button>B</button><button disabled>C</button><input aria-label="D" /></div>`;
    const container = document.getElementById('c')!;
    const els = focusableElements(container);
    expect(els).toHaveLength(3);
    expect(els.map((el) => el.tagName)).toEqual(['A', 'BUTTON', 'INPUT']);
  });

  it('tabOrder 遍历序与 DOM 顺序一致', async () => {
    document.body.innerHTML = `
      <div id="c"><a href="/a">A</a><button>B</button><input aria-label="D" /></div>`;
    const container = document.getElementById('c')!;
    const order = await tabOrder(container);
    expect(order).toHaveLength(3);
    expect(order.map((el) => el.tagName)).toEqual(['A', 'BUTTON', 'INPUT']);
    expect(getFocused()).toBe(order[2]);
  });
});

describe('表单错误关联夹具', () => {
  it('正确关联：label[for] + aria-describedby + aria-invalid', () => {
    document.body.innerHTML = `
      <form>
        <label for="f-email">邮箱</label>
        <input id="f-email" aria-describedby="f-email-error" aria-invalid="true" />
        <p id="f-email-error" role="alert">邮箱格式不正确</p>
      </form>`;
    const input = document.getElementById('f-email')!;
    expectErrorAssociation(input, '邮箱格式不正确');
  });

  it('getFormControl 按 label 文本找到控件', () => {
    document.body.innerHTML = `
      <form>
        <label for="f-name">昵称</label>
        <input id="f-name" />
      </form>`;
    const form = document.querySelector('form')!;
    const { input, label } = getFormControl(form, '昵称');
    expect(label).toHaveAttribute('for', 'f-name');
    expect(input.tagName).toBe('INPUT');
  });

  it('缺少 aria 关联时断言失败', () => {
    document.body.innerHTML = `
      <form>
        <label for="f-x">密码</label>
        <input id="f-x" />
        <p id="f-x-error" role="alert">密码过短</p>
      </form>`;
    const input = document.getElementById('f-x')!;
    expect(() => expectErrorAssociation(input, '密码过短')).toThrow();
  });
});

describe('屏幕阅读器夹具', () => {
  it('role=status 区域播报文案', () => {
    document.body.innerHTML = '<div role="status">已保存</div>';
    expectSrAnnouncement(document.body, '已保存', 'status');
  });

  it('role=alert 区域播报文案', () => {
    document.body.innerHTML = '<div role="alert">网络错误</div>';
    expectSrAnnouncement(document.body, '网络错误', 'alert');
  });
});

describe('触屏 / 指针夹具', () => {
  it('fireTouchStart 派发 touches', () => {
    document.body.innerHTML = '<div id="t"></div>';
    const el = document.getElementById('t')!;
    let seen: unknown = null;
    el.addEventListener('touchstart', (e) => {
      seen = (e as TouchEvent).touches;
    });
    fireTouchStart(el, [{ clientX: 10, clientY: 20 }]);
    const touches = seen as { length: number } | null;
    expect(touches).not.toBeNull();
    expect(touches!.length).toBe(1);
  });

  it('fireTouchEvent 支持四类 touch 事件', () => {
    document.body.innerHTML = '<div id="t"></div>';
    const el = document.getElementById('t')!;
    const seen: string[] = [];
    for (const type of ['touchstart', 'touchmove', 'touchend', 'touchcancel'] as const) {
      el.addEventListener(type, () => seen.push(type));
    }
    fireTouchEvent(el, 'touchstart');
    fireTouchEvent(el, 'touchmove');
    fireTouchEvent(el, 'touchend', []);
    fireTouchEvent(el, 'touchcancel', []);
    expect(seen).toEqual(['touchstart', 'touchmove', 'touchend', 'touchcancel']);
  });

  it('firePointerDown 携带 pointerType', () => {
    document.body.innerHTML = '<div id="p"></div>';
    const el = document.getElementById('p')!;
    let pointerType = '';
    el.addEventListener('pointerdown', (e) => {
      pointerType = (e as PointerEvent).pointerType ?? '';
    });
    firePointerDown(el, { pointerType: 'touch' });
    expect(pointerType).toBe('touch');
  });
});