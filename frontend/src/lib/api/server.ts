// M02-UX-01：SSR 服务端 API 代理（form action 专用）
//
// 前端 server 代码访问 Rust 后端的唯一入口，遵循 docs/FRONTEND.md：
// - 基址取 `INTERNAL_API_ORIGIN`（默认 http://127.0.0.1:8080）；
// - 写请求先 GET /api/v1/auth/csrf 取预认证 CSRF（cookie 与 header 配对，
//   M02-SESSION-08）；
// - 转发浏览器 Cookie（__Host-bblbb_csrf）与 X-Request-ID；
// - 后端返回的 Set-Cookie 必须显式逐属性复制到浏览器
//   （FRONTEND.md：若 form action 代理登录，必须显式复制所有属性）。

import { env } from '$env/dynamic/private';
import type { Cookies } from '@sveltejs/kit';
import { problemMessage, requestIdOf, type Problem } from '../errors';

/** 预认证 CSRF cookie 名（与后端 PREAUTH_COOKIE_NAME 一致）。 */
export const PREAUTH_COOKIE = '__Host-bblbb_csrf';

/** 重发冷却（秒）：与后端 ResendLimits::default().cooldown_ms 一致（60s）。 */
export const RESEND_COOLDOWN_SECS = 60;

const INTERNAL_API_ORIGIN: string = env.INTERNAL_API_ORIGIN ?? 'http://127.0.0.1:8080';

export interface RegisterViaServerInput {
  username: string;
  email: string;
  password: string;
}

export type RegisterServerResult =
  | { ok: true }
  | { ok: false; status: number; message: string; requestId: string | null };

interface ParsedCookie {
  name: string;
  value: string;
  path?: string;
  maxAge?: number;
  secure: boolean;
  httpOnly: boolean;
  sameSite?: 'lax' | 'strict' | 'none';
}

/** 解析单个 Set-Cookie 头为结构化 cookie（属性小写归一化）。 */
export function parseSetCookie(header: string): ParsedCookie | null {
  const parts = header.split(';');
  const first = parts.shift() ?? '';
  const eq = first.indexOf('=');
  if (eq <= 0) return null;
  const name = first.slice(0, eq).trim();
  const value = first.slice(eq + 1).trim();
  const cookie: ParsedCookie = { name, value, secure: false, httpOnly: false };
  for (const raw of parts) {
    const part = raw.trim();
    if (!part) continue;
    const [key, ...rest] = part.split('=');
    const k = key.trim().toLowerCase();
    const v = rest.join('=').trim();
    switch (k) {
      case 'path':
        cookie.path = v;
        break;
      case 'max-age': {
        const n = Number(v);
        if (Number.isFinite(n) && n >= 0) cookie.maxAge = n;
        break;
      }
      case 'secure':
        cookie.secure = true;
        break;
      case 'httponly':
        cookie.httpOnly = true;
        break;
      case 'samesite': {
        const s = v.toLowerCase();
        if (s === 'lax' || s === 'strict' || s === 'none') cookie.sameSite = s;
        break;
      }
    }
  }
  return cookie;
}

/**
 * 后端 Set-Cookie → cookies.set（逐属性复制）。
 * 必须通过 `cookies` API 写（SvelteKit 禁止直接设 Set-Cookie 头），
 * `__Host-` cookie 的 Secure/Path=/无 Domain 约束由解析出的属性保证。
 */
export function relaySetCookies(response: Response, cookies: Cookies): void {
  for (const header of response.headers.getSetCookie()) {
    const parsed = parseSetCookie(header);
    if (!parsed) continue;
    cookies.set(parsed.name, parsed.value, {
      path: parsed.path ?? '/',
      maxAge: parsed.maxAge,
      secure: parsed.secure,
      httpOnly: parsed.httpOnly,
      sameSite: parsed.sameSite
    });
  }
}

/** 从 Set-Cookie 响应头中取指定 cookie 的新值；无则返回 null。 */
export function cookieValueFromSetCookie(response: Response, name: string): string | null {
  for (const header of response.headers.getSetCookie()) {
    const parsed = parseSetCookie(header);
    if (parsed && parsed.name === name) return parsed.value;
  }
  return null;
}

interface CsrfState {
  token: string;
  cookieValue: string | null;
}

