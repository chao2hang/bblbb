// M02-UX-02：CooldownButton 冷却倒计时——cooldown>0 禁用并每秒递减，
// 倒计时结束恢复；attempt 变化强制重启（同秒数重复计时）。
//
// 注意：Svelte 5 `$effect` 在微任务中运行，fake timers 下须用
// `advanceTimersByTimeAsync`（同步 advance 可能先于 effect 创建定时器）。
import { describe, expect, it, vi } from 'vitest';
import { render } from '@testing-library/svelte';
import CooldownButton from './CooldownButton.svelte';

describe('CooldownButton', () => {
  it('cooldown=0：可用且显示默认文案', () => {
    const { container } = render(CooldownButton, { text: '重新发送' });
    const button = container.querySelector('button') as HTMLButtonElement;
    expect(button).not.toBeDisabled();
    expect(button).toHaveTextContent('重新发送');
    expect(button).toHaveAttribute('type', 'submit');
  });

  it('cooldown>0：禁用并显示剩余秒数，倒计时结束恢复', async () => {
    vi.useFakeTimers();
    try {
      const { container } = render(CooldownButton, { cooldown: 2, text: '重新发送' });
      const button = container.querySelector('button') as HTMLButtonElement;
      expect(button).toBeDisabled();
      expect(button).toHaveTextContent('重新发送（2 秒）');

      await vi.advanceTimersByTimeAsync(1000);
      expect(button).toHaveTextContent('重新发送（1 秒）');

      await vi.advanceTimersByTimeAsync(1000);
      expect(button).toHaveTextContent('重新发送');
      expect(button).not.toBeDisabled();
    } finally {
      vi.useRealTimers();
    }
  });

  it('attempt 变化强制重启（同秒数重复计时）', async () => {
    vi.useFakeTimers();
    try {
      const { container, rerender } = render(CooldownButton, { cooldown: 60, attempt: 1 });
      const button = container.querySelector('button') as HTMLButtonElement;
      await vi.advanceTimersByTimeAsync(55_000);
      expect(button).toHaveTextContent('重新发送（5 秒）');

      // 同 cooldown、attempt+1 → 计时重新从 60 开始
      await rerender({ cooldown: 60, attempt: 2 });
      expect(button).toHaveTextContent('重新发送（60 秒）');
      await vi.advanceTimersByTimeAsync(1000);
      expect(button).toHaveTextContent('重新发送（59 秒）');
    } finally {
      vi.useRealTimers();
    }
  });
});
