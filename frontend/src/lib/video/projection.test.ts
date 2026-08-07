// M10-UI：视频投影白名单 / 动态 CSP / 标签 单元测试。
//
// - pickVideoEmbedView：只保留白名单字段；blocked/removed 强制丢弃全部 URL
//   （M10-UI-04）；对抗性 Secret/内部字段不进投影；
// - pickVideoResolve：只保留 resolution_id 与允许字段（M10-UI-02）；
//   degraded_reason 只接受短稳定码；
// - pickVideoPolicies/pickVideoPolicy：Secret/内部字段丢弃，host 只接受
//   host 形状；
// - videoCspDirectives/canRenderInlinePlayer/safeExternalHref：精确指令、
//   无 JS 降级判定、外链安全（仅 https）。
import { describe, expect, it } from 'vitest';
import {
  pickVideoEmbedView,
  pickVideoPolicies,
  pickVideoPolicy,
  pickVideoResolve,
  safeHttpsUrl
} from './projection';
import {
  canRenderInlinePlayer,
  safeExternalHref,
  videoCspDirectives,
  videoCspHeader,
  hostOf
} from './csp';
import {
  formatVideoDuration,
  videoDegradedLabel,
  videoProviderLabel,
  videoStatusLabel,
  videoStatusTone
} from './labels';

