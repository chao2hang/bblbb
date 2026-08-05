// M02-UX-03：登录页服务端表单 action 测试——密码步（会话/两步/失败）、
// 第二步（TOTP/恢复码）、缺失字段、redirect。
import { afterEach, describe, expect, it, vi } from 'vitest';
import { actions } from './+page.server';
import { loginMfaViaServer, loginViaServer } from '$lib/api/server';
import type { LoginActionData } from './+page.server';

vi.mock('$lib/api/server', () => ({
  loginViaServer: vi.fn(),
  loginMfaViaServer: vi.fn()
}));

const loginMock = loginViaServer as unknown as ReturnType<typeof vi.fn>;
const loginMfaMock = loginMfaViaServer as unknown as ReturnType<typeof vi.fn>;

function actionEvent(
  entries: Record<string, string>,
  requestId: string | null = null,
  next: string | null = null
): Parameters<typeof actions.login>[0] {
  const fd = new FormData();
  for (const [k, v] of Object.entries(entries)) fd.set(k, v);
  const headers = new Headers();
  if (requestId) headers.set('x-request-id', requestId);
  const url = new URL(`http://localhost/login${next ? `?next=${next}` : ''}`);
  return {
    request: { formData: () => Promise.resolve(fd), headers },
    cookies: { get: vi.fn(() => null), set: vi.fn() },
    url
  } as unknown as Parameters<typeof actions.login>[0];
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

describe('login action（密码步）', () => {
  it('缺字段 → 422', async () => {
    const result = (await actions.login(actionEvent({ identifier: '' }))) as {
      status: number;
      data: LoginActionData;
    };
    expect(result.status).toBe(422);
    expect(loginMock).not.toHaveBeenCalled();
  });

  it('无 TOTP 用户登录成功 → redirect 到 /', async () => {
    loginMock.mockResolvedValueOnce({ kind: 'ok' });
    const result = await runAction(() => actions.login(actionEvent({ identifier: 'alice', password: 'password9' })));
    expect(isRedirectResult(result)).toBe(true);
    if (isRedirectResult(result)) {
      expect(result.status).toBe(303);
      expect(result.location).toBe('/');
    }
    const [cookies, input] = loginMock.mock.calls[0];
    expect(input).toEqual({ identifier: 'alice', password: 'password9' });
    expect(cookies.get).toBeTypeOf('function');
  });

  it('redirect 遵循 ?next=（仅站内路径）', async () => {
    loginMock.mockResolvedValueOnce({ kind: 'ok' });
    const result = await runAction(() =>
      actions.login(actionEvent({ identifier: 'alice', password: 'password9' }, null, '/me'))
    );
    expect(isRedirectResult(result)).toBe(true);
    if (isRedirectResult(result)) expect(result.location).toBe('/me');
  });

  it('TOTP 用户 → mfa_required + challenge_token（不 redirect）', async () => {
    loginMock.mockResolvedValueOnce({ kind: 'mfa', challengeToken: 'ch-1' });
    const result = (await actions.login(
      actionEvent({ identifier: 'bob', password: 'password9' })
    )) as LoginActionData;
    expect(result.mfa_required).toBe(true);
    expect(result.challenge_token).toBe('ch-1');
  });

  it('密码错误（401）→ fail(401) 透传，不泄漏账号状态', async () => {
    loginMock.mockResolvedValueOnce({
      kind: 'error',
      status: 401,
      message: '用户名或密码不正确',
      requestId: 'rid-1'
    });
    const result = (await actions.login(
      actionEvent({ identifier: 'alice', password: 'wrong-pass9' })
    )) as { status: number; data: LoginActionData };
    expect(result.status).toBe(401);
    expect(result.data.message).toBe('用户名或密码不正确');
  });

  it('代理抛错 → fail(503)', async () => {
    loginMock.mockRejectedValueOnce(new Error('down'));
    const result = (await actions.login(
      actionEvent({ identifier: 'alice', password: 'password9' })
    )) as { status: number; data: LoginActionData };
    expect(result.status).toBe(503);
  });
});

describe('mfa action（第二步）', () => {
  it('challenge 缺失 → 422', async () => {
    const result = (await actions.mfa(actionEvent({ totp_code: '123456' }))) as {
      status: number;
      data: LoginActionData;
    };
    expect(result.status).toBe(422);
    expect(loginMfaMock).not.toHaveBeenCalled();
  });

  it('验证码/恢复码都缺 → 422', async () => {
    const result = (await actions.mfa(actionEvent({ challenge_token: 'ch-1' }))) as {
      status: number;
      data: LoginActionData;
    };
    expect(result.status).toBe(422);
  });

  it('TOTP 成功 → redirect（会话 Cookie 已复制）', async () => {
    loginMfaMock.mockResolvedValueOnce({ ok: true });
    const result = await runAction(() =>
      actions.mfa(actionEvent({ challenge_token: 'ch-1', totp_code: '123456' }, 'req-mfa'))
    );
    expect(isRedirectResult(result)).toBe(true);
    const [, input, requestId] = loginMfaMock.mock.calls[0];
    expect(input).toEqual({ challenge_token: 'ch-1', totp_code: '123456', recovery_code: undefined });
    expect(requestId).toBe('req-mfa');
  });

  it('恢复码成功 → redirect', async () => {
    loginMfaMock.mockResolvedValueOnce({ ok: true });
    const result = await runAction(() =>
      actions.mfa(actionEvent({ challenge_token: 'ch-1', recovery_code: 'ABCDEFGHIJKLMNOP' }))
    );
    expect(isRedirectResult(result)).toBe(true);
    const [, input] = loginMfaMock.mock.calls[0];
    expect(input).toEqual({
      challenge_token: 'ch-1',
      totp_code: undefined,
      recovery_code: 'ABCDEFGHIJKLMNOP'
    });
  });

  it('验证码错误（401）→ fail(401)', async () => {
    loginMfaMock.mockResolvedValueOnce({
      ok: false,
      status: 401,
      message: '验证码不正确或已过期',
      requestId: 'rid-2'
    });
    const result = (await actions.mfa(
      actionEvent({ challenge_token: 'ch-1', totp_code: '000000' })
    )) as { status: number; data: LoginActionData };
    expect(result.status).toBe(401);
    expect(result.data.message).toContain('验证码');
  });
});
