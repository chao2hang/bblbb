// M18-MFA-01/M02-UX-SEC：/mfa 页服务端 action 测试——TOTP enrollment
// （enroll/confirm/cancel）、恢复码与停用（recovery/disable，含 step-up
// 交互）、step-up 重认证（re-auth）。自 /me/action.test.ts 平移并适配
// 本页 action 命名（功能拆分后两步验证管理收敛到 /mfa）。
import { afterEach, describe, expect, it, vi } from 'vitest';
import { actions } from './+page.server';
import { authedDelete, authedPost } from '$lib/api/server';
import type { MfaActionData } from './+page.server';

vi.mock('$lib/api/server', () => ({
  authedDelete: vi.fn(),
  authedPost: vi.fn(),
  getAuthed: vi.fn(),
  SESSION_COOKIE: 'dsh_session'
}));

const authedDeleteMock = authedDelete as unknown as ReturnType<typeof vi.fn>;
const authedPostMock = authedPost as unknown as ReturnType<typeof vi.fn>;

function actionEvent(
  entries: Record<string, string> = {},
  requestId: string | null = null
): Parameters<typeof actions.enroll>[0] {
  const fd = new FormData();
  for (const [k, v] of Object.entries(entries)) fd.set(k, v);
  const headers = new Headers();
  if (requestId) headers.set('x-request-id', requestId);
  return {
    request: { formData: () => Promise.resolve(fd), headers },
    cookies: { get: vi.fn(() => null), set: vi.fn() },
    url: new URL('http://localhost/mfa')
  } as unknown as Parameters<typeof actions.enroll>[0];
}

describe('enroll / confirm / cancel（TOTP enrollment）', () => {
  it('enroll 成功 → enroll-challenge（含 otpauth + secret + 二维码 data URL）', async () => {
    authedPostMock.mockResolvedValueOnce({
      ok: true,
      data: { otpauth_uri: 'otpauth://totp/BBLBB:alice@example.com', secret_base32: 'JBSWY3DP' }
    });
    const result = (await actions.enroll(actionEvent({}))) as MfaActionData;
    expect(result.mfa?.kind).toBe('enroll-challenge');
    if (result.mfa?.kind !== 'enroll-challenge') throw new Error('unreachable');
    expect(result.mfa.otpauth_uri).toBe('otpauth://totp/BBLBB:alice@example.com');
    expect(result.mfa.secret_base32).toBe('JBSWY3DP');
    // M18-MFA-01：服务端生成注册二维码（SVG data URL），页面 <img> 渲染
    expect(result.mfa.qr_data_url).toMatch(/^data:image\/svg\+xml;base64,/);
    const [cookies, path] = authedPostMock.mock.calls[0];
    expect(path).toBe('/api/v1/auth/mfa/enroll');
    expect(cookies.get).toBeTypeOf('function');
  });

  it('confirm code 非法（非 6 位数字）→ 422，不调用代理', async () => {
    const result = (await actions.confirm(
      actionEvent({ code: '12' })
    )) as { status: number; data: MfaActionData };
    expect(result.status).toBe(422);
    expect(authedPostMock).not.toHaveBeenCalled();
  });

  it('confirm 成功 → enroll-confirmed', async () => {
    authedPostMock.mockResolvedValueOnce({ ok: true, data: { ok: true } });
    const result = (await actions.confirm(
      actionEvent({ code: '123456' })
    )) as MfaActionData;
    expect(result.mfa).toEqual({ kind: 'enroll-confirmed' });
    const [, , body] = authedPostMock.mock.calls[0];
    expect(body).toEqual({ code: '123456' });
  });

  it('cancel 成功 → disabled 态', async () => {
    authedDeleteMock.mockResolvedValueOnce({ ok: true });
    const result = (await actions.cancel(actionEvent({}))) as MfaActionData;
    expect(result.mfa).toEqual({ kind: 'disabled' });
  });
});

describe('recovery / disable / re-auth（step-up 交互）', () => {
  it('recovery 成功 → recovery-codes（一次展示）', async () => {
    authedPostMock.mockResolvedValueOnce({
      ok: true,
      data: { codes: ['ABCDEFGHIJKLMNOP', 'QRSTUVWXYZ234567'] }
    });
    const result = (await actions.recovery(actionEvent({}))) as MfaActionData;
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
    const result = (await actions.recovery(actionEvent({}))) as MfaActionData;
    expect(result.mfa).toEqual({ kind: 'step-up', intent: 'recovery' });
  });

  it('disable 成功 → disabled 态', async () => {
    authedDeleteMock.mockResolvedValueOnce({ ok: true });
    const result = (await actions.disable(actionEvent({}))) as MfaActionData;
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
    const result = (await actions.disable(actionEvent({}))) as MfaActionData;
    expect(result.mfa).toEqual({ kind: 'step-up', intent: 'disable' });
  });

  it('re-auth 缺密码 → 422', async () => {
    const result = (await actions['re-auth'](
      actionEvent({ intent: 'disable' })
    )) as { status: number; data: MfaActionData };
    expect(result.status).toBe(422);
    expect(authedPostMock).not.toHaveBeenCalled();
  });

  it('re-auth 成功 → reauth-done（intent 透传）', async () => {
    authedPostMock.mockResolvedValueOnce({ ok: true, data: { ok: true } });
    const result = (await actions['re-auth'](
      actionEvent({ password: 'password9', intent: 'disable' }, 'req-ra')
    )) as MfaActionData;
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
    )) as { status: number; data: MfaActionData };
    expect(result.status).toBe(401);
    expect(result.data.message).toContain('密码');
  });
});

describe('passkeyRevoke（Passkey 撤销，M02-MFA-PK）', () => {
  it('id 缺失 → 422，不调用代理', async () => {
    const result = (await actions.passkeyRevoke(actionEvent({}))) as {
      status: number;
      data: MfaActionData;
    };
    expect(result.status).toBe(422);
    expect(authedDeleteMock).not.toHaveBeenCalled();
  });

  it('成功 → 空结果（invalidateAll 刷新列表），代理路径含 id', async () => {
    authedDeleteMock.mockResolvedValueOnce({ ok: true });
    const result = (await actions.passkeyRevoke(
      actionEvent({ id: 'pk-1' }, 'req-pk')
    )) as MfaActionData;
    expect(result.mfa).toBeUndefined();
    const [, path, requestId] = authedDeleteMock.mock.calls[0];
    expect(path).toBe('/api/v1/auth/passkeys/pk-1');
    expect(requestId).toBe('req-pk');
  });
});
