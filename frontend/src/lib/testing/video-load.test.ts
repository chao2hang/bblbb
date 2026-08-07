// M10-UI-06：/admin/video server load 投影白名单测试。
//
// - 401 → 跳登录；403 → forbidden；501 → not_implemented；
// - ok → pickVideoPolicies 只保留脱敏策略字段（Secret/内部字段丢弃）；
// - 对抗性 host/URL 形状被过滤，不进 SSR 数据。
import { afterEach, describe, expect, it, vi } from 'vitest';
import { getAuthed } from '$lib/api/server';
import { load as adminVideoLoad, type AdminVideoPageData } from '../../routes/admin/video/+page.server';

vi.mock('$lib/api/server', () => ({
  getAuthed: vi.fn()
}));

const getAuthedMock = getAuthed as unknown as ReturnType<typeof vi.fn>;

function event(overrides: Record<string, unknown> = {}) {
  const headers = new Headers();
  return {
    cookies: { get: vi.fn(() => null) },
    request: { headers },
    url: new URL('http://test.local/admin/video'),
    ...overrides
  } as unknown as Parameters<typeof adminVideoLoad>[0];
}

afterEach(() => vi.clearAllMocks());

describe('M10-UI-06 /admin/video load', () => {
  it('401 → redirect 登录', async () => {
    getAuthedMock.mockResolvedValueOnce({ ok: false, status: 401, message: 'x', requestId: null, retryAfterSecs: null, code: null });
    await expect(adminVideoLoad(event())).rejects.toMatchObject({ status: 303 });
  });

  it('403 → forbidden 态', async () => {
    getAuthedMock.mockResolvedValueOnce({ ok: false, status: 403, message: 'forbidden', requestId: 'r', retryAfterSecs: null, code: 'forbidden' });
    const data = (await adminVideoLoad(event())) as AdminVideoPageData;
    expect(data.state).toBe('forbidden');
    expect(data.policies).toBeNull();
  });

  it('501 → not_implemented 态（后端未就绪降级）', async () => {
    getAuthedMock.mockResolvedValueOnce({ ok: false, status: 501, message: 'x', requestId: null, retryAfterSecs: null, code: null });
    const data = (await adminVideoLoad(event())) as AdminVideoPageData;
    expect(data.state).toBe('not_implemented');
  });

  it('ok → 只保留脱敏策略字段（Secret/内部字段丢弃）', async () => {
    getAuthedMock.mockResolvedValueOnce({
      ok: true,
      data: {
        enabled: true,
        version: 2,
        items: [
          {
            provider: 'xigua',
            enabled: true,
            allowed_hosts: ['www.xigua.example'],
            embed_hosts: ['player.xigua.example'],
            allowed_media_types: ['video/mp4'],
            max_duration_seconds: 1800,
            policy_version: 3,
            updated_at: 1700000000000,
            provider_secret: 'VIDEO-LOAD-SECRET',
            embed_iframe_template: 'https://internal.example/embed/<%= id %>'
          }
        ]
      }
    });
    const data = (await adminVideoLoad(event())) as AdminVideoPageData;
    expect(data.state).toBe('ok');
    expect(data.policies?.items).toHaveLength(1);
    const policy = data.policies!.items[0];
    expect(policy.provider).toBe('xigua');
    expect(policy.policy_version).toBe(3);
    expect(policy).not.toHaveProperty('provider_secret');
    expect(policy).not.toHaveProperty('embed_iframe_template');
  });

  it('ok → 非法 host 形状（路径/斜杠/空格）过滤；非枚举 provider 丢弃', async () => {
    getAuthedMock.mockResolvedValueOnce({
      ok: true,
      data: {
        items: [
          {
            provider: 'direct',
            enabled: true,
            allowed_hosts: ['good.example.com', 'bad host/../x'],
            embed_hosts: ['../../etc/passwd'],
            allowed_media_types: ['video/mp4'],
            policy_version: 1
          },
          { provider: 'vimeo', enabled: true, policy_version: 1 }
        ]
      }
    });
    const data = (await adminVideoLoad(event())) as AdminVideoPageData;
    expect(data.policies?.items).toHaveLength(1);
    expect(data.policies!.items[0].allowed_hosts).toEqual(['good.example.com']);
    expect(data.policies!.items[0].embed_hosts).toEqual([]);
  });
});
