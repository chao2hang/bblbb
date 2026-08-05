// M02-UX-02：邮箱验证页服务端表单 action 测试——verify/resend 代理、
// 冷却（cooldown）映射、缺失 token/email 的 422。
import { afterEach, describe, expect, it, vi } from 'vitest';
import { actions } from './+page.server';
import { RESEND_COOLDOWN_SECS, resendVerificationViaServer, verifyEmailViaServer } from '$lib/api/server';
import type { VerifyEmailActionData } from './+page.server';

vi.mock('$lib/api/server', () => ({
  RESEND_COOLDOWN_SECS: 60,
  verifyEmailViaServer: vi.fn(),
  resendVerificationViaServer: vi.fn()
}));

const verifyMock = verifyEmailViaServer as unknown as ReturnType<typeof vi.fn>;
const resendMock = resendVerificationViaServer as unknown as ReturnType<typeof vi.fn>;

function actionEvent(entries: Record<string, string>, requestId: string | null = null): Parameters<typeof actions.verify>[0] {
  const fd = new FormData();
  for (const [k, v] of Object.entries(entries)) fd.set(k, v);
  const headers = new Headers();
  if (requestId) headers.set('x-request-id', requestId);
  return {
    request: { formData: () => Promise.resolve(fd), headers },
    cookies: { get: vi.fn(() => null), set: vi.fn() }
  } as unknown as Parameters<typeof actions.verify>[0];
}

afterEach(() => {
  vi.clearAllMocks();
});

describe('verify action', () => {
  it('token 有效 → verifyEmailViaServer 调用 + ok', async () => {
    verifyMock.mockResolvedValueOnce({ ok: true });
    const result = await actions.verify(actionEvent({ token: 'tok-1' }, 'req-1'));
    expect(result).toEqual({ ok: true });
    const [cookies, token, requestId] = verifyMock.mock.calls[0];
    expect(token).toBe('tok-1');
    expect(requestId).toBe('req-1');
  });

  it('token 缺失 → 422', async () => {
    const result = (await actions.verify(
      actionEvent({})
    )) as { status: number; data: VerifyEmailActionData };
    expect(result.status).toBe(422);
    expect(result.data.message).toContain('缺少 token');
    expect(verifyMock).not.toHaveBeenCalled();
  });

  it('后端 400（token 无效/过期）→ fail(400) 透传', async () => {
    verifyMock.mockResolvedValueOnce({
      ok: false,
      status: 400,
      message: '验证链接无效或已过期',
      requestId: 'rid-400',
      retryAfterSecs: null
    });
    const result = (await actions.verify(
      actionEvent({ token: 'bad' })
    )) as { status: number; data: VerifyEmailActionData };
    expect(result.status).toBe(400);
    expect(result.data.message).toContain('无效或已过期');
    expect(result.data.requestId).toBe('rid-400');
  });
});

describe('resend action', () => {
  it('重发成功 → sent + 默认冷却 60s + email 回填', async () => {
    resendMock.mockResolvedValueOnce({ ok: true });
    const result = (await actions.resend(
      actionEvent({ email: 'alice@example.com' })
    )) as unknown as VerifyEmailActionData;
    expect(result).toMatchObject({ sent: true, cooldown: RESEND_COOLDOWN_SECS, email: 'alice@example.com' });
  });

  it('冷却命中（429 + retryAfter 45）→ fail(429) 且 cooldown=45', async () => {
    resendMock.mockResolvedValueOnce({
      ok: false,
      status: 429,
      message: '操作过于频繁，请稍后再试',
      requestId: 'rid-429',
      retryAfterSecs: 45
    });
    const result = (await actions.resend(
      actionEvent({ email: 'bob@example.com' })
    )) as { status: number; data: VerifyEmailActionData };
    expect(result.status).toBe(429);
    expect(result.data.cooldown).toBe(45);
    expect(result.data.email).toBe('bob@example.com');
  });

  it('email 缺失 → 422', async () => {
    const result = (await actions.resend(
      actionEvent({})
    )) as { status: number; data: VerifyEmailActionData };
    expect(result.status).toBe(422);
    expect(resendMock).not.toHaveBeenCalled();
  });

  it('代理抛错 → fail(503) 统一文案', async () => {
    resendMock.mockRejectedValueOnce(new Error('down'));
    const result = (await actions.resend(
      actionEvent({ email: 'c@example.com' })
    )) as { status: number; data: VerifyEmailActionData };
    expect(result.status).toBe(503);
    expect(result.data.message).toBe('重发服务暂时不可用，请稍后重试');
  });
});
