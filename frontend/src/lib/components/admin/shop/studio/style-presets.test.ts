import { describe, expect, it } from 'vitest';
import { applyPreset, KIND_PRICE_SUGGESTIONS, STYLE_PRESETS } from './style-presets';
import { STYLE_KINDS, validateDraft } from './style-draft';

describe('STYLE_PRESETS', () => {
  it('每个装扮类型至少有 3 个预设', () => {
    for (const kind of STYLE_KINDS) {
      expect(STYLE_PRESETS[kind].length, kind).toBeGreaterThanOrEqual(3);
    }
  });

  it('预设 key 在类型内唯一，且都有名称/描述/建议名', () => {
    for (const kind of STYLE_KINDS) {
      const keys = new Set<string>();
      for (const p of STYLE_PRESETS[kind]) {
        expect(keys.has(p.key), `${kind}:${p.key} 重复`).toBe(false);
        keys.add(p.key);
        expect(p.label.length).toBeGreaterThan(0);
        expect(p.hint.length).toBeGreaterThan(0);
        expect(p.name.length).toBeGreaterThan(0);
      }
    }
  });

  it('每个预设应用后都通过 validateDraft（边界镜像后端）', () => {
    for (const kind of STYLE_KINDS) {
      for (const p of STYLE_PRESETS[kind]) {
        const draft = applyPreset(kind, p);
        expect(draft.kind).toBe(kind);
        expect(draft.name).toBe(p.name);
        expect(validateDraft(draft), `${kind}:${p.key}`).toEqual({});
      }
    }
  });

  it('applyPreset 不携带 kind/name 补丁覆盖入口 kind', () => {
    const p = STYLE_PRESETS.nickname_color[0];
    const draft = applyPreset('nickname_color', { ...p, params: { ...p.params } });
    expect(draft.kind).toBe('nickname_color');
    // 显式 name 优先于预设建议名
    expect(applyPreset('nickname_color', p, '自定义名').name).toBe('自定义名');
  });

  it('渐变色标数量在 2–5 之间（后端 stops 边界）', () => {
    for (const p of STYLE_PRESETS.nickname_color) {
      if (p.params.stops) {
        expect(p.params.stops.length).toBeGreaterThanOrEqual(2);
        expect(p.params.stops.length).toBeLessThanOrEqual(5);
      }
    }
  });

  it('每个类型都有建议价且为正整数', () => {
    for (const kind of STYLE_KINDS) {
      expect(Number.isInteger(KIND_PRICE_SUGGESTIONS[kind]), kind).toBe(true);
      expect(KIND_PRICE_SUGGESTIONS[kind]).toBeGreaterThan(0);
    }
  });
});
