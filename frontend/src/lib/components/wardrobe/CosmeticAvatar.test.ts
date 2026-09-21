import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/svelte';
import CosmeticAvatar from './CosmeticAvatar.svelte';

describe('CosmeticAvatar 头像框几何契约（跟随主题头像样式联动）', () => {
  it('未佩戴头像框时不注入 --avatar-radius，无 has-frame 类', () => {
    const { container } = render(CosmeticAvatar, { name: 'Alice' });
    const wrapper = container.querySelector('.cosmetic-avatar');
    expect(wrapper).not.toBeNull();
    expect(wrapper?.classList.contains('has-frame')).toBe(false);
    expect(wrapper?.getAttribute('style') || '').not.toContain('--avatar-radius');
    expect(container.querySelector('.cosmetic-avatar__fallback-frame')).toBeNull();
  });

  it('固定金色边框（gold_ring）带有 has-frame 类且不硬编码覆盖 --avatar-radius', () => {
    const { container } = render(CosmeticAvatar, {
      name: 'Bob',
      presentation: { avatar_frame: 'gold_ring' }
    });
    const wrapper = container.querySelector('.cosmetic-avatar');
    expect(wrapper?.classList.contains('has-frame')).toBe(true);
    expect(wrapper?.getAttribute('style') || '').not.toContain('--avatar-radius');
    const frame = container.querySelector('.cosmetic-avatar__fallback-frame.avatar-frame-gold');
    expect(frame).not.toBeNull();
  });

  it('彩色环边框（如 crimson）带有 has-frame 类且不硬编码覆盖 --avatar-radius', () => {
    const { container } = render(CosmeticAvatar, {
      name: 'Charlie',
      presentation: { avatar_frame: 'crimson' }
    });
    const wrapper = container.querySelector('.cosmetic-avatar');
    expect(wrapper?.classList.contains('has-frame')).toBe(true);
    expect(wrapper?.getAttribute('style') || '').not.toContain('--avatar-radius');
    const frame = container.querySelector('.cosmetic-avatar__fallback-frame.avatar-frame-ring');
    expect(frame).not.toBeNull();
  });

  it('自定义图片头像框（attachmentId）带有 has-frame 类且不硬编码覆盖 --avatar-radius', () => {
    const { container } = render(CosmeticAvatar, {
      name: 'David',
      presentation: { avatar_frame_attachment_id: 'att-frame-999' }
    });
    const wrapper = container.querySelector('.cosmetic-avatar');
    expect(wrapper?.classList.contains('has-frame')).toBe(true);
    expect(wrapper?.getAttribute('style') || '').not.toContain('--avatar-radius');
    const imgFrame = container.querySelector('img.cosmetic-avatar__frame');
    expect(imgFrame).not.toBeNull();
    expect(imgFrame?.getAttribute('src')).toContain('/attachments/att-frame-999');
  });

  it('自定义样式边框使用 var(--avatar-radius, 50%)，不写死固定圆角，跟随主题头像样式', () => {
    const { container } = render(CosmeticAvatar, {
      name: 'Eve',
      presentation: {
        avatar_frame: 'custom-1',
        avatar_frame_style: {
          mode: 'ring',
          color: '#f59e0b',
          widthPx: 4,
          glowPx: 14,
          animate: 'none'
        }
      }
    });
    const wrapper = container.querySelector('.cosmetic-avatar');
    expect(wrapper?.classList.contains('has-frame')).toBe(true);
    expect(wrapper?.getAttribute('style') || '').not.toContain('--avatar-radius');
    const frame = container.querySelector('.cosmetic-avatar__fallback-frame.avatar-frame-custom');
    expect(frame).not.toBeNull();
    expect(frame?.getAttribute('style')).toContain('border-radius: var(--avatar-radius, 50%)');
  });

  it('主题联动：当外部设置非圆角契约（--avatar-radius: 0）时，容器样式透传且头像与边框跟随非圆角', () => {
    const { container } = render(CosmeticAvatar, {
      name: 'Frank',
      presentation: { avatar_frame: 'gold_ring' },
      style: '--avatar-radius: 0;'
    });
    const wrapper = container.querySelector('.cosmetic-avatar');
    expect(wrapper?.classList.contains('has-frame')).toBe(true);
    expect(wrapper?.getAttribute('style')).toContain('--avatar-radius: 0;');
    const frame = container.querySelector('.cosmetic-avatar__fallback-frame.avatar-frame-gold');
    expect(frame).not.toBeNull();
  });

  it('主题联动：当外部设置圆角契约（--avatar-radius: 50%）时，容器样式透传且头像与边框跟随圆角', () => {
    const { container } = render(CosmeticAvatar, {
      name: 'Grace',
      presentation: { avatar_frame: 'crimson' },
      style: '--avatar-radius: 50%;'
    });
    const wrapper = container.querySelector('.cosmetic-avatar');
    expect(wrapper?.classList.contains('has-frame')).toBe(true);
    expect(wrapper?.getAttribute('style')).toContain('--avatar-radius: 50%;');
    const frame = container.querySelector('.cosmetic-avatar__fallback-frame.avatar-frame-ring');
    expect(frame).not.toBeNull();
  });

  it('保留调用方传递的自定义 style', () => {
    const { container } = render(CosmeticAvatar, {
      name: 'Heidi',
      presentation: { avatar_frame: 'gold_ring' },
      style: 'margin-right: 8px;'
    });
    const wrapper = container.querySelector('.cosmetic-avatar');
    expect(wrapper?.getAttribute('style')).toContain('margin-right: 8px');
    expect(wrapper?.getAttribute('style') || '').not.toContain('--avatar-radius: 50%');
  });
});
