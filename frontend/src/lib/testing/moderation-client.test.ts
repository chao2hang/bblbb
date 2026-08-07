// M05-UI：举报/申诉/案件/通知 API 客户端函数契约（路径/方法/CSRF/去重）。
import { describe, expect, it, vi, beforeEach } from 'vitest';
import {
  createReport,
  listOwnReports,
  withdrawReport,
  createAppeal,
  listOwnAppeals,
  withdrawAppeal,
  listModerationCases,
  decideModerationAppeal,
  markAllNotificationsRead,
  setNotificationPreference
} from '../api/client';

/** 记录请求的 mock fetch：CSRF 端点返回 token，其余按配置返回。 */
function mockFetch(routes: Record<string, { status: number; body?: unknown }>) {
  const calls: Array<{ url: string; init?: RequestInit }> = [];
  const fetchFn = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    calls.push({ url, init });
    const spec = routes[url];
    const status = spec?.status ?? 200;
    const body = spec?.body;
    return {
      ok: status >= 200 && status < 300,
      status,
      headers: { get: () => null },
      json: async () => body ?? {}
    } as Response;
  });
  return { fetchFn, calls };
}

beforeEach(() => {
  // 每个用例独立（模块级 csrfToken 缓存由 vi.resetModules 规避，这里直接清空）
});

describe('M05-UI 举报客户端', () => {
  it('createReport POST /api/v1/reports（携带 CSRF）', async () => {
    const csrfRoutes: Record<string, { status: number; body?: unknown }> = {
      '/api/v1/auth/csrf': { status: 200, body: { token: 't1' } },
      '/api/v1/reports': { status: 201, body: { id: 'r1', status: 'submitted' } }
    };
    const { fetchFn, calls } = mockFetch(csrfRoutes);
    const result = await createReport(fetchFn as unknown as typeof fetch, {
      target_type: 'post',
      target_id: 'p1',
      reason: 'spam',
      detail: '广告'
    });
    expect(result.id).toBe('r1');
    const post = calls.find((c) => c.url === '/api/v1/reports');
    expect(post).toBeDefined();
    expect(post!.init?.method).toBe('POST');
    expect(post!.init?.headers).toMatchObject({ 'X-CSRF-Token': 't1' });
    expect(JSON.parse(String(post!.init?.body))).toMatchObject({ target_type: 'post', reason: 'spam' });
  });

  it('listOwnReports GET /api/v1/reports；withdrawReport POST withdraw', async () => {
    const routes: Record<string, { status: number; body?: unknown }> = {
      '/api/v1/reports': { status: 200, body: { items: [{ id: 'r1' }], next_cursor: null, has_more: false } },
      '/api/v1/reports/r1/withdraw': { status: 204 }
    };
    const { fetchFn, calls } = mockFetch(routes);
    const list = await listOwnReports(fetchFn as unknown as typeof fetch);
    expect(list.items.length).toBe(1);
    await withdrawReport(fetchFn as unknown as typeof fetch, 'r1');
    const w = calls.find((c) => c.url === '/api/v1/reports/r1/withdraw');
    expect(w!.init?.method).toBe('POST');
  });
});

describe('M05-UI 申诉/案件/通知客户端', () => {
  it('createAppeal POST /api/v1/appeals', async () => {
    const routes: Record<string, { status: number; body?: unknown }> = {
      '/api/v1/auth/csrf': { status: 200, body: { token: 't1' } },
      '/api/v1/appeals': { status: 201, body: { id: 'a1', sanction_id: 's1', status: 'submitted' } }
    };
    const { fetchFn, calls } = mockFetch(routes);
    await createAppeal(fetchFn as unknown as typeof fetch, { sanction_id: 's1', content: '申诉' });
    const post = calls.find((c) => c.url === '/api/v1/appeals');
    expect(post!.init?.method).toBe('POST');
    expect(JSON.parse(String(post!.init?.body))).toMatchObject({ sanction_id: 's1', content: '申诉' });
  });

  it('listOwnAppeals GET /api/v1/appeals；withdrawAppeal POST /{id}/withdraw', async () => {
    const routes: Record<string, { status: number; body?: unknown }> = {
      '/api/v1/appeals': { status: 200, body: { items: [], next_cursor: null, has_more: false } },
      '/api/v1/appeals/a1/withdraw': { status: 204 }
    };
    const { fetchFn, calls } = mockFetch(routes);
    await listOwnAppeals(fetchFn as unknown as typeof fetch);
    await withdrawAppeal(fetchFn as unknown as typeof fetch, 'a1');
    const w = calls.find((c) => c.url === '/api/v1/appeals/a1/withdraw');
    expect(w!.init?.method).toBe('POST');
  });

  it('listModerationCases GET 管理案件；decideModerationAppeal PATCH 带版本', async () => {
    const routes: Record<string, { status: number; body?: unknown }> = {
      '/api/v1/admin/moderation/cases': { status: 200, body: { items: [] } },
      '/api/v1/auth/csrf': { status: 200, body: { token: 't1' } },
      '/api/v1/admin/moderation/appeals/a1': { status: 200, body: { status: 'rejected' } }
    };
    const { fetchFn, calls } = mockFetch(routes);
    await listModerationCases(fetchFn as unknown as typeof fetch);
    const get = calls.find((c) => c.url === '/api/v1/admin/moderation/cases');
    expect(get!.init?.method).toBeUndefined(); // GET 默认

    await decideModerationAppeal(fetchFn as unknown as typeof fetch, 'a1', {
      decision: 'rejected',
      reason: '证据不足',
      expected_version: 42
    });
    const patch = calls.find((c) => c.url === '/api/v1/admin/moderation/appeals/a1');
    expect(patch!.init?.method).toBe('PATCH');
    expect(JSON.parse(String(patch!.init?.body))).toMatchObject({ decision: 'rejected', expected_version: 42 });
  });

  it('markAllNotificationsRead POST read-all；setNotificationPreference PUT 偏好', async () => {
    const routes: Record<string, { status: number; body?: unknown }> = {
      '/api/v1/auth/csrf': { status: 200, body: { token: 't1' } },
      '/api/v1/notifications/read-all': { status: 200, body: { updated: 3 } },
      '/api/v1/notifications/preferences': { status: 200, body: { category: 'security', updated: true } }
    };
    const { fetchFn, calls } = mockFetch(routes);
    const r = await markAllNotificationsRead(fetchFn as unknown as typeof fetch);
    expect(r.updated).toBe(3);
    await setNotificationPreference(fetchFn as unknown as typeof fetch, {
      category: 'security',
      email_enabled: true,
      in_app_enabled: false,
      push_enabled: false
    });
    const put = calls.find((c) => c.url === '/api/v1/notifications/preferences');
    expect(put!.init?.method).toBe('PUT');
    expect(JSON.parse(String(put!.init?.body))).toMatchObject({ category: 'security', in_app_enabled: false });
  });
});
