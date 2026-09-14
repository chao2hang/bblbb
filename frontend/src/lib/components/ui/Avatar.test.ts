import { describe, expect, it } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import Avatar from './Avatar.svelte';

describe('Avatar 组件', () => {
  it('默认渲染首字母与渐变背景', () => {
    const { container } = render(Avatar, { name: 'Alice', size: 'md' });
    const avatar = container.querySelector('.avatar');
    expect(avatar).not.toBeNull();
    expect(avatar?.textContent?.trim()).toBe('A');
    expect(avatar?.getAttribute('role')).toBe('img');
    expect(avatar?.getAttribute('aria-label')).toBe('Alice');
    expect(avatar?.getAttribute('style')).toContain('linear-gradient');
    expect(avatar?.getAttribute('style')).toContain('--avatar-bg');
    expect(container.querySelector('img')).toBeNull();
  });

  it('按用户固定背景颜色（多次渲染保持绝对一致）', () => {
    const { container: c1 } = render(Avatar, { name: 'Chaos' });
    const { container: c2 } = render(Avatar, { name: 'Chaos' });
    const bg1 = c1.querySelector('.avatar')?.getAttribute('style');
    const bg2 = c2.querySelector('.avatar')?.getAttribute('style');
    expect(bg1).toBe(bg2);
  });

  it('忽略大小写保持颜色固定', () => {
    const { container: c1 } = render(Avatar, { name: 'Alice' });
    const { container: c2 } = render(Avatar, { name: 'alice' });
    const getGradient = (el: Element | null) => {
      const style = el?.getAttribute('style') || '';
      const m = style.match(/linear-gradient([^)]+)/);
      return m ? m[0] : '';
    };
    expect(getGradient(c1.querySelector('.avatar'))).toBe(getGradient(c2.querySelector('.avatar')));
  });

  it('seed 优于 name 决定背景颜色（用户改昵称背景色不变）', () => {
    const { container: c1 } = render(Avatar, { name: '旧昵称', seed: 'u_1001' });
    const { container: c2 } = render(Avatar, { name: '新昵称', seed: 'u_1001' });
    const getGradient = (el: Element | null) => {
      const style = el?.getAttribute('style') || '';
      const m = style.match(/linear-gradient([^)]+)/);
      return m ? m[0] : '';
    };
    expect(getGradient(c1.querySelector('.avatar'))).toBe(getGradient(c2.querySelector('.avatar')));
    expect(c1.querySelector('.avatar')?.textContent?.trim()).toBe('旧');
    expect(c2.querySelector('.avatar')?.textContent?.trim()).toBe('新');
  });

  it('传入 username 保证跨视图/跨组件背景颜色统一', () => {
    // 模拟顶栏与帖子列表：顶栏有展示名，列表只有用户名，但均关联同一用户
    const { container: c1 } = render(Avatar, { name: '站长', username: 'chaos' });
    const { container: c2 } = render(Avatar, { name: 'Chaos', seed: 'chaos' });
    const getGradient = (el: Element | null) => {
      const style = el?.getAttribute('style') || '';
      const m = style.match(/linear-gradient([^)]+)/);
      return m ? m[0] : '';
    };
    expect(getGradient(c1.querySelector('.avatar'))).toBe(getGradient(c2.querySelector('.avatar')));
  });

  it('去除前导 @ 并转大写英文字母', () => {
    const { container } = render(Avatar, { name: '@nina' });
    expect(container.querySelector('.avatar')?.textContent?.trim()).toBe('N');
  });

  it('中文用户名取首个汉字', () => {
    const { container } = render(Avatar, { name: '李白' });
    expect(container.querySelector('.avatar')?.textContent?.trim()).toBe('李');
  });

  it('传入 attachmentId 时渲染稳定内容端点图片', () => {
    const { container } = render(Avatar, {
      name: 'Bob',
      size: 'lg',
      attachmentId: '018f0000-0000-7000-8000-000000000001'
    });
    const img = container.querySelector('img');
    expect(img).not.toBeNull();
    expect(img?.getAttribute('src')).toBe(
      '/api/v1/attachments/018f0000-0000-7000-8000-000000000001/content'
    );
  });

  it('传入 direct src 时优先使用 src', () => {
    const { container } = render(Avatar, {
      name: 'Charlie',
      src: '/custom-avatar.png'
    });
    const img = container.querySelector('img');
    expect(img).not.toBeNull();
    expect(img?.getAttribute('src')).toBe('/custom-avatar.png');
  });

  it('图片加载失败时安全降级为首字母文本与渐变背景', async () => {
    const { container } = render(Avatar, {
      name: 'David',
      src: '/broken.png'
    });
    const img = container.querySelector('img')!;
    expect(img).not.toBeNull();

    // 触发 onerror
    await fireEvent.error(img);

    expect(container.querySelector('img')).toBeNull();
    const avatar = container.querySelector('.avatar');
    expect(avatar?.textContent?.trim()).toBe('D');
    expect(avatar?.getAttribute('style')).toContain('linear-gradient');
  });
});
