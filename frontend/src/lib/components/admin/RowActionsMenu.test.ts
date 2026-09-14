// M18-ADMIN-OPS：RowActionsMenu（行动作「⋯」三点菜单 = Dropdown/Overflow Menu）。
//
// 定位契约 v2：菜单 portal 到 document.body（不被 overflow:hidden 卡片裁剪，
// 也不受 transform/backdrop-filter 祖先劫持 fixed 包含块）；打开时按触发按钮
// 实时矩形定位（右对齐、下方优先、放不下翻上方、视口内夹紧），滚动/resize
// 跟随重新定位（不再「一滚就关」掩盖一次测量定终身）。几何规则在
// row-actions-position.test.ts 纯函数单测；本文件测组件行为契约。
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render } from '@testing-library/svelte';
import { tick } from 'svelte';
import RowActionsMenu from './RowActionsMenu.svelte';

function domRect(x: number, y: number, width = 28, height = 28): DOMRect {
  return {
    x,
    y,
    top: y,
    left: x,
    right: x + width,
    bottom: y + height,
    width,
    height,
    toJSON: () => ({})
  } as DOMRect;
}

/** mock 触发按钮矩形（jsdom 无布局）；菜单尺寸默认 150x108（min-width + 3 项）。 */
function mockGeometry(container: HTMLElement, triggerRect: DOMRect, menuSize = { width: 150, height: 108 }) {
  const trigger = container.querySelector('.row-actions__trigger') as HTMLButtonElement;
  vi.spyOn(trigger, 'getBoundingClientRect').mockReturnValue(triggerRect);
  return trigger;
}

function queryMenu(): HTMLElement | null {
  // 菜单 portal 到 body：断言一律走 document，而不是组件容器。
  return document.querySelector('.row-actions__menu');
}

function items(): HTMLButtonElement[] {
  return [...(queryMenu()?.querySelectorAll<HTMLButtonElement>('button[role="menuitem"]') ?? [])];
}

