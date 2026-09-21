import { describe, expect, it } from 'vitest';
import {
  harmonyStops,
  hexToHsl,
  hexToRgb,
  hslToHex,
  isHexColor,
  lerpHex,
  randomHarmonyStops,
  rgbToHex,
  sampleGradient
} from './gradient-utils';

describe('gradient-utils', () => {
  it('hex ↔ rgb ↔ hsl 往返', () => {
    expect(hexToRgb('#ff0080')).toEqual({ r: 255, g: 0, b: 128 });
    expect(rgbToHex({ r: 255, g: 0, b: 128 })).toBe('#ff0080');
    const hsl = hexToHsl('#ff0080');
    expect(hsl).not.toBeNull();
    expect(hslToHex(hsl!)).toBe('#ff0080');
    // 灰度（s=0 分支）
    expect(hslToHex({ h: 200, s: 0, l: 0.5 })).toBe('#808080');
    expect(hexToHsl('#808080')!.s).toBe(0);
  });

  it('非法输入回退与钳制', () => {
    expect(hexToRgb('red')).toBeNull();
    expect(hexToHsl('#fff')).toBeNull();
    expect(isHexColor('#ff0080')).toBe(true);
    expect(isHexColor('#FF0080')).toBe(true);
    expect(rgbToHex({ r: 300, g: -5, b: 12.6 })).toBe('#ff000d');
  });

  it('lerpHex 端点与中点', () => {
    expect(lerpHex('#000000', '#ffffff', 0)).toBe('#000000');
    expect(lerpHex('#000000', '#ffffff', 1)).toBe('#ffffff');
    expect(lerpHex('#000000', '#ffffff', 0.5)).toBe('#808080');
  });

  it('sampleGradient 采样', () => {
    expect(sampleGradient([], 0.5)).toBe('#888888');
    expect(sampleGradient(['#123456'], 0.9)).toBe('#123456');
    const mid = sampleGradient(['#000000', '#ffffff'], 0.5);
    expect(mid).toBe('#808080');
    // 三段：0.25 落在前两段中点
    expect(sampleGradient(['#000000', '#ff0000', '#ffffff'], 0.25)).toBe('#800000');
  });

  it('harmonyStops 生成合法色且数量钳制在 2–5', () => {
    for (const n of [1, 2, 3, 5, 9]) {
      const stops = harmonyStops('analogous', '#f472b6', n);
      expect(stops.length).toBeGreaterThanOrEqual(2);
      expect(stops.length).toBeLessThanOrEqual(5);
      expect(stops.every(isHexColor)).toBe(true);
    }
  });

  it('analogous 色相均匀铺开', () => {
    const stops = harmonyStops('analogous', '#ff0000', 3);
    const hues = stops.map((s) => hexToHsl(s)!.h);
    // 中间色即基色（0° 附近，允许 wrap）
    expect(Math.min(hues[1], 360 - hues[1])).toBeLessThan(2);
    // 相邻色相差 ~30°（环形距离，处理 0° 回绕）
    const circ = (a: number, b: number) => Math.min(Math.abs(a - b), 360 - Math.abs(a - b));
    expect(Math.abs(circ(hues[0], hues[1]) - 30)).toBeLessThan(2);
    expect(Math.abs(circ(hues[1], hues[2]) - 30)).toBeLessThan(2);
  });

  it('complementary 交替相差 ~180°', () => {
    const stops = harmonyStops('complementary', '#ff0000', 2);
    const hues = stops.map((s) => hexToHsl(s)!.h);
    const diff = Math.abs(hues[0] - hues[1]);
    expect(Math.abs(diff - 180)).toBeLessThan(3);
  });

  it('非法基色回退品牌粉且不报错', () => {
    const stops = harmonyStops('analogous', 'not-a-color', 3);
    expect(stops).toHaveLength(3);
    expect(stops.every(isHexColor)).toBe(true);
  });

  it('randomHarmonyStops 使用注入的随机源（确定性）', () => {
    const seq = [0.5, 0.5, 0.5];
    let i = 0;
    const stops = randomHarmonyStops(3, () => seq[i++ % seq.length]);
    expect(stops).toHaveLength(3);
    expect(stops.every(isHexColor)).toBe(true);
    // h=180 s=0.775 l=0.625 → 稳定输出
    expect(stops[1]).toBe(hslToHex({ h: 180, s: 0.775, l: 0.625 }));
  });
});