function readyDirectView() {
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

describe('M10-UI-02 pickVideoResolve 投影白名单', () => {
  it('只保留允许字段；对抗性 Secret/签名 URL 丢弃', () => {
    const raw = {
      resolution_id: 'res-1',
      provider: 'xigua',
      title: '西瓜视频测试',
      poster_url: 'https://cdn.xigua.example/cover.webp',
      official_url: 'https://player.xigua.example/embed/123',
      source_url: 'https://www.xigua.example/video/123',
      policy_version: 2,
      embeddable: true,
      provider_status: { provider: 'xigua', enabled: true, available: true },
      // 对抗性内部字段（后端不应返回；前端必须丢弃）
      secret_key: 'XIGUA-SECRET-KEY',
      signed_play_url: 'https://internal.example/signed?sig=SECRET',
      access_token: 'TOKEN-123',
      cookie: 'session=abc'
    } as Record<string, unknown>;
    const result = pickVideoResolve(raw);
    expect(result).not.toBeNull();
    expect(result!.resolution_id).toBe('res-1');
    expect(result!.provider).toBe('xigua');
    expect(result!.embeddable).toBe(true);
    expect(result!.provider_status?.enabled).toBe(true);
    expect(result!.policy_version).toBe(2);
    expect(result).not.toHaveProperty('secret_key');
    expect(result).not.toHaveProperty('signed_play_url');
    expect(result).not.toHaveProperty('access_token');
    expect(result).not.toHaveProperty('cookie');
  });

  it('不可嵌入：embeddable=false + 短稳定码（超长/原文不回显）', () => {
    const result = pickVideoResolve({
      resolution_id: 'res-2',
      provider: 'xigua',
      embeddable: false,
      degraded_reason: 'no_embed_permission',
      raw_probe_error: '这是一段很长的原始探测错误不应该进入前端投影 because it is way too long'
    });
    expect(result).not.toBeNull();
    expect(result!.embeddable).toBe(false);
    expect(result!.degraded_reason).toBe('no_embed_permission');
    expect(result).not.toHaveProperty('raw_probe_error');
  });

  it('embeddable 缺省时按是否携带官方播放 URL 推断', () => {
    expect(pickVideoResolve({ resolution_id: 'a', provider: 'direct', media_url: 'https://m.example/a.mp4' })!.embeddable).toBe(true);
    expect(pickVideoResolve({ resolution_id: 'b', provider: 'xigua', source_url: 'https://x.example/v/1' })!.embeddable).toBe(false);
  });

  it('缺 resolution_id 返回 null；非 https URL 一律丢弃', () => {
    expect(pickVideoResolve({ provider: 'direct' })).toBeNull();
    const result = pickVideoResolve({
      resolution_id: 'c',
      provider: 'direct',
      media_url: 'http://insecure.example/a.mp4',
      poster_url: 'javascript:alert(1)'
    });
    expect(result!.media_url).toBeUndefined();
    expect(result!.poster_url).toBeUndefined();
  });
});

describe('M10-UI-04 pickVideoEmbedView 受限内容不渲染 URL', () => {
  it('ready 投影保留白名单字段', () => {
    const view = pickVideoEmbedView(readyDirectView());
    expect(view).not.toBeNull();
    expect(view!.media_url).toBe('https://media.example.com/video.mp4');
    expect(view!.duration_seconds).toBe(754);
  });

  it('blocked/removed 即使后端误返回 URL 也强制丢弃', () => {
    const blocked = pickVideoEmbedView({
      ...readyDirectView(),
      id: 'emb-b',
      status: 'blocked',
      media_url: 'https://media.example.com/video.mp4',
      official_url: 'https://player.example.com/embed/1',
      poster_url: 'https://cdn.example.com/poster.jpg'
    });
    expect(blocked!.status).toBe('blocked');
    expect(blocked!.media_url).toBeUndefined();
    expect(blocked!.official_url).toBeUndefined();
    expect(blocked!.poster_url).toBeUndefined();
    expect(blocked!.source_url).toBeUndefined();

    const removed = pickVideoEmbedView({ ...readyDirectView(), id: 'emb-r', status: 'removed' });
    expect(removed!.status).toBe('removed');
    expect(removed!.media_url).toBeUndefined();
  });

  it('对抗性字段（Secret/内部配置）不进投影', () => {
    const view = pickVideoEmbedView({
      ...readyDirectView(),
      id: 'emb-2',
      secret_key: 'VIDEO-SSR-SECRET',
      hls_key_uri: 'https://internal.example/key',
      provider_internal: { auth: 'INTERNAL' }
    });
    expect(view).not.toBeNull();
    expect(view).not.toHaveProperty('secret_key');
    expect(view).not.toHaveProperty('hls_key_uri');
    expect(view).not.toHaveProperty('provider_internal');
  });

  it('非法 provider/status 返回 null', () => {
    expect(pickVideoEmbedView({ ...readyDirectView(), provider: 'youtube' })).toBeNull();
    expect(pickVideoEmbedView({ ...readyDirectView(), status: 'archived' })).toBeNull();
  });
});

describe('M10-UI-06 pickVideoPolicies/pickVideoPolicy 管理端投影', () => {
  it('只保留策略字段；对抗性 Secret 丢弃；host 只接受 host 形状', () => {
    const picked = pickVideoPolicies({
      enabled: true,
      version: 5,
      items: [
        {
          provider: 'xigua',
          enabled: true,
          allowed_hosts: ['www.xigua.example', 'player.xigua.example'],
          embed_hosts: ['player.xigua.example'],
          allowed_media_types: ['video/mp4'],
          max_duration_seconds: 1800,
          hls_max_segments: 200,
          policy_version: 4,
          updated_at: 1700000000000,
          provider_secret: 'ADMIN-VIDEO-SECRET',
          'signed_url_template': 'https://internal.example/s?={SECRET}'
        }
      ]
    });
    expect(picked.enabled).toBe(true);
    expect(picked.items).toHaveLength(1);
    const policy = picked.items[0];
    expect(policy.allowed_hosts).toEqual(['www.xigua.example', 'player.xigua.example']);
    expect(policy.policy_version).toBe(4);
    expect(policy).not.toHaveProperty('provider_secret');
    expect(policy).not.toHaveProperty('signed_url_template');
  });

  it('非法 host 形状（路径/斜杠/空格）被过滤', () => {
    const policy = pickVideoPolicy({
      provider: 'direct',
      enabled: true,
      allowed_hosts: ['example.com', 'bad host/../', 'evil.example.com/x'],
      allowed_media_types: ['video/mp4; boundary=EVIL'],
      policy_version: 1
    });
    expect(policy!.allowed_hosts).toEqual(['example.com']);
    expect(policy!.allowed_media_types).toEqual([]);
  });

  it('非枚举 provider 返回 null', () => {
    expect(pickVideoPolicy({ provider: 'vimeo', enabled: true, policy_version: 1 })).toBeNull();
  });
});

describe('M10-UI-03 动态 CSP 指令', () => {
  it('xigua ready → frame-src 只放行官方嵌入 host', () => {
    const directives = videoCspDirectives({
      provider: 'xigua',
      status: 'ready',
      official_url: 'https://player.xigua.example/embed/123',
      poster_url: 'https://cdn.xigua.example/cover.webp',
      media_url: null
    });
    expect(directives['frame-src']).toEqual(['player.xigua.example']);
    expect(directives['media-src']).toEqual([]);
    expect(directives['img-src']).toEqual(['cdn.xigua.example']);
    expect(videoCspHeader(directives)).toBe('frame-src player.xigua.example; img-src cdn.xigua.example');
  });

  it('direct/hls ready → media-src + connect-src 只放行媒体 host', () => {
    const directives = videoCspDirectives({
      provider: 'hls',
      status: 'ready',
      media_url: 'https://media.example.com/stream.m3u8',
      poster_url: null,
      official_url: null
    });
    expect(directives['media-src']).toEqual(['media.example.com']);
    expect(directives['connect-src']).toEqual(['media.example.com']);
    expect(directives['frame-src']).toEqual([]);
  });

  it('非 ready（pending/blocked/error/removed）不生成任何放行来源', () => {
    for (const status of ['pending', 'blocked', 'error', 'removed'] as const) {
      const directives = videoCspDirectives({
        provider: 'xigua',
        status,
        official_url: 'https://player.xigua.example/embed/1',
        media_url: 'https://media.example.com/a.mp4',
        poster_url: 'https://cdn.example.com/p.jpg'
      });
      expect(directives['frame-src']).toEqual([]);
      expect(directives['media-src']).toEqual([]);
      expect(directives['img-src']).toEqual([]);
      expect(videoCspHeader(directives)).toBe('');
    }
  });

  it('非 https / userinfo / 解析失败 URL 不进指令', () => {
    expect(hostOf('http://x.example/a')).toBeNull();
    expect(hostOf('https://user:pass@x.example/a')).toBeNull();
    expect(hostOf('not a url')).toBeNull();
    expect(safeHttpsUrl('https://x.example/a')).toBe('https://x.example/a');
    expect(safeHttpsUrl('javascript:alert(1)')).toBeNull();
  });
});

describe('M10-UI-05 内联播放与外链降级判定', () => {
  it('ready + 官方 https URL → 可内联播放；pending/blocked/非 https → 否', () => {
    const view = {
      provider: 'direct' as const,
      status: 'ready' as const,
      media_url: 'https://media.example.com/v.mp4',
      official_url: null
    };
    expect(canRenderInlinePlayer(view)).toBe(true);
    expect(canRenderInlinePlayer({ ...view, status: 'pending' as const })).toBe(false);
    expect(canRenderInlinePlayer({ ...view, status: 'blocked' as const })).toBe(false);
    // 来源 host 白名单由后端最终裁决；前端只保证 https/无 userinfo。
    expect(canRenderInlinePlayer({ ...view, media_url: 'http://insecure.example.com/v.mp4' })).toBe(false);
    expect(canRenderInlinePlayer({ ...view, media_url: 'https://user:pass@x.example/v.mp4' })).toBe(false);
    expect(canRenderInlinePlayer({ ...view, media_url: null })).toBe(false);
  });

  it('xigua 只认官方 iframe URL（https），缺失/非 https → 不可内联', () => {
    const view = {
      provider: 'xigua' as const,
      status: 'ready' as const,
      media_url: null,
      official_url: 'https://player.xigua.example/embed/1'
    };
    expect(canRenderInlinePlayer(view)).toBe(true);
    expect(canRenderInlinePlayer({ ...view, official_url: 'https://attacker.example/frame' })).toBe(true); // 后端只返回白名单内 host
    expect(canRenderInlinePlayer({ ...view, official_url: 'javascript:alert(1)' })).toBe(false);
    expect(canRenderInlinePlayer({ ...view, official_url: null })).toBe(false);
  });

  it('safeExternalHref：media→source→official 依次取首个；blocked/removed 恒 null', () => {
    const base = {
      status: 'ready' as const,
      media_url: 'https://media.example.com/v.mp4',
      official_url: 'https://player.xigua.example/embed/1',
      source_url: 'https://src.example.com/v'
    };
    expect(safeExternalHref(base)).toBe('https://media.example.com/v.mp4');
    expect(safeExternalHref({ ...base, media_url: null })).toBe('https://src.example.com/v');
    expect(safeExternalHref({ ...base, media_url: null, source_url: null })).toBe('https://player.xigua.example/embed/1');
    expect(safeExternalHref({ ...base, media_url: 'http://insecure.example/a' })).toBe('https://src.example.com/v');
    expect(safeExternalHref({ ...base, status: 'blocked' })).toBeNull();
    expect(safeExternalHref({ ...base, status: 'removed' })).toBeNull();
  });
});

describe('M10-UI-05 标签与时长格式化', () => {
  it('状态/Provider 中文标签', () => {
    expect(videoStatusLabel('ready')).toBe('可嵌入');
    expect(videoStatusLabel('blocked')).toBe('已下架');
    expect(videoStatusLabel('pending')).toBe('解析中');
    expect(videoProviderLabel('xigua')).toBe('西瓜视频');
    expect(videoProviderLabel(null)).toBe('');
    expect(videoStatusTone('ready')).toBe('badge-success');
    expect(videoStatusTone('blocked')).toBe('badge-warning');
  });

  it('降级稳定码 → 中文说明（未命中原样短展示）', () => {
    expect(videoDegradedLabel('no_embed_permission')).toBe('来源未授权嵌入，仅可外链打开');
    expect(videoDegradedLabel('rate_limited')).toBe('来源限流，仅可外链打开');
    expect(videoDegradedLabel(null)).toBe('当前只能以外链方式引用');
    expect(videoDegradedLabel('some_unknown_code')).toBe('some_unknown_code');
  });

  it('时长格式化（mm:ss / h:mm:ss；无效返回空）', () => {
    expect(formatVideoDuration(754)).toBe('12:34');
    expect(formatVideoDuration(3725)).toBe('1:02:05');
    expect(formatVideoDuration(0)).toBe('00:00');
    expect(formatVideoDuration(null)).toBe('');
    expect(formatVideoDuration(-1)).toBe('');
  });
});
