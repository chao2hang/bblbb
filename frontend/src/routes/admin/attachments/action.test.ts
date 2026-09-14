// 附件管理动作测试：删除与封禁违规用户（banUser）
import { describe, expect, it, vi } from 'vitest';
import { actions } from './+page.server';

vi.mock('$lib/api/server', () => ({
  getAuthed: vi.fn(async (_cookies, path: string) => {
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
  }),
  authedPatch: vi.fn(async () => ({ ok: true, data: { status: 'banned' } })),
  authedDeleteBody: vi.fn(async () => ({ ok: true }))
}));

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
});
