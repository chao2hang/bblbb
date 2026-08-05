// M02-UX-04：重置密码页服务端 action 测试——token/密码校验、成功 ok、
// 无效 token 400、代理异常。
import { afterEach, describe, expect, it, vi } from 'vitest';
import { actions } from './+page.server';
import { confirmPasswordResetViaServer } from '$lib/api/server';
import type { PasswordResetConfirmData } from './+page.server';

vi.mock('$lib/api/server', () => ({
  confirmPasswordResetViaServer: vi.fn()
}));

const confirmResetMock = confirmPasswordResetViaServer as unknown as ReturnType<typeof vi.fn>;

function actionEvent(
  entries: Record<string, string>,
  requestId: string | null = null
): Parameters<typeof actions.default>[0] {
  const fd = new FormData();
  for (const [k, v] of Object.entries(entries)) fd.set(k, v);
  const headers = new Headers();
  if (requestId) headers.set('x-request-id', requestId);
  return {
    request: { formData: () => Promise.resolve(fd), headers },
    cookies: { get: vi.fn(() => null), set: vi.fn() },
    url: new URL('http://localhost/password-reset/confirm?token=tok')
  } as unknown as Parameters<typeof actions.default>[0];
}

afterEach(() => {
  vi.clearAllMocks();
});

describe('password-reset confirm action（重置密码）', () => {
  it('token 缺失 → 422', async () => {
    const result = (await actions.default(
      actionEvent({ password: 'newpass123', confirm: 'newpass123' })
    )) as { status: number; data: PasswordResetConfirmData };
    expect(result.status).toBe(422);
    expect(confirmResetMock).not.toHaveBeenCalled();
  });

  it('密码过短 → 422 字段错误（不调用代理）', async () => {
    const result = (await actions.default(
      actionEvent({ token: 'tok', password: 'short', confirm: 'short' })
    )) as { status: number; data: PasswordResetConfirmData };
    expect(result.status).toBe(422);
    expect(result.data.fieldErrors?.password).toBe('密码需为 8-128 个字符');
    expect(confirmResetMock).not.toHaveBeenCalled();
  });

  it('两次密码不一致 → 422 字段错误', async () => {
    const result = (await actions.default(
      actionEvent({ token: 'tok', password: 'newpass123', confirm: 'newpass124' })
    )) as { status: number; data: PasswordResetConfirmData };
    expect(result.status).toBe(422);
    expect(result.data.fieldErrors?.confirm).toBe('两次输入的密码不一致');
  });

  it('成功 → ok:true（后端已撤销其他 Session）', async () => {
    confirmResetMock.mockResolvedValueOnce({ ok: true });
    const result = (await actions.default(
      actionEvent({ token: 'tok', password: 'newpass123', confirm: 'newpass123' }, 'req-1')
    )) as PasswordResetConfirmData;
    expect(result.ok).toBe(true);
    const [cookies, token, password, requestId] = confirmResetMock.mock.calls[0];
    expect(token).toBe('tok');
    expect(password).toBe('newpass123');
    expect(requestId).toBe('req-1');
    expect(cookies.get).toBeTypeOf('function');
  });

  it('无效/已消费/过期 token（400）→ fail(400)', async () => {
    confirmResetMock.mockResolvedValueOnce({
      ok: false,
      status: 400,
      message: '重置链接无效或已过期',
      requestId: 'rid-400',
      retryAfterSecs: null
    });
    const result = (await actions.default(
      actionEvent({ token: 'stale', password: 'newpass123', confirm: 'newpass123' })
    )) as { status: number; data: PasswordResetConfirmData };
    expect(result.status).toBe(400);
    expect(result.data.message).toContain('重置链接');
  });

  it('代理抛错 → fail(503)', async () => {
    confirmResetMock.mockRejectedValueOnce(new Error('down'));
    const result = (await actions.default(
      actionEvent({ token: 'tok', password: 'newpass123', confirm: 'newpass123' })
    )) as { status: number; data: PasswordResetConfirmData };
    expect(result.status).toBe(503);
  });
});
