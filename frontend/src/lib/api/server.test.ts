// M02-UX-01：SSR 服务端 API 代理测试——Set-Cookie 逐属性复制、
// 预认证 CSRF 配对、Cookie/X-Request-ID 转发、Problem 映射。
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Cookies } from '@sveltejs/kit';
import {
  cookieValueFromSetCookie,
  parseSetCookie,
  PREAUTH_COOKIE,
  registerViaServer,
  relaySetCookies,
  resendVerificationViaServer,
  verifyEmailViaServer
} from './server';

interface SetCall {
  name: string;
  value: string;
  options: Record<string, unknown>;
}

function mockCookies(initial: Record<string, string> = {}): Cookies & {
  setCalls: SetCall[];
  get: ReturnType<typeof vi.fn>;
} {
  const store = new Map(Object.entries(initial));
  const setCalls: SetCall[] = [];
  const get = vi.fn((name: string) => store.get(name) ?? null);
  const set = vi.fn((name: string, value: string, options: Record<string, unknown> = {}) => {
    store.set(name, value);
    setCalls.push({ name, value, options });
  });
  return { get, set, setCalls } as unknown as Cookies & { setCalls: SetCall[]; get: ReturnType<typeof vi.fn> };
}

function jsonResponse(body: unknown, status: number, setCookie?: string[]): Response {
  const headers = new Headers({ 'Content-Type': 'application/json' });
  for (const cookie of setCookie ?? []) headers.append('Set-Cookie', cookie);
  return new Response(JSON.stringify(body), { status, headers });
}

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('parseSetCookie / relaySetCookies（逐属性复制）', () => {
  it('解析属性：Path/Secure/HttpOnly/SameSite/Max-Age', () => {
    const parsed = parseSetCookie(
      `${PREAUTH_COOKIE}=abc123; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=600`
    );
    expect(parsed).not.toBeNull();
    expect(parsed!.name).toBe(PREAUTH_COOKIE);
    expect(parsed!.value).toBe('abc123');
    expect(parsed!.path).toBe('/');
    expect(parsed!.secure).toBe(true);
    expect(parsed!.httpOnly).toBe(true);
    expect(parsed!.sameSite).toBe('lax');
    expect(parsed!.maxAge).toBe(600);
  });

  it('relay 通过 cookies.set 复制全部属性（不直接写 Set-Cookie 头）', () => {
    const cookies = mockCookies();
    const response = new Response(null, {
      headers: {
        'Set-Cookie': `${PREAUTH_COOKIE}=v1; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=600`
      }
    });
    relaySetCookies(response, cookies);
    expect(cookies.setCalls).toHaveLength(1);
    const call = cookies.setCalls[0];
    expect(call.name).toBe(PREAUTH_COOKIE);
    expect(call.value).toBe('v1');
    expect(call.options).toMatchObject({
      path: '/',
      secure: true,
      httpOnly: true,
      sameSite: 'lax',
      maxAge: 600
    });
  });

  it('cookieValueFromSetCookie 取指定 cookie 新值；无则 null', () => {
    const response = new Response(null, {
      headers: { 'Set-Cookie': `${PREAUTH_COOKIE}=fresh; Path=/` }
    });
    expect(cookieValueFromSetCookie(response, PREAUTH_COOKIE)).toBe('fresh');
    expect(cookieValueFromSetCookie(response, 'other')).toBeNull();
  });
});

