import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/svelte';
import CosmeticName from './CosmeticName.svelte';

describe('CosmeticName 动画与样式渲染', () => {
  it('渐变流动模式（gradient + flow）：注入流动动画与循环渐变色标', () => {
    const { container } = render(CosmeticName, {
      name: '测试用户',
      presentation: {
        nickname_color: 'preview',
        nickname_color_style: {
          mode: 'gradient',
          colors: ['#f43f5e', '#f59e0b', '#8b5cf6'],
          animate: 'flow',
          durationMs: 1500
        }
      }
    });
    const span = container.querySelector('.cosmetic-name');
    expect(span).not.toBeNull();
    const style = span?.getAttribute('style') ?? '';
    expect(style).toContain('animation: cosmetic-name-shift 1500ms linear infinite');
    expect(style).toContain('linear-gradient(90deg,#f43f5e,#f59e0b,#8b5cf6,#f43f5e)');
    expect(style).toContain('background-clip: text');
  });

  it('波浪起伏模式（gradient + wave）：注入 wave 动画', () => {
    const { container } = render(CosmeticName, {
      name: '测试用户',
      presentation: {
        nickname_color: 'preview',
        nickname_color_style: {
          mode: 'gradient',
          colors: ['#f43f5e', '#8b5cf6'],
          animate: 'wave',
          durationMs: 3000
        }
      }
    });
    const span = container.querySelector('.cosmetic-name');
    const style = span?.getAttribute('style') ?? '';
    expect(style).toContain('animation: cosmetic-name-wave 3000ms ease-in-out infinite');
  });

  it('发光模式（glow + breathe）：注入 breathe 动画', () => {
    const { container } = render(CosmeticName, {
      name: '测试用户',
      presentation: {
        nickname_color: 'preview',
        nickname_color_style: {
          mode: 'glow',
          color: '#10b981',
          animate: 'breathe',
          durationMs: 2500
        }
      }
    });
    const span = container.querySelector('.cosmetic-name');
    const style = span?.getAttribute('style') ?? '';
    expect(style).toContain('animation: cosmetic-name-breathe 2500ms ease-in-out infinite');
  });

  it('纯色微动模式（solid + bounce）：注入 bounce 动画', () => {
    const { container } = render(CosmeticName, {
      name: '测试用户',
      presentation: {
        nickname_color: 'preview',
        nickname_color_style: {
          mode: 'solid',
          color: '#3b82f6',
          animate: 'bounce',
          durationMs: 2000
        }
      }
    });
    const span = container.querySelector('.cosmetic-name');
    const style = span?.getAttribute('style') ?? '';
    expect(style).toContain('animation: cosmetic-name-bounce 2000ms ease-in-out infinite');
  });

  it('纯色微动模式（solid + shimmer）：注入 shimmer 渐变与动画', () => {
    const { container } = render(CosmeticName, {
      name: '测试用户',
      presentation: {
        nickname_color: 'preview',
        nickname_color_style: {
          mode: 'solid',
          color: '#3b82f6',
          animate: 'shimmer',
          durationMs: 2000
        }
      }
    });
    const span = container.querySelector('.cosmetic-name');
    const style = span?.getAttribute('style') ?? '';
    expect(style).toContain('animation: cosmetic-name-shimmer 2000ms ease-in-out infinite');
    expect(style).toContain('linear-gradient(90deg,#3b82f6,#ffffff,#3b82f6)');
  });
});
