// M03-UI-02：资料编辑 action 测试——If-Match 版本头转发、409 版本冲突、
// 字段错误透传、版本缺失拒绝、代理异常降级。
import { afterEach, describe, expect, it, vi } from 'vitest';
import { actions } from './+page.server';
import { authedPatch } from '$lib/api/server';
import type { SettingsFormResult } from './+page.server';

vi.mock('$lib/api/server', () => ({
  authedPatch: vi.fn(),
  getAuthed: vi.fn()
}));

const patchMock = authedPatch as unknown as ReturnType<typeof vi.fn>;

function actionEvent(
  entries: Record<string, string>,
  requestId: string | null = null
): Parameters<typeof actions.profile>[0] {
  const fd = new FormData();
  for (const [k, v] of Object.entries(entries)) fd.set(k, v);
  const headers = new Headers();
  if (requestId) headers.set('x-request-id', requestId);
  return {
    request: { formData: () => Promise.resolve(fd), headers },
    cookies: { get: vi.fn(() => null), set: vi.fn() },
    url: new URL('http://localhost/settings')
  } as unknown as Parameters<typeof actions.profile>[0];
}

const updatedUser = {
  id: 'u1',
  username: 'alice',
  email: 'alice@example.com',
  email_verified: true,
  status: 'active',
  display_name: '爱丽丝',
  bio: '新版简介',
  signature: null,
  timezone: 'UTC',
  theme_name: null,
  email_visible_to: 'nobody',
  profile_visible_to: 'everyone',
  level: 7,
  roles: [],
  mfa_enabled: false,
  version: 4
};

afterEach(() => {
  vi.clearAllMocks();
});

describe('M03-UI-02 资料编辑 profile action', () => {
  it('成功 → 转发 If-Match 版本头与字段体，返回更新后投影', async () => {
    patchMock.mockResolvedValueOnce({ ok: true, data: updatedUser });
    const result = (await actions.profile(
      actionEvent({ version: '3', display_name: ' 爱丽丝 ', bio: '新版简介', signature: '' }, 'req-1')
    )) as SettingsFormResult;
    expect(result.ok).toBe(true);
    expect(result.user).toEqual(updatedUser);
    const [cookies, path, body, headers, requestId] = patchMock.mock.calls[0];
    expect(path).toBe('/api/v1/me');
    expect(body).toEqual({ display_name: '爱丽丝', bio: '新版简介', signature: null });
    expect(headers).toEqual({ 'If-Match': '3' });
    expect(requestId).toBe('req-1');
    expect(cookies.get).toBeTypeOf('function');
  });

  it('超长字段被截断到上限后再提交', async () => {
    patchMock.mockResolvedValueOnce({ ok: true, data: updatedUser });
    await actions.profile(actionEvent({ version: '3', display_name: 'a'.repeat(50), bio: 'b'.repeat(3000), signature: 's'.repeat(500) }));
    const [, , body] = patchMock.mock.calls[0];
    expect(body.display_name).toBe('a'.repeat(32));
    expect(body.bio).toBe('b'.repeat(2000));
    expect(body.signature).toBe('s'.repeat(200));
  });

  it('版本缺失/无效 → 422，不调用代理', async () => {
    const result = (await actions.profile(actionEvent({ display_name: 'x' }))) as {
      status: number;
      data: SettingsFormResult;
    };
    expect(result.status).toBe(422);
    const result2 = (await actions.profile(actionEvent({ version: 'abc', display_name: 'x' }))) as {
      status: number;
      data: SettingsFormResult;
    };
    expect(result2.status).toBe(422);
    expect(patchMock).not.toHaveBeenCalled();
  });

  it('409 version_conflict → fail(409) 标记 conflict', async () => {
    patchMock.mockResolvedValueOnce({
      ok: false,
      status: 409,
      message: 'profile version conflict',
      requestId: 'rid-409',
      retryAfterSecs: null,
      code: 'version_conflict'
    });
    const result = (await actions.profile(actionEvent({ version: '3', display_name: 'x' }))) as {
      status: number;
      data: SettingsFormResult;
    };
    expect(result.status).toBe(409);
    expect(result.data.conflict).toBe(true);
    expect(result.data.message).toContain('其他窗口');
  });

  it('后端字段校验失败（400）→ fail(400) 透传错误文案', async () => {
    patchMock.mockResolvedValueOnce({
      ok: false,
      status: 400,
      message: 'bio 含不允许的字符',
      requestId: 'rid-400',
      retryAfterSecs: null,
      code: 'invalid_request'
    });
    const result = (await actions.profile(actionEvent({ version: '3', bio: '<script>' }))) as {
      status: number;
      data: SettingsFormResult;
    };
    expect(result.status).toBe(400);
    expect(result.data.message).toBe('bio 含不允许的字符');
    expect(result.data.conflict).toBeFalsy();
  });

  it('代理抛错 → fail(503)', async () => {
    patchMock.mockRejectedValueOnce(new Error('down'));
    const result = (await actions.profile(actionEvent({ version: '3', display_name: 'x' }))) as {
      status: number;
      data: SettingsFormResult;
    };
    expect(result.status).toBe(503);
  });
});
