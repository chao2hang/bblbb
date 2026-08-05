// M02-UX-04：忘记密码页服务端 action 测试——邮箱校验、统一 202（不泄漏）、
// 429 冷却透传、代理异常。
import { afterEach, describe, expect, it, vi } from 'vitest';
import { actions } from './+page.server';
import { requestPasswordResetViaServer } from '$lib/api/server';
import type { PasswordResetRequestData } from './+page.server';

vi.mock('$lib/api/server', () => ({
  requestPasswordResetViaServer: vi.fn()
}));

const requestResetMock = requestPasswordResetViaServer as unknown as ReturnType<typeof vi.fn>;

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
    url: new URL('http://localhost/password-reset')
  } as unknown as Parameters<typeof actions.default>[0];
}

afterEach(() => {
  vi.clearAllMocks();
});

describe('password-reset action（忘记密码）', () => {
  it('邮箱缺失 → 422，不调用代理', async () => {
    const result = (await actions.default(actionEvent({}))) as {
      status: number;
      data: PasswordResetRequestData;
    };
    expect(result.status).toBe(422);
    expect(requestResetMock).not.toHaveBeenCalled();
  });

  it('邮箱格式错误 → 422', async () => {
    const result = (await actions.default(actionEvent({ email: 'not-an-email' }))) as {
      status: number;
      data: PasswordResetRequestData;
    };
    expect(result.status).toBe(422);
    expect(requestResetMock).not.toHaveBeenCalled();
  });

  it('统一 202 → sent（不泄漏邮箱是否注册）', async () => {
    requestResetMock.mockResolvedValueOnce({ ok: true });
    const result = (await actions.default(
      actionEvent({ email: 'alice@example.com' }, 'req-1')
    )) as PasswordResetRequestData;
    expect(result.sent).toBe(true);
    expect(result.email).toBe('alice@example.com');
    const [cookies, email, requestId] = requestResetMock.mock.calls[0];
    expect(email).toBe('alice@example.com');
    expect(requestId).toBe('req-1');
    expect(cookies.get).toBeTypeOf('function');
  });

  it('429 → fail(429) 透传冷却秒数', async () => {
    requestResetMock.mockResolvedValueOnce({
      ok: false,
      status: 429,
      message: '操作过于频繁，请稍后再试',
      requestId: 'rid-429',
      retryAfterSecs: 45
    });
    const result = (await actions.default(
      actionEvent({ email: 'alice@example.com' })
    )) as { status: number; data: PasswordResetRequestData };
    expect(result.status).toBe(429);
    expect(result.data.cooldown).toBe(45);
  });

  it('代理抛错 → fail(503)', async () => {
    requestResetMock.mockRejectedValueOnce(new Error('down'));
    const result = (await actions.default(
      actionEvent({ email: 'alice@example.com' })
    )) as { status: number; data: PasswordResetRequestData };
    expect(result.status).toBe(503);
  });
});
