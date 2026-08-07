// M10-UI-03/04/05：VideoEmbedView 安全投影组件测试。
//
// - ready + direct/hls → 内联 <video>（controls/playsinline/无 autoplay，
//   来源走 <source>，字幕轨只在后端提供时渲染）；meta 行保留安全外链；
// - xigua → 官方 iframe（sandbox/allow 最小权限、referrerpolicy=no-referrer、
//   title）；allow 不含 camera/microphone/autoplay；
// - blocked/removed → 不渲染视频 URL 与播放器配置（M10-UI-04）；
// - 减少动效：任何情况下无 autoplay、无动画类（M10-UI-05）；
// - 键盘：外链可聚焦、Tab 可达；video/iframe 有可访问名称（M10-UI-05）；
// - 移动端比例：播放容器 aspect-ratio 16:9 + max-width 100%（M10-UI-05）。
import { describe, expect, it } from 'vitest';
import { render, waitFor } from '@testing-library/svelte';
import { setPrefersReducedMotion } from '$lib/testing/a11y';
import VideoEmbedView from './VideoEmbedView.svelte';
import type { VideoEmbedView as VideoEmbedViewType } from '$lib/api/types';

function directView(): VideoEmbedViewType {
  return {
    id: 'emb-1',
    provider: 'direct',
    status: 'ready',
    media_url: 'https://media.example.com/video.mp4',
    source_url: 'https://media.example.com/video.mp4',
    poster_url: 'https://cdn.example.com/poster.jpg',
    title: '示例视频',
    media_type: 'video/mp4',
    duration_seconds: 754,
    policy_version: 3,
    version: 2,
    created_at: 1700000000000,
    updated_at: 1700000000000
  };
}

function xiguaView(): VideoEmbedViewType {
  return {
    id: 'emb-2',
    provider: 'xigua',
    status: 'ready',
    official_url: 'https://player.xigua.example/embed/123',
    source_url: 'https://www.xigua.example/video/123',
    poster_url: 'https://cdn.xigua.example/cover.webp',
    title: '西瓜视频测试',
    policy_version: 2,
    version: 1,
    created_at: 1700000000000,
    updated_at: 1700000000000
  };
}

describe('M10-UI-03 direct/hls 安全视频投影', () => {
  it('ready 直链 → 内联 <video>：controls/playsinline、无 autoplay、来源走 <source>', async () => {
    const { container } = render(VideoEmbedView, { props: { view: directView() } });
    await waitFor(() => expect(container.querySelector('video')).not.toBeNull());
    const video = container.querySelector('video')!;
    expect(video).toHaveAttribute('controls');
    expect(video).toHaveAttribute('playsinline');
    expect(video).not.toHaveAttribute('autoplay');
    expect(video).toHaveAttribute('preload', 'metadata');
    expect(video).toHaveAttribute('poster', 'https://cdn.example.com/poster.jpg');
    const source = video.querySelector('source');
    expect(source?.getAttribute('src')).toBe('https://media.example.com/video.mp4');
    expect(source?.getAttribute('type')).toBe('video/mp4');
    // meta 行保留来源/外链
    const external = Array.from(container.querySelectorAll('a')).find((a) => a.textContent?.includes('来源 / 外链'));
    expect(external?.getAttribute('href')).toBe('https://media.example.com/video.mp4');
  });

  it('字幕轨只在后端提供 caption_url 时渲染', async () => {
    const { container } = render(VideoEmbedView, {
      props: { view: { ...directView(), caption_url: 'https://cdn.example.com/captions.vtt' } }
    });
    await waitFor(() => expect(container.querySelector('video')).not.toBeNull());
    const track = container.querySelector('track');
    expect(track?.getAttribute('kind')).toBe('captions');
    expect(track?.getAttribute('src')).toBe('https://cdn.example.com/captions.vtt');
  });

  it('移动端比例：容器 aspect-ratio 16:9 + max-width 100%', async () => {
    const { container } = render(VideoEmbedView, { props: { view: directView() } });
    await waitFor(() => expect(container.querySelector('video')).not.toBeNull());
    const frame = container.querySelector('.video-embed-frame')!;
    expect(frame.getAttribute('style')).toContain('aspect-ratio:16/9');
    expect(frame.getAttribute('style')).toContain('max-width:100%');
  });

  it('减少动效：无 autoplay、无动画类', async () => {
    setPrefersReducedMotion(true);
    const { container } = render(VideoEmbedView, { props: { view: directView() } });
    await waitFor(() => expect(container.querySelector('video')).not.toBeNull());
    const video = container.querySelector('video')!;
    expect(video).not.toHaveAttribute('autoplay');
    expect(container.querySelector('.video-embed')?.className ?? '').not.toContain('animate');
  });

  it('键盘：外链可聚焦、视频有可访问名称', async () => {
    const { container } = render(VideoEmbedView, { props: { view: directView() } });
    await waitFor(() => expect(container.querySelector('video')).not.toBeNull());
    const video = container.querySelector('video')!;
    expect(video).toHaveAttribute('aria-label', '示例视频');
    const external = Array.from(container.querySelectorAll('a')).find((a) => a.textContent?.includes('来源 / 外链'))!;
    external.focus();
    expect(document.activeElement).toBe(external);
    expect(external).toHaveAttribute('rel', 'noopener noreferrer nofollow');
  });
});