describe('registerViaServer（预认证 CSRF + 转发 + 复制）', () => {
  it('成功路径：取 CSRF → 带 token/cookie/Request-ID 提交 → 复制 Set-Cookie → ok', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        jsonResponse({ token: 'csrf-token-1' }, 200, [
          `${PREAUTH_COOKIE}=issued-cookie; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=600`
        ])
      )
      .mockResolvedValueOnce(jsonResponse({ ok: true }, 201));
    vi.stubGlobal('fetch', fetchMock);
    const cookies = mockCookies();

    const result = await registerViaServer(
      cookies,
      { username: 'alice', email: 'alice@example.com', password: 'secret9' },
      'req-xyz'
    );

    expect(result).toEqual({ ok: true });

    // 第一次调用：GET /auth/csrf，转发浏览器无 cookie、带 X-Request-ID
    const [csrfUrl, csrfInit] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(csrfUrl).toContain('/api/v1/auth/csrf');
    expect((csrfInit.headers as Record<string, string>)['X-Request-ID']).toBe('req-xyz');
    expect((csrfInit.headers as Record<string, string>).Cookie).toBeUndefined();

    // 第二次调用：POST /auth/register，X-CSRF-Token + Cookie 配对 + 转发
    const [regUrl, regInit] = fetchMock.mock.calls[1] as [string, RequestInit];
    expect(regUrl).toContain('/api/v1/auth/register');
    const headers = regInit.headers as Record<string, string>;
    expect(headers['X-CSRF-Token']).toBe('csrf-token-1');
    expect(headers.Cookie).toBe(`${PREAUTH_COOKIE}=issued-cookie`);
    expect(headers['X-Request-ID']).toBe('req-xyz');
    expect(JSON.parse(regInit.body as string)).toEqual({
      username: 'alice',
      email: 'alice@example.com',
      password: 'secret9'
    });

    // 预认证 CSRF Set-Cookie 已复制到浏览器
    expect(cookies.setCalls).toHaveLength(1);
    expect(cookies.setCalls[0]).toMatchObject({ name: PREAUTH_COOKIE, value: 'issued-cookie' });
  });

  it('复用浏览器已有预认证 cookie：无新 Set-Cookie 时用旧值配对', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ token: 'csrf-token-2' }, 200))
      .mockResolvedValueOnce(jsonResponse({ ok: true }, 201));
    vi.stubGlobal('fetch', fetchMock);
    const cookies = mockCookies({ [PREAUTH_COOKIE]: 'browser-cookie' });

    const result = await registerViaServer(
      cookies,
      { username: 'bob', email: 'bob@example.com', password: 'secret9' }
    );
    expect(result).toEqual({ ok: true });

    const [csrfUrl, csrfInit] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect((csrfInit.headers as Record<string, string>).Cookie).toBe(
      `${PREAUTH_COOKIE}=browser-cookie`
    );
    const [regUrl, regInit] = fetchMock.mock.calls[1] as [string, RequestInit];
    const headers = regInit.headers as Record<string, string>;
    expect(headers['X-CSRF-Token']).toBe('csrf-token-2');
    expect(headers.Cookie).toBe(`${PREAUTH_COOKIE}=browser-cookie`);
    expect(cookies.setCalls).toHaveLength(0);
  });

  it('后端 429：映射为 ok:false + 状态码 + 中文文案（含 request_id）', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ token: 't' }, 200))
      .mockResolvedValueOnce(
        jsonResponse(
          {
            type: 'application/problem+json',
            status: 429,
            code: 'rate_limited',
            title: 'Too Many Requests',
            request_id: 'rid-429'
          },
          429
        )
      );
    vi.stubGlobal('fetch', fetchMock);
    const cookies = mockCookies();

    const result = await registerViaServer(
      cookies,
      { username: 'carol', email: 'carol@example.com', password: 'secret9' }
    );
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.status).toBe(429);
      expect(result.message).toContain('操作过于频繁');
      expect(result.requestId).toBe('rid-429');
    }
  });
});

describe('verifyEmailViaServer / resendVerificationViaServer（M02-UX-02）', () => {
  it('验证成功：带 token 提交 → ok:true', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ token: 't' }, 200))
      .mockResolvedValueOnce(jsonResponse({ ok: true }, 200));
    vi.stubGlobal('fetch', fetchMock);
    const cookies = mockCookies();

    const result = await verifyEmailViaServer(cookies, 'verify-token-1');
    expect(result).toEqual({ ok: true });

    const [url, init] = fetchMock.mock.calls[1] as [string, RequestInit];
    expect(url).toContain('/api/v1/auth/verify-email');
    expect(JSON.parse(init.body as string)).toEqual({ token: 'verify-token-1' });
  });

  it('验证 token 无效：映射 400 + 中文文案', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ token: 't' }, 200))
      .mockResolvedValueOnce(
        jsonResponse(
          { status: 400, code: 'bad_request', detail: 'invalid or expired verification token', request_id: 'rid-400' },
          400
        )
      );
    vi.stubGlobal('fetch', fetchMock);
    const cookies = mockCookies();

    const result = await verifyEmailViaServer(cookies, 'bad-token');
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.status).toBe(400);
      expect(result.requestId).toBe('rid-400');
    }
  });

  it('重发成功：统一 202 → ok:true（不泄漏邮箱存在性）', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ token: 't' }, 200))
      .mockResolvedValueOnce(jsonResponse({ ok: true }, 202));
    vi.stubGlobal('fetch', fetchMock);
    const cookies = mockCookies();

    const result = await resendVerificationViaServer(cookies, 'alice@example.com');
    expect(result).toEqual({ ok: true });

    const [url, init] = fetchMock.mock.calls[1] as [string, RequestInit];
    expect(url).toContain('/api/v1/auth/resend-verification');
    expect(JSON.parse(init.body as string)).toEqual({ email: 'alice@example.com' });
  });

  it('重发冷却命中：429 + Retry-After → retryAfterSecs 供倒计时', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ token: 't' }, 200))
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({ status: 429, code: 'rate_limited', title: 'Too Many Requests', request_id: 'rid-429' }),
          { status: 429, headers: { 'Content-Type': 'application/json', 'Retry-After': '45' } }
        )
      );
    vi.stubGlobal('fetch', fetchMock);
    const cookies = mockCookies();

    const result = await resendVerificationViaServer(cookies, 'alice@example.com');
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.status).toBe(429);
      expect(result.retryAfterSecs).toBe(45);
      expect(result.requestId).toBe('rid-429');
    }
  });
});
