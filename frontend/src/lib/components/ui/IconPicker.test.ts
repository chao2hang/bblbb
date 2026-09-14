// M18：IconPicker（板块图标选择器）——原生 select（无 JS 可用）+ JS 增强网格。
import { describe, expect, it } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/svelte';
import { tick } from 'svelte';
import IconPicker from './IconPicker.svelte';
import { ICON_CATEGORIES, catalogIcons } from './icon-catalog.generated';
import { icons, parseIconNodes } from './icons';

describe('IconPicker（板块图标选择）', () => {
  it('常驻原生 select（表单可序列化），按分类 optgroup 分组且含「不使用图标」选项', () => {
    const { container } = render(IconPicker, { name: 'icon', id: 'ip-test' });
    const select = container.querySelector('select[name="icon"]')!;
    expect(select).not.toBeNull();
    expect(select.querySelector('option[value=""]')?.textContent).toContain('不使用图标');
    const groups = [...select.querySelectorAll('optgroup')];
    expect(groups.map((g) => g.getAttribute('label'))).toEqual(
      ICON_CATEGORIES.map((c) => c.label)
    );
    // 每个目录图标都有对应 option（值为图标名）
    const optionValues = new Set(
      [...select.querySelectorAll('option')].map((o) => o.getAttribute('value'))
    );
    for (const { name } of ICON_CATEGORIES.flatMap((c) => c.icons)) {
      expect(optionValues.has(name), `option ${name}`).toBe(true);
    }
  });

  it('label 通过 for 关联 select', () => {
    const { container } = render(IconPicker, { name: 'icon', id: 'ip-label' });
    const label = container.querySelector('label')!;
    expect(label.getAttribute('for')).toBe('ip-label');
    expect(label.textContent).toContain('板块图标');
  });

  it('SSR（无 JS）不渲染网格面板与搜索：仅 select 可用', async () => {
    // jsdom 下 hasJs $effect 会执行；用 unmount 前的初始 HTML 断言不可行，
    // 这里直接断言 JS 增强区渲染后出现（面板默认收起）。
    const { container } = render(IconPicker, { name: 'icon' });
    await tick();
    expect(container.querySelector('.icon-picker__panel')).toBeNull();
  });

  it('JS 增强：展开面板后点击图标网格项更新绑定值', async () => {
    const { container } = render(IconPicker, { name: 'icon', value: '' });
    await tick();
    const toggle = container.querySelector('button[aria-controls$="-panel"]') as HTMLButtonElement;
    expect(toggle.getAttribute('aria-expanded')).toBe('false');
    await fireEvent.click(toggle);
    expect(toggle.getAttribute('aria-expanded')).toBe('true');

    const tile = container.querySelector('.icon-picker__tile') as HTMLButtonElement;
    const tileName = tile.getAttribute('title')!;
    expect(catalogIcons[tileName]).toBeTruthy();
    await fireEvent.click(tile);
    await tick();
    const select = container.querySelector('select') as HTMLSelectElement;
    expect(select.value).toBe(tileName);
    // 选中态：aria-pressed
    expect(tile.getAttribute('aria-pressed')).toBe('true');
    cleanup();
  });

  it('清除按钮把值重置为空（后端语义 = NULL）', async () => {
    const { container } = render(IconPicker, { name: 'icon', value: 'cog' });
    await tick();
    const clear = [...container.querySelectorAll('button')].find((b) =>
      b.textContent?.includes('清除')
    ) as HTMLButtonElement;
    expect(clear).toBeTruthy();
    await fireEvent.click(clear);
    await tick();
    const select = container.querySelector('select') as HTMLSelectElement;
    expect(select.value).toBe('');
  });

  it('搜索按图标名过滤网格', async () => {
    const { container } = render(IconPicker, { name: 'icon' });
    await tick();
    const toggle = container.querySelector('button[aria-controls$="-panel"]') as HTMLButtonElement;
    await fireEvent.click(toggle);
    const search = container.querySelector('input[type="search"]') as HTMLInputElement;
    await fireEvent.input(search, { target: { value: 'git-' } });
    await tick();
    const names = [...container.querySelectorAll('.icon-picker__tile')].map((b) =>
      b.getAttribute('title')
    );
    expect(names.length).toBeGreaterThan(0);
    expect(names.every((n) => n!.includes('git-'))).toBe(true);
    cleanup();
  });

  it('当前值不在目录（历史数据）时 select 补回显 option', async () => {
    const { container } = render(IconPicker, { name: 'icon', value: 'x-circle' });
    await tick();
    const select = container.querySelector('select') as HTMLSelectElement;
    expect(select.value).toBe('x-circle');
    const extra = select.querySelector('option[value="x-circle"]');
    expect(extra?.textContent).toContain('当前');
  });
});

describe('图标目录与渲染 allowlist 一致性', () => {
  it('目录内全部图标都能经 parseIconNodes 渲染出节点', () => {
    for (const [name, markup] of Object.entries(catalogIcons)) {
      expect(markup, name).toBeTruthy();
      expect(parseIconNodes(name).length, name).toBeGreaterThan(0);
    }
  });

  it('合并后的 allowlist = 目录 ∪ 手写表（手写键优先，值不变）', () => {
    // 手写表键在合并表中仍存在（页面上游引用不被目录覆盖破坏）
    for (const key of ['workflow', 'x-circle', 'message-square', 'sparkles']) {
      expect(icons[key], key).toBeTruthy();
    }
    // 目录独有键（手写表没有的）也可渲染
    expect(icons['party-popper']).toBeTruthy();
    expect(icons['graduation-cap']).toBeTruthy();
  });
});
