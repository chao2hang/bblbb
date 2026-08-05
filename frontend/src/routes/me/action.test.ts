// M02-UX-05/06：/me 页服务端 action 测试——设备管理（逐设备撤销/退出
// 全部）与 MFA 管理（enroll/confirm/cancel/recovery/disable/re-auth）。
import { afterEach, describe, expect, it, vi } from 'vitest';
import { actions } from './+page.server';
import { authedDelete, authedPost } from '$lib/api/server';
import type { MeActionData } from './+page.server';

vi.mock('$lib/api/server', () => ({
  authedDelete: vi.fn(),
  authedPost: vi.fn(),
  getAuthed: vi.fn()
}));

const authedDeleteMock = authedDelete as unknown as ReturnType<typeof vi.fn>;
const authedPostMock = authedPost as unknown as ReturnType<typeof vi.fn>;

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
    url: new URL('http://localhost/me')
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
      data: MeActionData;
    };
    expect(result.status).toBe(422);
    expect(authedDeleteMock).not.toHaveBeenCalled();
  });

  it('成功 → redirect 回 /me 刷新列表', async () => {
    authedDeleteMock.mockResolvedValueOnce({ ok: true });
    const result = await runAction(() => actions.revoke(actionEvent({ session_id: 'sess-2' }, 'req-r')));
    expect(isRedirectResult(result)).toBe(true);
    if (isRedirectResult(result)) {
      expect(result.status).toBe(303);
      expect(result.location).toBe('/me');
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
    )) as { status: number; data: MeActionData };
    expect(result.status).toBe(404);
    expect(result.data.message).toContain('设备');
  });

  it('代理抛错 → fail(503)', async () => {
    authedDeleteMock.mockRejectedValueOnce(new Error('down'));
    const result = (await actions.revoke(
      actionEvent({ session_id: 'sess-2' })
    )) as { status: number; data: MeActionData };
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
    )) as { status: number; data: MeActionData };
    expect(result.status).toBe(403);
  });

  it('代理抛错 → fail(503)', async () => {
    authedDeleteMock.mockRejectedValueOnce(new Error('down'));
    const result = (await actions.logoutall(
      actionEvent()
    )) as { status: number; data: MeActionData };
    expect(result.status).toBe(503);
  });
});

describe('mfa-enroll / mfa-confirm / mfa-cancel（TOTP enrollment）', () => {
  it('enroll 成功 → enroll-challenge（含 otpauth + secret）', async () => {
    authedPostMock.mockResolvedValueOnce({
      ok: true,
      data: { otpauth_uri: 'otpauth://totp/BBLBB:alice@example.com', secret_base32: 'JBSWY3DP' }
    });
    const result = (await actions['mfa-enroll'](actionEvent({}))) as MeActionData;
    expect(result.mfa).toEqual({
      kind: 'enroll-challenge',
      otpauth_uri: 'otpauth://totp/BBLBB:alice@example.com',
      secret_base32: 'JBSWY3DP'
    });
    const [cookies, path] = authedPostMock.mock.calls[0];
    expect(path).toBe('/api/v1/auth/mfa/enroll');
    expect(cookies.get).toBeTypeOf('function');
  });

  it('confirm code 非法（非 6 位数字）→ 422，不调用代理', async () => {
    const result = (await actions['mfa-confirm'](
      actionEvent({ code: '12' })
    )) as { status: number; data: MeActionData };
    expect(result.status).toBe(422);
    expect(authedPostMock).not.toHaveBeenCalled();
  });

  it('confirm 成功 → enroll-confirmed', async () => {
    authedPostMock.mockResolvedValueOnce({ ok: true, data: { ok: true } });
    const result = (await actions['mfa-confirm'](
      actionEvent({ code: '123456' })
    )) as MeActionData;
    expect(result.mfa).toEqual({ kind: 'enroll-confirmed' });
    const [, , body] = authedPostMock.mock.calls[0];
    expect(body).toEqual({ code: '123456' });
  });

  it('cancel 成功 → disabled 态', async () => {
    authedDeleteMock.mockResolvedValueOnce({ ok: true });
    const result = (await actions['mfa-cancel'](actionEvent({}))) as MeActionData;
    expect(result.mfa).toEqual({ kind: 'disabled' });
  });
});

