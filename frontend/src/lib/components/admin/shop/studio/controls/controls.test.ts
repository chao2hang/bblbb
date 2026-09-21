import { describe, expect, it } from 'vitest';
import { fireEvent, render } from '@testing-library/svelte';
import GradientEditor from './GradientEditor.svelte';
import SegmentedSpeed from './SegmentedSpeed.svelte';
import SliderField from './SliderField.svelte';

describe('SegmentedSpeed', () => {
  it('渲染 5 个原生 radio（name=durationMs），当前值最近的分段被选中', () => {
    const { container } = render(SegmentedSpeed, { value: 5000 });
    const radios = container.querySelectorAll<HTMLInputElement>('input[type="radio"][name="durationMs"]');
    expect(radios).toHaveLength(5);
    expect([...radios].map((r) => r.value)).toEqual(['1500', '2800', '5000', '8000', '20000']);
    expect(container.querySelector<HTMLInputElement>('input[value="5000"]')?.checked).toBe(true);
  });

  it('非分段点值（如 12000）选中最近分段展示', () => {
    const { container } = render(SegmentedSpeed, { value: 12000 });
    expect(container.querySelector<HTMLInputElement>('input[value="8000"]')?.checked).toBe(true);
  });
});

describe('SliderField', () => {
  it('渲染具名 range 输入与数值徽标', () => {
    const { container } = render(SliderField, { value: 4, min: 1, max: 8, name: 'widthPx', label: '边框粗细', unit: 'px', id: 't-width' });
    const range = container.querySelector<HTMLInputElement>('input[type="range"][name="widthPx"]');
    expect(range).not.toBeNull();
    expect(range?.min).toBe('1');
    expect(range?.max).toBe('8');
    expect(range?.value).toBe('4');
    expect(container.querySelector('.slf-badge')?.textContent).toBe('4px');
  });
});

describe('GradientEditor', () => {
  it('每个色标渲染一个 name="stops" 的原生 color input 与一个拖拽手柄', () => {
    const { container } = render(GradientEditor, { stops: ['#ff0000', '#00ff00', '#0000ff'] });
    const inputs = container.querySelectorAll<HTMLInputElement>('input[type="color"][name="stops"]');
    expect(inputs).toHaveLength(3);
    expect([...inputs].map((i) => i.value)).toEqual(['#ff0000', '#00ff00', '#0000ff']);
    expect(container.querySelectorAll('.ge-handle')).toHaveLength(3);
  });

  it('渐变条背景按色标生成', () => {
    const { container } = render(GradientEditor, { stops: ['#ff0000', '#0000ff'] });
    const track = container.querySelector('.ge-track');
    expect(track?.getAttribute('style')).toContain('linear-gradient(90deg, #ff0000, #0000ff)');
  });

  it('「互补色」灵感按钮替换全部色标且数量不变', async () => {
    const { container, getByText } = render(GradientEditor, { stops: ['#ff0000', '#00ff00'] });
    await fireEvent.click(getByText('互补色'));
    const inputs = container.querySelectorAll<HTMLInputElement>('input[type="color"][name="stops"]');
    expect(inputs).toHaveLength(2);
    // 互补色模式：第二色与第一色色相差 ~180°（#ff0000 → 青色系）
    expect(inputs[1].value).not.toBe('#00ff00');
  });

  it('「减色标」在下界（2 个）时禁用', () => {
    const { getByText } = render(GradientEditor, { stops: ['#ff0000', '#00ff00'] });
    expect(getByText('− 减色标').closest('button')?.disabled).toBe(true);
  });

  it('「加色标」追加色标（不超过 5 个）', async () => {
    const { container, getByText } = render(GradientEditor, { stops: ['#ff0000', '#00ff00'] });
    await fireEvent.click(getByText('+ 加色标'));
    expect(container.querySelectorAll('input[type="color"][name="stops"]')).toHaveLength(3);
  });
});
