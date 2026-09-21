// M07-SHOP-STUDIO-UX：渐变编辑器的色彩工具（纯函数，可单测）。
// 只处理 #rrggbb；所有输出保证为合法 #rrggbb（与后端 validate_style 对齐）。

export interface Rgb {
  r: number;
  g: number;
  b: number;
}

export interface Hsl {
  h: number; // 0–360
  s: number; // 0–1
  l: number; // 0–1
}

const HEX = /^#[0-9a-fA-F]{6}$/;

export function isHexColor(v: unknown): v is string {
  return typeof v === 'string' && HEX.test(v);
}

export function hexToRgb(hex: string): Rgb | null {
  if (!isHexColor(hex)) return null;
  return {
    r: parseInt(hex.slice(1, 3), 16),
    g: parseInt(hex.slice(3, 5), 16),
    b: parseInt(hex.slice(5, 7), 16)
  };
}

export function rgbToHex(rgb: Rgb): string {
  const c = (v: number) => Math.max(0, Math.min(255, Math.round(v))).toString(16).padStart(2, '0');
  return `#${c(rgb.r)}${c(rgb.g)}${c(rgb.b)}`;
}

export function hexToHsl(hex: string): Hsl | null {
  const rgb = hexToRgb(hex);
  if (!rgb) return null;
  const r = rgb.r / 255;
  const g = rgb.g / 255;
  const b = rgb.b / 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  if (max === min) return { h: 0, s: 0, l };
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  let h: number;
  if (max === r) h = ((g - b) / d + (g < b ? 6 : 0)) * 60;
  else if (max === g) h = ((b - r) / d + 2) * 60;
  else h = ((r - g) / d + 4) * 60;
  return { h, s, l };
}

export function hslToHex(hsl: Hsl): string {
  const h = ((hsl.h % 360) + 360) % 360;
  const s = Math.max(0, Math.min(1, hsl.s));
  const l = Math.max(0, Math.min(1, hsl.l));
  if (s === 0) {
    const v = Math.round(l * 255);
    return rgbToHex({ r: v, g: v, b: v });
  }
  const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
  const p = 2 * l - q;
  const f = (t0: number): number => {
    let t = t0;
    if (t < 0) t += 1;
    if (t > 1) t -= 1;
    if (t < 1 / 6) return p + (q - p) * 6 * t;
    if (t < 1 / 2) return q;
    if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6;
    return p;
  };
  return rgbToHex({ r: f(h / 360 + 1 / 3) * 255, g: f(h / 360) * 255, b: f(h / 360 - 1 / 3) * 255 });
}

/** 两色 RGB 线性插值（t=0 → a，t=1 → b）。 */
export function lerpHex(a: string, b: string, t: number): string {
  const ca = hexToRgb(a) ?? { r: 0, g: 0, b: 0 };
  const cb = hexToRgb(b) ?? { r: 0, g: 0, b: 0 };
  const k = Math.max(0, Math.min(1, t));
  return rgbToHex({
    r: ca.r + (cb.r - ca.r) * k,
    g: ca.g + (cb.g - ca.g) * k,
    b: ca.b + (cb.b - ca.b) * k
  });
}

/** 渐变带上 position∈[0,1] 处的插值色（用于「点击空白添加色标」）。 */
export function sampleGradient(stops: string[], position: number): string {
  if (stops.length === 0) return '#888888';
  if (stops.length === 1) return stops[0];
  const p = Math.max(0, Math.min(1, position)) * (stops.length - 1);
  const i = Math.min(stops.length - 2, Math.floor(p));
  return lerpHex(stops[i], stops[i + 1], p - i);
}

export type HarmonyMode = 'analogous' | 'complementary';

/**
 * 基于基色生成协调色组（HSL 空间）：
 * - analogous：以基色为中心，±30° 步进铺开；
 * - complementary：基色与互补色（+180°）交替，明度交错拉开层次。
 * count 会被钳制到 2–5（后端 stops 边界）；基色非法时回退品牌粉。
 */
export function harmonyStops(mode: HarmonyMode, baseHex: string, count: number): string[] {
  const n = Math.max(2, Math.min(5, Math.round(count)));
  const base = hexToHsl(isHexColor(baseHex) ? baseHex : '#f472b6') ?? { h: 330, s: 0.8, l: 0.7 };
  const out: string[] = [];
  for (let i = 0; i < n; i += 1) {
    if (mode === 'analogous') {
      const offset = (i - (n - 1) / 2) * 30;
      out.push(hslToHex({ h: base.h + offset, s: base.s, l: base.l }));
    } else {
      const h = i % 2 === 0 ? base.h : base.h + 180;
      // 同一色相内的明度交错，避免相邻色过于接近
      const lShift = (Math.floor(i / 2) - (Math.ceil(n / 2) - 1) / 2) * 0.12;
      out.push(hslToHex({ h, s: base.s, l: Math.max(0.25, Math.min(0.85, base.l + lShift)) }));
    }
  }
  return out;
}

/** 随机灵感：随机基色的类比色组（rand 可注入便于测试）。 */
export function randomHarmonyStops(count: number, rand: () => number = Math.random): string[] {
  const base = hslToHex({ h: rand() * 360, s: 0.65 + rand() * 0.25, l: 0.55 + rand() * 0.15 });
  return harmonyStops('analogous', base, count);
}
