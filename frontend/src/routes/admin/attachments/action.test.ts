// 附件管理动作测试：删除与封禁违规用户（banUser）；
// step-up 交互（M02-MFA-07）：删除/批量删除/封禁命中 403 step_up_required 时
// 返回 stepUpRequired（交由页面 re-auth 密码弹窗处理，不再裸报错）——批量删除
// 命中即中止（后续必然同样失败，不再空转 30 次调用）；reauth 重认证。
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { actions } from './+page.server';
import { authedDeleteBody, authedPatch, authedPost, getAuthed } from '$lib/api/server';
import type { AdminAttachmentsActionData } from './+page.server';

vi.mock('$lib/api/server', () => ({
  getAuthed: vi.fn(),
  authedPatch: vi.fn(),
  authedDeleteBody: vi.fn(),
  authedPost: vi.fn()
}));

const getAuthedMock = getAuthed as unknown as ReturnType<typeof vi.fn>;
const patchMock = authedPatch as unknown as ReturnType<typeof vi.fn>;
const deleteBodyMock = authedDeleteBody as unknown as ReturnType<typeof vi.fn>;
const postMock = authedPost as unknown as ReturnType<typeof vi.fn>;

beforeEach(() => {
  getAuthedMock.mockImplementation(async (_cookies: unknown, path: string) => {
    if (path.includes('baduser')) {
      return {
        ok: true,
        data: {
          items: [{ id: 'u-bad', username: 'baduser', status: 'active', version: 2 }]
        }
      };
    }
    if (path.includes('alreadybanned')) {
      return {
        ok: true,
        data: {
          items: [{ id: 'u-banned', username: 'alreadybanned', status: 'banned', version: 1 }]
        }
      };
    }
    return { ok: false, status: 404, message: 'not found' };
  });
  patchMock.mockResolvedValue({ ok: true, data: { status: 'banned' } });
  deleteBodyMock.mockResolvedValue({ ok: true, data: {} });
  postMock.mockResolvedValue({ ok: true, data: {} });
});

afterEach(() => {
  vi.resetAllMocks();
});

function mockRequest(entries: Record<string, string>): Request {
  const fd = new FormData();
  for (const [k, v] of Object.entries(entries)) {
    fd.append(k, v);
  }
  return {
    formData: async () => fd,
    headers: new Headers({ 'x-request-id': 'test-req' })
  } as unknown as Request;
}

const mockCookies = {} as any;

/** 403 step_up_required 的标准失败结果（ServerWriteFailure 形态）。 */
const stepUpFailure = {
  ok: false,
  status: 403,
  message: '此操作需要重新验证身份，请重新验证后重试',
  requestId: 'rid-stepup' as string | null,
  retryAfterSecs: null,
  code: 'step_up_required' as string | null
};

describe('附件管理 action: banUser', () => {
  it('缺少用户名或原因时返回 422 校验失败', async () => {
    const res1 = await (actions.banUser as any)({
      request: mockRequest({ username: '', reason: '违规' }),
      cookies: mockCookies
    });
    expect(res1.status).toBe(422);

    const res2 = await (actions.banUser as any)({
      request: mockRequest({ username: 'baduser', reason: '' }),
      cookies: mockCookies
    });
    expect(res2.status).toBe(422);
  });

  it('用户不存在时返回 404', async () => {
    const res = await (actions.banUser as any)({
      request: mockRequest({ username: 'nonexistent', reason: '违规附件' }),
      cookies: mockCookies
    });
    expect(res.status).toBe(404);
  });

  it('用户已处于封禁状态时拒绝重复封禁', async () => {
    const res = await (actions.banUser as any)({
      request: mockRequest({ username: 'alreadybanned', reason: '违规' }),
      cookies: mockCookies
    });
    expect(res.status).toBe(400);
  });

  it('有效参数成功封禁用户并可同时清理违规附件', async () => {
    const res = await (actions.banUser as any)({
      request: mockRequest({
        username: 'baduser',
        reason: '发布违规色情图片',
        delete_attachment_id: 'att-malicious-1'
      }),
      cookies: mockCookies
    });
    expect(res.message).toContain('已成功封禁违规用户「baduser」');
    expect(res.message).toContain('并清理对应违规附件');
  });

  it('封禁 PATCH 命中 403 step_up_required → fail(403) stepUpRequired=true，不执行附件软删除', async () => {
    patchMock.mockResolvedValueOnce({ ...stepUpFailure });
    const res = await (actions.banUser as any)({
      request: mockRequest({
        username: 'baduser',
        reason: '上传违规附件',
        delete_attachment_id: 'att-1'
      }),
      cookies: mockCookies
    });
    expect(res.status).toBe(403);
    expect(res.data.stepUpRequired).toBe(true);
    expect(res.data.message).toContain('重新验证身份');
    expect(deleteBodyMock).not.toHaveBeenCalled();
  });
});

