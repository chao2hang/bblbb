// M02-UX-SEC：/me/security 页服务端 action 测试——设备管理（逐设备撤销/
// 退出全部）。自 /me/action.test.ts 平移（功能拆分至安全中心页）。
import { afterEach, describe, expect, it, vi } from 'vitest';
import { actions } from './+page.server';
import { authedDelete } from '$lib/api/server';
import type { SecurityActionData } from './+page.server';

vi.mock('$lib/api/server', () => ({
  authedDelete: vi.fn(),
  authedPost: vi.fn(),
  getAuthed: vi.fn()
}));

const authedDeleteMock = authedDelete as unknown as ReturnType<typeof vi.fn>;

function actionEvent(
  entries: Record<string, string> = {},
  requestId: string | null = null
): Parameters<typeof actions.revoke>[0] {
  const fd = new FormData();
  for (const [k, v] of Object.entries(entries)) fd.set(k, v);
  const headers = new Headers();
  if (requestId) headers.set('x-request-id', requestId);
  return {
    request: { formData: () => Promise.resolve(fd), headers },
    cookies: { get: vi.fn(() => null), set: vi.fn() },
    url: new URL('http://localhost/me/security')
  } as unknown as Parameters<typeof actions.revoke>[0];
}

/** SvelteKit redirect 以异常形式抛出；调用 action 时须捕获。 */
async function runAction(action: () => unknown): Promise<unknown> {
  try {
    return await action();
  } catch (e) {
    return e;
  }
}

function isRedirectResult(r: unknown): r is { status: number; location: string } {
  return typeof r === 'object' && r !== null && 'status' in r && 'location' in r;
}

afterEach(() => {
  vi.clearAllMocks();
});

describe('revoke action（逐设备撤销）', () => {
  it('session_id 缺失 → 422，不调用代理', async () => {
    const result = (await actions.revoke(actionEvent({}))) as {
      status: number;
      data: SecurityActionData;
    };
    expect(result.status).toBe(422);
    expect(authedDeleteMock).not.toHaveBeenCalled();
  });

  it('成功 → redirect 回 /me/security 刷新列表', async () => {
    authedDeleteMock.mockResolvedValueOnce({ ok: true });
    const result = await runAction(() => actions.revoke(actionEvent({ session_id: 'sess-2' }, 'req-r')));
    expect(isRedirectResult(result)).toBe(true);
    if (isRedirectResult(result)) {
      expect(result.status).toBe(303);
      expect(result.location).toBe('/me/security');
    }
    const [cookies, path, requestId] = authedDeleteMock.mock.calls[0];
    expect(path).toBe('/api/v1/auth/sessions/sess-2');
    expect(requestId).toBe('req-r');
    expect(cookies.get).toBeTypeOf('function');
  });

  it('他人设备（404）→ fail(404)', async () => {
    authedDeleteMock.mockResolvedValueOnce({
      ok: false,
      status: 404,
      message: '设备不存在或已撤销',
      requestId: 'rid-404',
      retryAfterSecs: null
    });
    const result = (await actions.revoke(
      actionEvent({ session_id: 'sess-x' })
    )) as { status: number; data: SecurityActionData };
    expect(result.status).toBe(404);
    expect(result.data.message).toContain('设备');
  });

  it('代理抛错 → fail(503)', async () => {
    authedDeleteMock.mockRejectedValueOnce(new Error('down'));
    const result = (await actions.revoke(
      actionEvent({ session_id: 'sess-2' })
    )) as { status: number; data: SecurityActionData };
    expect(result.status).toBe(503);
  });
});

describe('logoutall action（退出全部设备）', () => {
  it('成功 → redirect 到 /login（清 Cookie 已复制）', async () => {
    authedDeleteMock.mockResolvedValueOnce({ ok: true });
    const result = await runAction(() => actions.logoutall(actionEvent({}, 'req-lo')));
    expect(isRedirectResult(result)).toBe(true);
    if (isRedirectResult(result)) {
      expect(result.status).toBe(303);
      expect(result.location).toBe('/login');
    }
    const [cookies, path, requestId] = authedDeleteMock.mock.calls[0];
    expect(path).toBe('/api/v1/auth/sessions');
    expect(requestId).toBe('req-lo');
    expect(cookies.get).toBeTypeOf('function');
  });

  it('失败 → fail(status) 透传', async () => {
    authedDeleteMock.mockResolvedValueOnce({
      ok: false,
      status: 403,
      message: '安全校验失败',
      requestId: 'rid-403',
      retryAfterSecs: null
    });
    const result = (await actions.logoutall(
      actionEvent()
    )) as { status: number; data: SecurityActionData };
    expect(result.status).toBe(403);
  });

  it('代理抛错 → fail(503)', async () => {
    authedDeleteMock.mockRejectedValueOnce(new Error('down'));
    const result = (await actions.logoutall(
      actionEvent()
    )) as { status: number; data: SecurityActionData };
    expect(result.status).toBe(503);
  });
});
