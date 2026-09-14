import { describe, expect, it } from 'vitest';
import { formatChatTime, formatRelative, formatTime } from './utils';

describe('formatChatTime (微信风格时间格式化)', () => {
  it('当天时间显示 HH:mm', () => {
    const now = new Date();
    now.setHours(11, 33, 0, 0);
    expect(formatChatTime(now.getTime())).toBe('11:33');
  });

  it('昨天时间显示 昨天 HH:mm', () => {
    const yesterday = new Date();
    yesterday.setDate(yesterday.getDate() - 1);
    yesterday.setHours(9, 15, 0, 0);
    expect(formatChatTime(yesterday.getTime())).toBe('昨天 09:15');
  });

  it('跨年时间显示 YYYY年M月d日 HH:mm', () => {
    const pastYear = new Date('2020-05-12T14:20:00');
    expect(formatChatTime(pastYear.getTime())).toBe('2020年5月12日 14:20');
  });

  it('空值返回空字符串', () => {
    expect(formatChatTime(null)).toBe('');
    expect(formatChatTime(undefined)).toBe('');
  });
});