/** 等待一帧：rAF 节流的跟随定位在 jsdom 里由真实定时器驱动。 */
function frame(): Promise<void> {
  return new Promise((resolve) => requestAnimationFrame(() => resolve()));
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe('RowActionsMenu（⋯ 行动菜单）', () => {
  it('初始关闭：不渲染菜单 DOM（SSR 基线），aria-expanded=false', async () => {
    const { container } = render(RowActionsMenu, {
      label: '更多操作：帖子 p-1',
      actions: [{ label: '设为精华', run: () => {} }]
    });
    expect(queryMenu()).toBeNull();
    const trigger = container.querySelector('.row-actions__trigger') as HTMLButtonElement;
    expect(trigger).toHaveAttribute('aria-expanded', 'false');
    expect(trigger).toHaveAttribute('aria-haspopup', 'menu');
  });

  it('点击打开：菜单 portal 到 document.body + role=menu + aria-expanded/controls', async () => {
    const { container } = render(RowActionsMenu, {
      label: '更多操作：帖子 p-1',
      actions: [{ label: '设为精华', run: () => {} }]
    });
    const trigger = mockGeometry(container, domRect(800, 200));
    await fireEvent.click(trigger);
    await tick();

    const menu = queryMenu();
    expect(menu).not.toBeNull();
    expect(menu!.parentElement).toBe(document.body); // portal：不在组件容器内
    expect(menu).toHaveAttribute('role', 'menu');
    expect(trigger).toHaveAttribute('aria-expanded', 'true');
    expect(trigger.getAttribute('aria-controls')).toBe(menu!.id);

    // 定位：右对齐触发按钮、下方展开（left/top 由 JS 实时写入，jsdom 1024x768）。
    // jsdom 菜单无布局 → 尺寸回退 min-width 前为 0：仅断言坐标已写入。
    const style = (menu!.getAttribute('style') ?? '').replace(/\s+/g, '');
    expect(style).toContain('visibility:visible');
  });

  it('常规定位：右对齐触发按钮、正下方展开（gap=4，clientWidth 排除滚动条）', async () => {
    const { container } = render(RowActionsMenu, {
      label: '更多操作',
      actions: [
        { label: '一', run: () => {} },
        { label: '二', run: () => {} },
        { label: '三', run: () => {} }
      ]
    });
    const trigger = mockGeometry(container, domRect(800, 200));
    await fireEvent.click(trigger);
    await tick();

    const menu = queryMenu()!;
    vi.spyOn(menu, 'getBoundingClientRect').mockReturnValue(domRect(0, 0, 150, 108));
    // 模拟第二帧定位（字体/布局稳定后的二次定位）。
    window.dispatchEvent(new Event('resize'));
    await frame();

    const style = (menu.getAttribute('style') ?? '').replace(/\s+/g, '');
    expect(style).toContain(`top:232px`); // bottom(228) + gap(4)
    expect(style).toContain(`left:678px`); // right(828) - width(150)，右对齐
  });

  it('视口下方放不下 → 翻到触发按钮上方（flip）', async () => {
    const { container } = render(RowActionsMenu, {
      label: '更多操作',
      actions: [
        { label: '一', run: () => {} },
        { label: '二', run: () => {} },
        { label: '三', run: () => {} }
      ]
    });
    // bottom=700，菜单高 108：jsdom 视口高 768 → 下方放不下，上方放得下。
    const trigger = mockGeometry(container, domRect(800, 672));
    await fireEvent.click(trigger);
    await tick();
    const menu = queryMenu()!;
    vi.spyOn(menu, 'getBoundingClientRect').mockReturnValue(domRect(0, 0, 150, 108));
    window.dispatchEvent(new Event('resize'));
    await frame();

    const style = (menu.getAttribute('style') ?? '').replace(/\s+/g, '');
    expect(style).toContain('top:560px'); // top(672) - gap(4) - 108
  });

  it('点菜单项：先关闭菜单，再执行 run()', async () => {
    const run = vi.fn();
    const { container } = render(RowActionsMenu, {
      label: '更多操作',
      actions: [{ label: '编辑规则', run }]
    });
    await fireEvent.click(container.querySelector('.row-actions__trigger') as HTMLButtonElement);
    await tick();
    await fireEvent.click(items()[0]);
    await tick();
    expect(run).toHaveBeenCalledTimes(1);
    expect(queryMenu()).toBeNull();
  });

  it('danger 项红色、disabled 项不执行也不关闭', async () => {
    const run = vi.fn();
    const { container } = render(RowActionsMenu, {
      label: '更多操作',
      actions: [
        { label: '设为精华', run: () => {} },
        { label: '删除', danger: true, disabled: true, hint: '已锁定', run }
      ]
    });
    await fireEvent.click(container.querySelector('.row-actions__trigger') as HTMLButtonElement);
    await tick();
    expect(items()[1]).toHaveClass('is-danger');
    expect(items()[1]).toBeDisabled();
    expect(items()[1].getAttribute('title')).toBe('已锁定');
    await fireEvent.click(items()[1]);
    await tick();
    expect(run).not.toHaveBeenCalled();
    expect(queryMenu()).not.toBeNull();
  });

  it('Escape 关闭并把焦点还给触发按钮', async () => {
    const { container } = render(RowActionsMenu, {
      label: '更多操作',
      actions: [{ label: '编辑规则', run: () => {} }]
    });
    const trigger = container.querySelector('.row-actions__trigger') as HTMLButtonElement;
    await fireEvent.click(trigger);
    await tick();
    expect(queryMenu()).not.toBeNull();
    await fireEvent.keyDown(document, { key: 'Escape' });
    await tick();
    expect(queryMenu()).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });

  it('点击外部（pointerdown/click）关闭；portal 菜单内点击不算外部', async () => {
    const { container } = render(RowActionsMenu, {
      label: '更多操作',
      actions: [{ label: '编辑规则', run: () => {} }]
    });
    const trigger = container.querySelector('.row-actions__trigger') as HTMLButtonElement;
    await fireEvent.click(trigger);
    await tick();
    await fireEvent.click(queryMenu()!); // 菜单内部（挂在 body 下）→ 保持打开
    await tick();
    expect(queryMenu()).not.toBeNull();

    await fireEvent.pointerDown(document.body); // 外部 pointerdown → 关闭
    await tick();
    expect(queryMenu()).toBeNull();

    await fireEvent.click(trigger);
    await tick();
    await fireEvent.click(document.body); // 外部 click 兜底（无 PointerEvent 环境）
    await tick();
    expect(queryMenu()).toBeNull();
  });

  it('滚动（含内层滚动容器）与 resize：菜单跟随重新定位而不是悬空/关闭', async () => {
    const { container } = render(RowActionsMenu, {
      label: '更多操作',
      actions: [
        { label: '一', run: () => {} },
        { label: '二', run: () => {} },
        { label: '三', run: () => {} }
      ]
    });
    const trigger = mockGeometry(container, domRect(800, 200));
    await fireEvent.click(trigger);
    await tick();
    const menu = queryMenu()!;
    vi.spyOn(menu, 'getBoundingClientRect').mockReturnValue(domRect(0, 0, 150, 108));

    // 触发按钮随页面滚动移到新位置 → 菜单坐标跟着更新，菜单保持打开。
    vi.spyOn(trigger, 'getBoundingClientRect').mockReturnValue(domRect(800, 300));
    await fireEvent.scroll(document.body);
    await frame();
    expect(queryMenu()).not.toBeNull();
    expect((menu.getAttribute('style') ?? '').replace(/\s+/g, '')).toContain('top:332px');

    // 锚点行完全滚出视口 → 菜单关闭（不留钉在视口边缘的幽灵菜单）。
    vi.spyOn(trigger, 'getBoundingClientRect').mockReturnValue(domRect(800, -500));
    await fireEvent.scroll(document.body);
    await frame();
    expect(queryMenu()).toBeNull();
  });

  it('键盘：触发按钮 ↑/↓ 打开并聚焦末/首可用项；菜单内 ↑/↓/Home/End 循环', async () => {
    const { container } = render(RowActionsMenu, {
      label: '更多操作',
      actions: [
        { label: '一', run: () => {} },
        { label: '二', run: () => {} },
        { label: '三', disabled: true, run: () => {} }
      ]
    });
    const trigger = container.querySelector('.row-actions__trigger') as HTMLButtonElement;
    await fireEvent.keyDown(trigger, { key: 'ArrowDown' });
    await tick();
    expect(queryMenu()).not.toBeNull();
    expect(document.activeElement).toBe(items()[0]);

    await fireEvent.keyDown(items()[0], { key: 'ArrowDown' });
    expect(document.activeElement).toBe(items()[1]);
    await fireEvent.keyDown(items()[1], { key: 'ArrowDown' });
    expect(document.activeElement).toBe(items()[0]); // 第 3 项 disabled → 循环回绕到首项
    // 键盘流：始终从**当前聚焦项**派发（与真实用户输入一致）。
    await fireEvent.keyDown(document.activeElement!, { key: 'ArrowUp' });
    expect(document.activeElement).toBe(items()[1]); // 从首项 ↑ → 末个可用项
    await fireEvent.keyDown(document.activeElement!, { key: 'Home' });
    expect(document.activeElement).toBe(items()[0]);
    await fireEvent.keyDown(document.activeElement!, { key: 'End' });
    expect(document.activeElement).toBe(items()[1]);
    // 注：Enter 激活聚焦按钮是浏览器原生行为（click → pick → 关闭），jsdom
    // 不实现按钮激活默认动作，不在组件层断言；点菜单项路径已有专门用例。
  });

  it('触发按钮 ArrowUp 打开聚焦末个可用项；菜单内 Tab 关闭', async () => {
    const { container } = render(RowActionsMenu, {
      label: '更多操作',
      actions: [
        { label: '一', run: () => {} },
        { label: '二', run: () => {} },
        { label: '三', disabled: true, run: () => {} }
      ]
    });
    const trigger = container.querySelector('.row-actions__trigger') as HTMLButtonElement;
    await fireEvent.keyDown(trigger, { key: 'ArrowUp' });
    await tick();
    expect(document.activeElement).toBe(items()[1]);
    await fireEvent.keyDown(items()[1], { key: 'Tab' });
    await tick();
    expect(queryMenu()).toBeNull();
  });
});
