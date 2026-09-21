import { describe, expect, it } from 'vitest';
import { getCaretCoordinates } from './caret';

describe('getCaretCoordinates', () => {
  it('在 DOM 环境下能安全计算 textarea 指定光标坐标', () => {
    const textarea = document.createElement('textarea');
    textarea.value = 'hello @ch world';
    document.body.appendChild(textarea);

    const coords = getCaretCoordinates(textarea, 6);
    expect(coords).toHaveProperty('top');
    expect(coords).toHaveProperty('left');
    expect(coords).toHaveProperty('lineHeight');
    expect(typeof coords.top).toBe('number');
    expect(typeof coords.left).toBe('number');
    expect(coords.lineHeight).toBeGreaterThan(0);

    document.body.removeChild(textarea);
  });
});