describe('附件管理 action: delete（单条删除）', () => {
  it('命中 403 step_up_required → fail(403) stepUpRequired=true', async () => {
    deleteBodyMock.mockResolvedValueOnce({ ...stepUpFailure });
    const res = await (actions.delete as any)({
      request: mockRequest({ id: 'att-1', reason: '违规清理' }),
      cookies: mockCookies
    });
    expect(res.status).toBe(403);
    expect(res.data.stepUpRequired).toBe(true);
    expect(res.data.message).toContain('重新验证身份');
    expect(deleteBodyMock.mock.calls[0][1]).toBe('/api/v1/admin/attachments/att-1');
  });

  it('其他失败（非 step-up）→ 原样透传，不带 stepUpRequired', async () => {
    deleteBodyMock.mockResolvedValueOnce({
      ok: false,
      status: 500,
      message: '服务器开小差了，请稍后重试',
      requestId: 'rid-500',
      retryAfterSecs: null,
      code: 'internal_error'
    });
    const res = await (actions.delete as any)({
      request: mockRequest({ id: 'att-1', reason: '违规清理' }),
      cookies: mockCookies
    });
    expect(res.status).toBe(500);
    expect(res.data.stepUpRequired).toBeUndefined();
  });
});

describe('附件管理 action: batchDelete（批量删除）', () => {
  it('首条命中 403 step_up_required → fail(403) stepUpRequired=true 且中止（不再调用后续项）', async () => {
    deleteBodyMock.mockResolvedValue({ ...stepUpFailure });
    const res = await (actions.batchDelete as any)({
      request: mockRequest({ ids: 'a,b,c', reason: '违规清理' }),
      cookies: mockCookies
    });
    expect(res.status).toBe(403);
    expect(res.data.stepUpRequired).toBe(true);
    // 中止：step-up 未通过时后续条目必然同样失败，不再空转
    expect(deleteBodyMock).toHaveBeenCalledTimes(1);
  });

  it('全部成功 → 汇总成功文案', async () => {
    const res = await (actions.batchDelete as any)({
      request: mockRequest({ ids: 'a,b', reason: '违规清理' }),
      cookies: mockCookies
    });
    expect(res.message).toBe('批量删除附件完成：成功 2 项');
    expect(deleteBodyMock).toHaveBeenCalledTimes(2);
  });

  it('普通失败（非 step-up）→ 汇总失败文案且逐条尝试，不带 stepUpRequired', async () => {
    deleteBodyMock.mockResolvedValue({
      ok: false,
      status: 409,
      message: '存储状态已变化，请刷新后重试',
      requestId: 'rid-409',
      retryAfterSecs: null,
      code: 'storage_conflict'
    });
    const res = await (actions.batchDelete as any)({
      request: mockRequest({ ids: 'a,b', reason: '违规清理' }),
      cookies: mockCookies
    });
    expect(res.status).toBe(500);
    expect(res.data.message).toBe('批量删除附件失败：2 项均未成功（首个失败 a：存储状态已变化，请刷新后重试）');
    expect(res.data.stepUpRequired).toBeUndefined();
    expect(deleteBodyMock).toHaveBeenCalledTimes(2);
  });
});

describe('附件管理 action: reauth（step-up 重认证）', () => {
  it('缺密码 → 422，不调用代理', async () => {
    const res = await (actions.reauth as any)({
      request: mockRequest({}),
      cookies: mockCookies
    });
    expect(res.status).toBe(422);
    expect(postMock).not.toHaveBeenCalled();
  });

  it('成功 → 提示重试原操作（POST /api/v1/auth/re-auth）', async () => {
    const res = await (actions.reauth as any)({
      request: mockRequest({ password: 'Passw0rd!' }),
      cookies: mockCookies
    });
    expect(res.message).toBe('已重新验证身份，请重试刚才的操作');
    expect(postMock.mock.calls[0][1]).toBe('/api/v1/auth/re-auth');
    expect(postMock.mock.calls[0][2]).toEqual({ password: 'Passw0rd!' });
  });

  it('密码错误（401）→ fail 透传', async () => {
    postMock.mockResolvedValueOnce({
      ok: false,
      status: 401,
      message: '密码不正确，请重试',
      requestId: 'rid-401',
      retryAfterSecs: null,
      code: 'reauth_password_invalid'
    });
    const res = await (actions.reauth as any)({
      request: mockRequest({ password: 'wrong' }),
      cookies: mockCookies
    });
    expect(res.status).toBe(401);
    expect(res.data.message).toBe('密码不正确，请重试');
  });
});