describe('M10-UI-03 xigua 官方 iframe 投影', () => {
  it('iframe 带 sandbox/allow 最小权限与 referrerpolicy=no-referrer', async () => {
    const { container } = render(VideoEmbedView, { props: { view: xiguaView() } });
    await waitFor(() => expect(container.querySelector('iframe')).not.toBeNull());
    const iframe = container.querySelector('iframe')!;
    expect(iframe.getAttribute('src')).toBe('https://player.xigua.example/embed/123');
    expect(iframe).toHaveAttribute('title', '西瓜视频测试');
    expect(iframe).toHaveAttribute('referrerpolicy', 'no-referrer');
    expect(iframe).toHaveAttribute('sandbox', 'allow-scripts allow-same-origin allow-presentation');
    const allow = iframe.getAttribute('allow') ?? '';
    expect(allow).toContain('encrypted-media');
    expect(allow).not.toMatch(/camera|microphone|autoplay/);
    expect(iframe).toHaveAttribute('loading', 'lazy');
  });
});

describe('M10-UI-04 受限/已删除内容不渲染 URL 或播放器', () => {
  it('blocked → 无播放器、无媒体链接、显示下架说明', async () => {
    const view = {
      ...directView(),
      id: 'emb-b',
      status: 'blocked' as const,
      media_url: 'https://media.example.com/video.mp4'
    };
    const { container } = render(VideoEmbedView, { props: { view } });
    expect(container.querySelector('video')).toBeNull();
    expect(container.querySelector('iframe')).toBeNull();
    // blocked 时任何 a 都不能指向媒体/官方 URL
    for (const a of container.querySelectorAll('a')) {
      expect(a.getAttribute('href')).toBeNull();
    }
    expect(container.textContent).toContain('已下架');
  });

  it('removed → 无播放器、无媒体链接', () => {
    const { container } = render(VideoEmbedView, {
      props: { view: { ...directView(), id: 'emb-r', status: 'removed' as const } }
    });
    expect(container.querySelector('video')).toBeNull();
    for (const a of container.querySelectorAll('a')) {
      expect(a.getAttribute('href')).toBeNull();
    }
    expect(container.textContent).toContain('已移除');
  });

  it('pending → 解析中说明，无播放器', () => {
    const { container } = render(VideoEmbedView, {
      props: { view: { ...directView(), id: 'emb-p', status: 'pending' as const } }
    });
    expect(container.querySelector('video')).toBeNull();
    expect(container.textContent).toContain('解析中');
  });

  it('对抗性内部字段（Secret/签名 URL）绝不进入渲染', async () => {
    const adversarial = {
      ...directView(),
      secret_key: 'VIDEO-COMPONENT-SECRET',
      signed_url: 'https://internal.example/signed?sig=XYZ',
      hls_key_uri: 'https://internal.example/key'
    } as unknown as VideoEmbedViewType;
    const { container } = render(VideoEmbedView, { props: { view: adversarial } });
    await waitFor(() => expect(container.querySelector('video')).not.toBeNull());
    expect(container.textContent).not.toContain('VIDEO-COMPONENT-SECRET');
    expect(container.textContent).not.toContain('internal.example');
  });
});
