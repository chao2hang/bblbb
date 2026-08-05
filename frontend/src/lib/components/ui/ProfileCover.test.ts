// M03-UI-05：ProfileCover 安全渲染测试——缺省渐变占位、加载失败安全降级、
// 装饰性 aria-hidden、不持久化签名 URL。
import { describe, expect, it } from 'vitest';
import { fireEvent, render } from '@testing-library/svelte';
import ProfileCover from './ProfileCover.svelte';

describe('M03-UI-05 ProfileCover', () => {
  it('无 src → 只输出渐变占位，不输出任何 URL', () => {
    const { container } = render(ProfileCover, { props: { src: null } });
    expect(container.querySelector('img')).toBeNull();
    expect(container.querySelector('.profile-cover')).not.toBeNull();
  });

  it('有 src → 渲染 img，加载失败后安全降级（移除 img，标记 has-error）', async () => {
    const { container } = render(ProfileCover, {
      props: { src: 'https://cdn.example.com/cover-abc123?v=1', label: '个人资料背景' }
    });
    const img = container.querySelector('img.profile-cover-img');
    expect(img).not.toBeNull();
    expect(img!.getAttribute('src')).toBe('https://cdn.example.com/cover-abc123?v=1');
    expect(img!.getAttribute('alt')).toBe('');
    expect(container.querySelector('.profile-cover')!.getAttribute('role')).toBe('img');

    // 模拟加载失败 → 降级为占位，不留破图、不残留 URL 属性。
    await fireEvent.error(img!);
    expect(container.querySelector('img')).toBeNull();
    expect(container.querySelector('.profile-cover')).toHaveClass('has-error');
  });

  it('无 label → 装饰性（aria-hidden），有 label → 可感知', () => {
    const { container, rerender } = render(ProfileCover, { props: { src: null } });
    expect(container.querySelector('.profile-cover')!.getAttribute('aria-hidden')).toBe('true');
    expect(container.querySelector('.profile-cover')!.getAttribute('role')).toBeNull();
    rerender({ src: null, label: '个人资料背景' });
    expect(container.querySelector('.profile-cover')!.getAttribute('aria-hidden')).toBeNull();
    expect(container.querySelector('.profile-cover')!.getAttribute('role')).toBe('img');
  });
});