describe('mfa-recovery / mfa-disable / re-auth（step-up 交互）', () => {
  it('recovery 成功 → recovery-codes（一次展示）', async () => {
    authedPostMock.mockResolvedValueOnce({
      ok: true,
      data: { codes: ['ABCDEFGHIJKLMNOP', 'QRSTUVWXYZ234567'] }
    });
    const result = (await actions['mfa-recovery'](actionEvent({}))) as MeActionData;
    expect(result.mfa).toEqual({
      kind: 'recovery-codes',
      codes: ['ABCDEFGHIJKLMNOP', 'QRSTUVWXYZ234567']
    });
    const [, path] = authedPostMock.mock.calls[0];
    expect(path).toBe('/api/v1/auth/mfa/recovery-codes');
  });

  it('recovery 遇 403 step_up_required → step-up 态（intent=recovery）', async () => {
    authedPostMock.mockResolvedValueOnce({
      ok: false,
      status: 403,
      message: '操作需要重新认证',
      requestId: 'rid-403',
      retryAfterSecs: null,
      code: 'step_up_required'
    });
    const result = (await actions['mfa-recovery'](actionEvent({}))) as MeActionData;
    expect(result.mfa).toEqual({ kind: 'step-up', intent: 'recovery' });
  });

  it('disable 成功 → disabled 态', async () => {
    authedDeleteMock.mockResolvedValueOnce({ ok: true });
    const result = (await actions['mfa-disable'](actionEvent({}))) as MeActionData;
    expect(result.mfa).toEqual({ kind: 'disabled' });
    const [, path] = authedDeleteMock.mock.calls[0];
    expect(path).toBe('/api/v1/auth/mfa');
  });

  it('disable 遇 403 step_up_required → step-up 态（intent=disable）', async () => {
    authedDeleteMock.mockResolvedValueOnce({
      ok: false,
      status: 403,
      message: '操作需要重新认证',
      requestId: 'rid-403',
      retryAfterSecs: null,
      code: 'step_up_required'
    });
    const result = (await actions['mfa-disable'](actionEvent({}))) as MeActionData;
    expect(result.mfa).toEqual({ kind: 'step-up', intent: 'disable' });
  });

  it('re-auth 缺密码 → 422', async () => {
    const result = (await actions['re-auth'](
      actionEvent({ intent: 'disable' })
    )) as { status: number; data: MeActionData };
    expect(result.status).toBe(422);
    expect(authedPostMock).not.toHaveBeenCalled();
  });

  it('re-auth 成功 → reauth-done（intent 透传）', async () => {
    authedPostMock.mockResolvedValueOnce({ ok: true, data: { ok: true } });
    const result = (await actions['re-auth'](
      actionEvent({ password: 'password9', intent: 'disable' }, 'req-ra')
    )) as MeActionData;
    expect(result.mfa).toEqual({ kind: 'reauth-done', intent: 'disable' });
    const [cookies, path, body, requestId] = authedPostMock.mock.calls[0];
    expect(path).toBe('/api/v1/auth/re-auth');
    expect(body).toEqual({ password: 'password9' });
    expect(requestId).toBe('req-ra');
    expect(cookies.get).toBeTypeOf('function');
  });

  it('re-auth 失败（401）→ fail(401) 透传', async () => {
    authedPostMock.mockResolvedValueOnce({
      ok: false,
      status: 401,
      message: '密码不正确',
      requestId: 'rid-401',
      retryAfterSecs: null,
      code: 'unauthorized'
    });
    const result = (await actions['re-auth'](
      actionEvent({ password: 'wrong', intent: 'disable' })
    )) as { status: number; data: MeActionData };
    expect(result.status).toBe(401);
    expect(result.data.message).toContain('密码');
  });
});
