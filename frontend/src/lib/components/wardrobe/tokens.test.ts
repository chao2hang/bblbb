// 昵称颜色/特效白名单单元测试：动态特效 Token（彩虹/呼吸/渐变）必须能从
// 权益投影进 visual，否则商品买了也渲染不出来（M07-SHOP-06 回归）。
import { describe, expect, it } from 'vitest';
import {
  NICKNAME_COLORS,
  NICKNAME_EFFECTS,
  nicknameEffectClass,
  projectEntitlementTokens
} from './tokens';

describe('NICKNAME_EFFECTS（昵称颜色特效白名单）', () => {
  it('纯色 + 动态特效全部注册且有 class', () => {
    for (const key of Object.keys(NICKNAME_COLORS)) {
      expect(NICKNAME_EFFECTS[key]?.className).toBe(`nickname-solid-${key}`);
    }
    expect(NICKNAME_EFFECTS.rainbow).toMatchObject({ label: '彩虹流光', className: 'nickname-rainbow' });
    expect(NICKNAME_EFFECTS.breathing).toMatchObject({ label: '呼吸微光', className: 'nickname-breathing' });
    expect(NICKNAME_EFFECTS.gradient_sunset?.className).toBe('nickname-gradient-sunset');
    expect(NICKNAME_EFFECTS.gradient_ocean?.className).toBe('nickname-gradient-ocean');
    expect(NICKNAME_EFFECTS.gradient_aurora?.className).toBe('nickname-gradient-aurora');
  });

  it('nicknameEffectClass：白名单内返回 class，未知值返回 null', () => {
    expect(nicknameEffectClass('rainbow')).toBe('nickname-rainbow');
    expect(nicknameEffectClass('breathing')).toBe('nickname-breathing');
    expect(nicknameEffectClass('gold')).toBe('nickname-solid-gold');
    expect(nicknameEffectClass('not-a-token')).toBeNull();
    expect(nicknameEffectClass(42)).toBeNull();
  });
});

describe('projectEntitlementTokens（权益 → 视觉 Token 投影）', () => {
  it('动态特效 Token（nickname.color.rainbow 等）投影为 nickname_color', () => {
    const { visual, labels } = projectEntitlementTokens([
      'nickname.color.rainbow',
      'nickname.color.breathing'
    ]);
    expect(visual.nickname_color).toBe('breathing'); // 后值覆盖前值（同槽位）
    expect(labels).toEqual(['彩虹流光', '呼吸微光']);
  });

  it('纯色 Token 沿用中文名，渐变 Token 投影为对应值', () => {
    expect(projectEntitlementTokens(['nickname.color.gold']).visual.nickname_color).toBe('gold');
    expect(projectEntitlementTokens(['nickname.color.gold']).labels).toEqual(['鎏金']);
    expect(projectEntitlementTokens(['nickname.color.gradient_sunset']).visual.nickname_color).toBe(
      'gradient_sunset'
    );
  });

  it('未注册的值不投影（安全白名单不放宽）', () => {
    const { visual, labels } = projectEntitlementTokens(['nickname.color.hacker_css']);
    expect(visual.nickname_color).toBeUndefined();
    expect(labels).toEqual([]);
  });
});
