import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/svelte';
import CosmeticPreviewThumbnail from './CosmeticPreviewThumbnail.svelte';

describe('CosmeticPreviewThumbnail 列表缩略预览组件', () => {
  it('Steam 个人资料背景（profile_effect）渲染封面预览与 ProfileCover 容器', () => {
    const { container } = render(CosmeticPreviewThumbnail, {
      kind: 'profile_effect',
      id: 'steam_bg_4079902',
      name: 'A City to Burn',
      style: {
        mode: 'profile',
        image: 'ab78d1ab87ad1d93029960fea8ac848b31520de0.jpg'
      }
    });
    const thumbBg = container.querySelector('.cosmetic-thumb-bg');
    expect(thumbBg).not.toBeNull();
    const cover = container.querySelector('.profile-cover');
    expect(cover).not.toBeNull();
    const img = container.querySelector('.profile-cover-img');
    expect(img).not.toBeNull();
    expect(img?.getAttribute('src')).toContain('ab78d1ab87ad1d93029960fea8ac848b31520de0.jpg');
  });

  it('动态资料背景包含 webm 视频时渲染 video 元素', () => {
    const { container } = render(CosmeticPreviewThumbnail, {
      kind: 'profile_effect',
      id: 'steam_bg_4086375',
      name: 'Casting Animated Miniback',
      style: {
        mode: 'profile',
        image: '9a87bd267cfe136b9d348e464e782e8c99bf7831.jpg',
        webm: 'https://shared.fastly.steamstatic.com/community_assets/images/items/4602300/f885628dbc09b86737429170b738cc87f13d996d.webm'
      }
    });
    const thumbBg = container.querySelector('.cosmetic-thumb-bg');
    expect(thumbBg).not.toBeNull();
    const video = container.querySelector('video.profile-cover-video');
    expect(video).not.toBeNull();
    const source = container.querySelector('video source');
    expect(source?.getAttribute('src')).toContain('f885628dbc09b86737429170b738cc87f13d996d.webm');
  });

  it('Steam 动效头像框渲染 CosmeticAvatar', () => {
    const { container } = render(CosmeticPreviewThumbnail, {
      kind: 'avatar_frame',
      id: 'frame-lightning',
      name: 'Avatar frame: Lightning',
      style: {
        mode: 'steam_frame',
        image: 'lightning.png'
      }
    });
    const avatar = container.querySelector('.cosmetic-avatar');
    expect(avatar).not.toBeNull();
  });

  it('渐变彩色昵称渲染渐变条色块', () => {
    const { container } = render(CosmeticPreviewThumbnail, {
      kind: 'nickname_color',
      id: 'nick-grad',
      name: '霓虹幻彩',
      style: {
        mode: 'gradient',
        colors: ['#ff007a', '#7928ca']
      }
    });
    const span = container.querySelector('span[title="渐变色"]');
    expect(span).not.toBeNull();
    expect(span?.getAttribute('style')).toContain('linear-gradient');
  });

  it('纯色彩色昵称渲染纯色小圆点', () => {
    const { container } = render(CosmeticPreviewThumbnail, {
      kind: 'nickname_color',
      id: 'nick-solid',
      name: '纯金闪耀',
      style: {
        mode: 'solid',
        color: '#fbbf24'
      }
    });
    const span = container.querySelector('span[title="#fbbf24"]');
    expect(span).not.toBeNull();
    const styleAttr = span?.getAttribute('style') || '';
    expect(styleAttr.includes('#fbbf24') || styleAttr.includes('rgb(251, 191, 36)')).toBe(true);
  });
});
