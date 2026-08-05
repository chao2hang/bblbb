// M02-UX-01：注册页服务端表单 action 测试——字段校验（与后端规则一致）、
// 代理失败映射（429/503）、统一冲突提示（后端 201 ok:true → action ok）。
import { afterEach, describe, expect, it, vi } from 'vitest';
import { actions } from './+page.server';
import { registerViaServer } from '$lib/api/server';
import type { RegisterActionData } from './+page.server';

vi.mock('$lib/api/server', () => ({
  registerViaServer: vi.fn()
}));

const registerViaServerMock = registerViaServer as unknown as ReturnType<typeof vi.fn>;

function actionEvent(entries: Record<string, string>, requestId: string | null = null): Parameters<typeof actions.default>[0] {
  const fd = new FormData();
  for (const [k, v] of Object.entries(entries)) fd.set(k, v);
  const headers = new Headers();
  if (requestId) headers.set('x-request-id', requestId);
  return {
    request: { formData: () => Promise.resolve(fd), headers },
    cookies: { get: vi.fn(() => null), set: vi.fn() }
  } as unknown as Parameters<typeof actions.default>[0];
}

afterEach(() => {
  vi.clearAllMocks();
});

describe('字段校验（与后端规则一致，$lib/validation）', () => {
  it('两次密码不一致 → 422 且 confirm 字段错误', async () => {
    const result = (await actions.default(
      actionEvent({ username: 'alice', email: 'a@example.com', password: 'secret99', confirm: 'different9' })
    )) as { status: number; data: RegisterActionData };
    expect(result.status).toBe(422);
    expect(result.data.fieldErrors?.confirm).toBe('两次输入的密码不一致');
    expect(result.data.values).toEqual({ username: 'alice', email: 'a@example.com' });
    expect(registerViaServerMock).not.toHaveBeenCalled();
  });

  it('用户名过短 + 邮箱格式错误 + 密码不含字母 → 多字段错误', async () => {
    const result = (await actions.default(
      actionEvent({ username: 'ab', email: 'not-an-email', password: '12345678', confirm: '12345678' })
    )) as { status: number; data: RegisterActionData };
    expect(result.status).toBe(422);
    expect(result.data.fieldErrors?.username).toBe('用户名需为 3-20 个字符');
    expect(result.data.fieldErrors?.email).toBe('邮箱格式不正确');
    expect(result.data.fieldErrors?.password).toBe('密码必须同时包含字母和数字');
  });
});

describe('代理调用与错误映射', () => {
  it('有效表单 → 调 registerViaServer（转发 X-Request-ID）→ ok', async () => {
    registerViaServerMock.mockResolvedValueOnce({ ok: true });
    const result = await actions.default(
      actionEvent({ username: 'alice', email: 'a@example.com', password: 'secret99', confirm: 'secret99' }, 'req-1')
    );
    expect(result).toEqual({ ok: true });
    expect(registerViaServerMock).toHaveBeenCalledTimes(1);
    const [cookies, input, requestId] = registerViaServerMock.mock.calls[0];
    expect(input).toEqual({ username: 'alice', email: 'a@example.com', password: 'secret99' });
    expect(requestId).toBe('req-1');
    expect(cookies.get).toBeTypeOf('function');
  });

  it('后端 429 限流 → fail(429) 透传 message/requestId 且保留 values', async () => {
    registerViaServerMock.mockResolvedValueOnce({
      ok: false,
      status: 429,
      message: '操作过于频繁，请稍后再试（请求号 rid-9）',
      requestId: 'rid-9'
    });
    const result = (await actions.default(
      actionEvent({ username: 'bob', email: 'b@example.com', password: 'secret99', confirm: 'secret99' })
    )) as { status: number; data: RegisterActionData };
    expect(result.status).toBe(429);
    expect(result.data.message).toContain('操作过于频繁');
    expect(result.data.requestId).toBe('rid-9');
    expect(result.data.values).toEqual({ username: 'bob', email: 'b@example.com' });
  });

  it('代理抛错（后端不可达）→ fail(503) 统一文案', async () => {
    registerViaServerMock.mockRejectedValueOnce(new Error('connect refused'));
    const result = (await actions.default(
      actionEvent({ username: 'carol', email: 'c@example.com', password: 'secret99', confirm: 'secret99' })
    )) as { status: number; data: RegisterActionData };
    expect(result.status).toBe(503);
    expect(result.data.message).toBe('注册服务暂时不可用，请稍后重试');
  });
});
