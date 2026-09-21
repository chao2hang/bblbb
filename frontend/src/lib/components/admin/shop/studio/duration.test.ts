// 时效工具单元测试：友好选项 ↔ 秒的双向映射与中文展示。
import { describe, expect, it } from 'vitest';
import { DAY_SECONDS, choiceToSeconds, formatValidity, validityToChoice } from './duration';

describe('validityToChoice / choiceToSeconds', () => {
  it('永久（null/undefined）', () => {
    expect(validityToChoice(null)).toBe('permanent');
    expect(validityToChoice(undefined)).toBe('permanent');
    expect(choiceToSeconds('permanent')).toBeNull();
  });

  it('预设天数', () => {
    expect(validityToChoice(7 * DAY_SECONDS)).toBe('7d');
    expect(validityToChoice(30 * DAY_SECONDS)).toBe('30d');
    expect(validityToChoice(90 * DAY_SECONDS)).toBe('90d');
    expect(choiceToSeconds('7d')).toBe(7 * DAY_SECONDS);
    expect(choiceToSeconds('30d')).toBe(30 * DAY_SECONDS);
    expect(choiceToSeconds('90d')).toBe(90 * DAY_SECONDS);
  });

  it('非预设值归入 custom 并按天数取整', () => {
    expect(validityToChoice(14 * DAY_SECONDS)).toBe('custom');
    expect(choiceToSeconds('custom', 14)).toBe(14 * DAY_SECONDS);
    expect(choiceToSeconds('custom', 0)).toBe(DAY_SECONDS); // 至少 1 天
    expect(choiceToSeconds('custom', 2.7)).toBe(3 * DAY_SECONDS);
  });
});

describe('formatValidity', () => {
  it('永久 / 天 / 小时 / 秒', () => {
    expect(formatValidity(null)).toBe('永久');
    expect(formatValidity(0)).toBe('永久');
    expect(formatValidity(30 * DAY_SECONDS)).toBe('30 天');
    expect(formatValidity(3600)).toBe('1 小时');
    expect(formatValidity(90)).toBe('90 秒');
  });
});
