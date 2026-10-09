import { describe, expect, it, vi, beforeEach } from 'vitest';
import { load } from './+page.server';
import * as serverApi from '$lib/api/server';

describe('GET /moderation/report load 函数', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('?post={id} 访问时直接重定向到帖子页并携带 ?report=1 触发弹窗', async () => {
    vi.spyOn(serverApi, 'getAuthed').mockResolvedValueOnce({
      ok: true,
      data: { id: 'u1', username: 'alice' }
    });

    const cookies = {} as any;
    const request = new Request('http://localhost/moderation/report?post=01a09869-954d-7a47-976b-53ff5b5edbf3');
    const url = new URL(request.url);

    await expect(
      load({
        cookies,
        request,
        url,
        params: {},
        route: { id: '/moderation/report' },
        parent: async () => ({}),
        depends: () => {},
        untrack: (fn: any) => fn(),
        setHeaders: () => {}
      } as any)
    ).rejects.toMatchObject({
      status: 303,
      location: '/posts/01a09869-954d-7a47-976b-53ff5b5edbf3?report=1'
    });
  });

  it('未登录访问 ?post={id} 时重定向到登录页且 next 为帖子带 ?report=1 链接', async () => {
    vi.spyOn(serverApi, 'getAuthed').mockResolvedValueOnce({
      ok: false,
      status: 401,
      message: 'unauthorized',
      requestId: null,
      retryAfterSecs: null,
      code: 'UNAUTHORIZED'
    });

    const cookies = {} as any;
    const request = new Request('http://localhost/moderation/report?post=post-abc');
    const url = new URL(request.url);

    await expect(
      load({
        cookies,
        request,
        url,
        params: {},
        route: { id: '/moderation/report' },
        parent: async () => ({}),
        depends: () => {},
        untrack: (fn: any) => fn(),
        setHeaders: () => {}
      } as any)
    ).rejects.toMatchObject({
      status: 303,
      location: expect.stringContaining('/login?next=')
    });
  });
});
