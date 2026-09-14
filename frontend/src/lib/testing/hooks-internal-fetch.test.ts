// hooks.server.ts handleFetch 内部重定向回归测试（origin_not_allowed 修复）。
//
// 背景：SvelteKit 的 event.fetch 会给非 GET 内部请求合成 `Origin: <前端站点
// origin>`（@sveltejs/kit runtime/server/fetch.js：缺 origin 则 set，仅
// GET/HEAD 同源时移除）。handleFetch 把请求重定向到 INTERNAL_API_ORIGIN
// （默认 http://127.0.0.1:8080）后，后端看到的 Host 是内部 API 主机；
// 若 Origin 头原样透传，后端 M02-SESSION-09 来源校验（Origin 主机 ≠ Host
// 主机且未命中 BBLBB__ALLOWED_ORIGINS）会 400 origin_not_allowed——
// 用户可见症状：成就墙「装备/卸下」等所有经 event.fetch 的 SSR 写请求
// 报「请求来源不被允许，请从本站发起操作」。
//
// 本测试锁定三点：
// 1. 重定向后的后端请求**不带** origin/referer（无论上游是否注入过）；
// 2. 发送必须走全局 fetch——SvelteKit 注入的 kitFetch 会重新注入 origin；
// 3. Cookie / X-Request-ID / 方法 / 业务头照常透传；非 /api 路径行为不变。
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { HandleFetch } from '@sveltejs/kit';
import { handleFetch } from '../../hooks.server';

type HandleFetchInput = Parameters<HandleFetch>[0];

const INTERNAL_ORIGIN = 'http://127.0.0.1:8080';

function makeEvent(input: {
  browserUrl?: string;
  browserHeaders?: Record<string, string>;
  internalUrl: string;
  internalInit?: { method?: string; headers?: Record<string, string> };
}) {
  const browserRequest = new Request(input.browserUrl ?? 'http://localhost:5173/achievements', {
    headers: input.browserHeaders ?? {}
  });
  const internalRequest = new Request(input.internalUrl, input.internalInit ?? {});
  // kitFetch：若被调用即测试失败目标（通过返回哨兵状态检测）
  const kitFetch = vi.fn(async (_input: RequestInfo | URL) => new Response(null, { status: 299 }));
  const hookInput = {
    event: { request: browserRequest, url: new URL(browserRequest.url) },
    request: internalRequest,
    fetch: kitFetch
  } as unknown as HandleFetchInput;
  return { kitFetch, hookInput, internalRequest };
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('hooks.server handleFetch：/api 内部重定向剥离浏览器来源', () => {
  it('PUT 写请求 → 重定向到 INTERNAL_API_ORIGIN 且不带 origin/referer（核心回归）', async () => {
    const fetchStub = vi.fn(async (_input: RequestInfo | URL) => new Response('{"equipped":true}', { status: 200 }));
    vi.stubGlobal('fetch', fetchStub);
    const { kitFetch, hookInput } = makeEvent({
      browserHeaders: { cookie: '__Host-bblbb_session=s1; __Host-bblbb_csrf=c1' },
      internalUrl: 'http://localhost:5173/api/v1/me/achievements/community_elder/equip',
      internalInit: {
        method: 'PUT',
        headers: { 'x-csrf-token': 'tok-1', 'content-type': 'application/json' }
      }
    });

    const response = await handleFetch(hookInput);

    expect(response.status).toBe(200);
    expect(kitFetch).not.toHaveBeenCalled();
    expect(fetchStub).toHaveBeenCalledTimes(1);
    const sent = fetchStub.mock.calls[0][0] as Request;
    expect(sent.url).toBe(`${INTERNAL_ORIGIN}/api/v1/me/achievements/community_elder/equip`);
    expect(sent.method).toBe('PUT');
    expect(sent.headers.get('origin')).toBeNull();
    expect(sent.headers.get('referer')).toBeNull();
    // 业务与凭证头照常透传
    expect(sent.headers.get('x-csrf-token')).toBe('tok-1');
    expect(sent.headers.get('cookie')).toBe('__Host-bblbb_session=s1; __Host-bblbb_csrf=c1');
  });

  it('上游已注入 origin（SvelteKit 合成行为）时也必须剥离', async () => {
    const fetchStub = vi.fn(async (_input: RequestInfo | URL) => new Response(null, { status: 200 }));
    vi.stubGlobal('fetch', fetchStub);
    const { hookInput } = makeEvent({
      internalUrl: 'http://localhost:5173/api/v1/me/achievements/x/equip',
      internalInit: {
        method: 'POST',
        headers: { origin: 'http://localhost:5173', referer: 'http://localhost:5173/achievements' }
      }
    });

    await handleFetch(hookInput);

    const sent = fetchStub.mock.calls[0][0] as Request;
    expect(sent.headers.get('origin')).toBeNull();
    expect(sent.headers.get('referer')).toBeNull();
  });

  it('GET 请求同样重定向且无 origin（SvelteKit 对 GET 本就不发 origin）', async () => {
    const fetchStub = vi.fn(async (_input: RequestInfo | URL) => new Response('[]', { status: 200 }));
    vi.stubGlobal('fetch', fetchStub);
    const { kitFetch, hookInput } = makeEvent({
      internalUrl: 'http://localhost:5173/api/v1/me/achievements'
    });

    const response = await handleFetch(hookInput);

    expect(response.status).toBe(200);
    expect(kitFetch).not.toHaveBeenCalled();
    const sent = fetchStub.mock.calls[0][0] as Request;
    expect(sent.url).toBe(`${INTERNAL_ORIGIN}/api/v1/me/achievements`);
    expect(sent.method).toBe('GET');
    expect(sent.headers.get('origin')).toBeNull();
  });

  it('X-Request-ID 从浏览器请求透传到后端', async () => {
    const fetchStub = vi.fn(async (_input: RequestInfo | URL) => new Response(null, { status: 200 }));
    vi.stubGlobal('fetch', fetchStub);
    const { hookInput } = makeEvent({
      browserHeaders: { 'x-request-id': 'req-42' },
      internalUrl: 'http://localhost:5173/api/v1/me/achievements/x/equip',
      internalInit: { method: 'DELETE' }
    });

    await handleFetch(hookInput);

    const sent = fetchStub.mock.calls[0][0] as Request;
    expect(sent.headers.get('x-request-id')).toBe('req-42');
  });

  it('非 /api、/healthz、/readyz 路径不重定向，仍走 SvelteKit kitFetch', async () => {
    const fetchStub = vi.fn(async (_input: RequestInfo | URL) => new Response(null, { status: 200 }));
    vi.stubGlobal('fetch', fetchStub);
    const { kitFetch, hookInput, internalRequest } = makeEvent({
      internalUrl: 'http://localhost:5173/settings'
    });

    await handleFetch(hookInput);

    expect(kitFetch).toHaveBeenCalledTimes(1);
    expect(kitFetch.mock.calls[0][0]).toBe(internalRequest);
    expect(fetchStub).not.toHaveBeenCalled();
  });

  it('/apikeys（前端路由，非后端 /api/ 前缀）不被误代理', async () => {
    const fetchStub = vi.fn(async (_input: RequestInfo | URL) => new Response(null, { status: 200 }));
    vi.stubGlobal('fetch', fetchStub);
    const { kitFetch, hookInput } = makeEvent({
      internalUrl: 'http://localhost:5173/apikeys'
    });

    await handleFetch(hookInput);

    expect(kitFetch).toHaveBeenCalledTimes(1);
    expect(fetchStub).not.toHaveBeenCalled();
  });
});
