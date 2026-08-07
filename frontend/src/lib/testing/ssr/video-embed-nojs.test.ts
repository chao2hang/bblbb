// M10-UI-03/04/05：VideoEmbedView 无 JS（SSR）快照测试。
//
// - 无 JS 时只渲染安全外链卡片：SSR HTML 含外链 <a href>（rel 防反向），
//   绝不含 <video>/<iframe>（播放器只在内联 JS 挂载后渲染）；
// - blocked/removed：不渲染任何视频 URL 或播放器配置（M10-UI-04）；
// - 对抗性 Provider/内部字段（Secret/签名 URL）不进入 SSR HTML。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import VideoEmbedView from '../../components/video/VideoEmbedView.svelte';
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

describe('M10-UI-05 无 JS 降级：只渲染安全外链', () => {
  it('ready 直链 → SSR 只含外链卡片（无 <video>/<iframe>）', () => {
    const { body } = render(VideoEmbedView, { props: { view: directView() } });
    expect(body).toContain('示例视频');
    expect(body).toMatch(/<a[^>]*href="https:\/\/media\.example\.com\/video\.mp4"[^>]*>/);
    expect(body).toMatch(/rel="noopener noreferrer nofollow"/);
    expect(body).not.toContain('<video');
    expect(body).not.toContain('<iframe');
    expect(body).toContain('来源 / 外链');
    // 播放器不因 SSR 而自动播放：任何 video/iframe 标记都不存在。
    expect(body).not.toContain('autoplay');
  });

  it('ready xigua → SSR 外链优先规范化页面（source_url）而非裸播放器', () => {
    const { body } = render(VideoEmbedView, {
      props: {
        view: {
          id: 'emb-2',
          provider: 'xigua',
          status: 'ready',
          official_url: 'https://player.xigua.example/embed/123',
          source_url: 'https://www.xigua.example/video/123',
          title: '西瓜视频测试',
          policy_version: 2,
          version: 1,
          created_at: 1700000000000,
          updated_at: 1700000000000
        }
      }
    });
    expect(body).toMatch(/<a[^>]*href="https:\/\/www\.xigua\.example\/video\/123"[^>]*>/);
    expect(body).not.toContain('<video');
    expect(body).not.toContain('<iframe');
  });

  it('无播放 URL 的 ready → 只渲染占位与元信息，不出现任何 href 外链以外的 URL', () => {
    const { body } = render(VideoEmbedView, {
      props: {
        view: {
          id: 'emb-3',
          provider: 'hls',
          status: 'ready',
          title: '无 URL 示例',
          policy_version: 1,
          version: 1,
          created_at: 1700000000000,
          updated_at: 1700000000000
        }
      }
    });
    expect(body).toContain('无 URL 示例');
    expect(body).not.toContain('http');
  });
});

describe('M10-UI-04 受限/删除内容无 JS 也不渲染 URL', () => {
  it('blocked → SSR 无任何链接、无播放器，显示下架说明', () => {
    const view = {
      ...directView(),
      id: 'emb-b',
      status: 'blocked' as const,
      media_url: 'https://media.example.com/video.mp4',
      source_url: 'https://media.example.com/video.mp4'
    };
    const { body } = render(VideoEmbedView, { props: { view } });
    expect(body).not.toContain('<video');
    expect(body).not.toContain('<iframe');
    expect(body).not.toContain('https://media.example.com/video.mp4');
    expect(body).not.toMatch(/<a[^>]*href=/);
    expect(body).toContain('已下架');
    expect(body).toContain('不再提供播放');
  });

  it('removed → SSR 无任何链接、无播放器', () => {
    const { body } = render(VideoEmbedView, {
      props: { view: { ...directView(), id: 'emb-r', status: 'removed' as const } }
    });
    expect(body).not.toContain('<video');
    expect(body).not.toMatch(/<a[^>]*href=/);
    expect(body).toContain('已移除');
  });

  it('对抗性字段（Secret/签名 URL/内部配置）不进 SSR HTML', () => {
    const adversarial = {
      ...directView(),
      secret_key: 'VIDEO-SSR-SECRET-KEY',
      signed_play_url: 'https://internal.example/signed?sig=SSR-SECRET',
      provider_internal: { auth_header: 'Bearer SSR-TOKEN' }
    } as unknown as VideoEmbedViewType;
    const { body } = render(VideoEmbedView, { props: { view: adversarial } });
    expect(body).not.toContain('VIDEO-SSR-SECRET-KEY');
    expect(body).not.toContain('internal.example');
    expect(body).not.toContain('SSR-TOKEN');
  });
});