/** GET /api/v1/auth/csrf：复用浏览器已有预认证状态或签发新状态，并复制 Set-Cookie。 */
async function prepareCsrf(cookies: Cookies, requestId: string | null): Promise<CsrfState> {
  const browserCookie = cookies.get(PREAUTH_COOKIE) ?? null;
  const headers: Record<string, string> = { Accept: 'application/json' };
  if (browserCookie) headers.Cookie = `${PREAUTH_COOKIE}=${browserCookie}`;
  if (requestId) headers['X-Request-ID'] = requestId;

  const response = await fetch(`${INTERNAL_API_ORIGIN}/api/v1/auth/csrf`, { headers });
  relaySetCookies(response, cookies);
  if (!response.ok) {
    throw new Error(`CSRF token fetch failed (status ${response.status})`);
  }
  const data = (await response.json()) as { token: string };
  const newValue = cookieValueFromSetCookie(response, PREAUTH_COOKIE);
  return { token: data.token, cookieValue: newValue ?? browserCookie };
}

async function parseProblem(
  response: Response
): Promise<{ message: string; requestId: string | null }> {
  let problem: Problem | null = null;
  try {
    problem = (await response.json()) as Problem;
  } catch {
    problem = null;
  }
  return { message: problemMessage(problem), requestId: requestIdOf(problem) };
}

/** 预认证写操作失败结果（429 额外带 retryAfterSecs，供冷却倒计时）。 */
export interface ServerWriteFailure {
  ok: false;
  status: number;
  message: string;
  requestId: string | null;
  retryAfterSecs: number | null;
}

/** 通用预认证写操作：CSRF 配对 + Cookie/X-Request-ID 转发 + Set-Cookie 复制。 */
async function postWithCsrf(
  cookies: Cookies,
  path: string,
  body: unknown,
  requestId: string | null
): Promise<{ ok: true } | ServerWriteFailure> {
  const csrf = await prepareCsrf(cookies, requestId);
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    Accept: 'application/json',
    'X-CSRF-Token': csrf.token
  };
  if (csrf.cookieValue) headers.Cookie = `${PREAUTH_COOKIE}=${csrf.cookieValue}`;
  if (requestId) headers['X-Request-ID'] = requestId;

  const response = await fetch(`${INTERNAL_API_ORIGIN}${path}`, {
    method: 'POST',
    headers,
    body: JSON.stringify(body)
  });
  relaySetCookies(response, cookies);

  if (response.ok) return { ok: true };
  const { message, requestId: rid } = await parseProblem(response);
  const retryAfterHeader = response.headers.get('Retry-After');
  const retryAfterSecs =
    retryAfterHeader !== null && Number.isFinite(Number(retryAfterHeader))
      ? Number(retryAfterHeader)
      : null;
  return { ok: false, status: response.status, message, requestId: rid, retryAfterSecs };
}

/**
 * POST /api/v1/auth/register（M02-UX-01）。
 *
 * 后端对“用户名/邮箱已存在”返回与成功一致的 201 {ok:true}（防枚举，
 * M02-IDENTITY-05），因此冲突时同样返回 ok:true——统一账号冲突提示。
 */
export async function registerViaServer(
  cookies: Cookies,
  input: RegisterViaServerInput,
  requestId: string | null = null
): Promise<RegisterServerResult> {
  const result = await postWithCsrf(cookies, '/api/v1/auth/register', input, requestId);
  if (result.ok) return { ok: true };
  return { ok: false, status: result.status, message: result.message, requestId: result.requestId };
}

/** POST /api/v1/auth/verify-email（M02-UX-02）：token 一次性验证。 */
export async function verifyEmailViaServer(
  cookies: Cookies,
  token: string,
  requestId: string | null = null
): Promise<{ ok: true } | ServerWriteFailure> {
  return postWithCsrf(cookies, '/api/v1/auth/verify-email', { token }, requestId);
}

/**
 * POST /api/v1/auth/resend-verification（M02-UX-02）。
 *
 * 后端统一 202（邮箱不存在/已激活与正常重发一致，不泄漏）；冷却 60s /
 * 日 3 次命中返回 429 + Retry-After（秒），供前端冷却倒计时。
 */
export async function resendVerificationViaServer(
  cookies: Cookies,
  email: string,
  requestId: string | null = null
): Promise<{ ok: true } | ServerWriteFailure> {
  return postWithCsrf(cookies, '/api/v1/auth/resend-verification', { email }, requestId);
}
