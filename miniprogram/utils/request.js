/**
 * BBLBB API 客户端（wx.request 封装）
 *
 * 对接 BBLBB 后端（Rust/axum）的三项关键机制：
 *
 * 1. 会话 Cookie
 *    后端会话 cookie 为 `__Host-bblbb_session`（HttpOnly/Secure/SameSite=Lax）。
 *    wx.request 由运行时 cookie 引擎自动携带（默认路径）；config.MANUAL_COOKIES
 *    开启时改用内置 cookie jar 显式携带（兜底路径）。
 *
 * 2. CSRF（后端 M02-SESSION-07/08）
 *    - 预认证写路径（login / register / verify-email / resend-verification /
 *      password-reset 及其 confirm）：必须先 GET /api/v1/auth/csrf 取得
 *      「匿名预认证 CSRF 状态」（`__Host-bblbb_csrf` cookie + 派生 token），
 *      并在请求头携带 X-CSRF-Token；
 *    - 已登录写请求：携带会话绑定的 synchronizer token（同样通过
 *      GET /api/v1/auth/csrf 获取，同一会话稳定）；
 *    - 收到 403 csrf_failed 时自动重新获取 token 并重试一次。
 *
 * 3. 请求来源校验（M02-SESSION-09）
 *    微信 wx.request 会自动携带
 *    `Referer: https://servicewechat.com/{appid}/{page}/{version}`。
 *    后端据此做来源校验，因此服务端必须把 `https://servicewechat.com`
 *    加入 BBLBB__ALLOWED_ORIGINS（见 README 与 dev/start-backend.sh）。
 *
 * 错误模型：后端统一返回 RFC 7807 Problem JSON
 * （`{type,title,status,code,detail,instance,request_id,errors?}`），
 * 本模块归一化为 ApiError 并附带面向用户的中文提示。
 */

const config = require('../config');
const store = require('./store');
const cookieJar = require('./cookie-jar');

/** 后端强制预认证 CSRF 的写路径（与 backend/src/middleware/csrf.rs 一一对应） */
const PREAUTH_WRITE_PATHS = [
  '/api/v1/auth/login',
  '/api/v1/auth/login/mfa',
  '/api/v1/auth/register',
  '/api/v1/auth/verify-email',
  '/api/v1/auth/resend-verification',
  '/api/v1/auth/password-reset',
  '/api/v1/auth/password-reset/confirm',
];

/** 稳定 Problem code → 面向用户的中文提示 */
const CODE_MESSAGES = {
  invalid_credentials: '账号或密码错误',
  mfa_required: '需要两步验证',
  invalid_mfa: '两步验证码错误',
  rate_limited: '操作过于频繁，请稍后再试',
  account_locked: '账号已临时锁定，请稍后再试',
  csrf_failed: '安全令牌失效，请重试',
  origin_not_allowed:
    '服务器未放行小程序来源（BBLBB__ALLOWED_ORIGINS 需包含 https://servicewechat.com）',
  host_not_allowed: '服务器拒绝了当前请求主机（BBLBB__ALLOWED_HOSTS 配置）',
  feature_disabled: '该功能服务端暂未开启',
  email_verification_required: '请先完成邮箱验证',
  email_unverified: '邮箱未验证，暂不能进行该操作',
  in_cooldown: '账号处于新号冷静期，请稍后再试',
  account_not_allowed: '当前账号状态不可用',
  muted: '账号处于禁言期',
  post_not_found: '帖子不存在或已删除',
  not_found: '请求的内容不存在',
  version_conflict: '内容已被更新，请刷新后重试',
  invalid_request: '请求参数有误',
};

class ApiError extends Error {
  constructor(opts) {
    super(opts.message || '请求失败');
    this.name = 'ApiError';
    this.status = opts.status; // HTTP 状态码
    this.code = opts.code; // 稳定 Problem code
    this.detail = opts.detail; // 服务端原始 detail
    this.requestId = opts.requestId;
    this.retryAfterSecs = opts.retryAfterSecs || null;
    this.isNetwork = !!opts.isNetwork;
  }
}

function friendlyMessage(err, fallback) {
  if (err.isNetwork) return '网络连接失败，请检查网络后重试';
  if (err.code && CODE_MESSAGES[err.code]) return CODE_MESSAGES[err.code];
  if (err.status === 429) {
    const wait = err.retryAfterSecs ? `（约 ${err.retryAfterSecs} 秒后可重试）` : '';
    return `操作过于频繁${wait}`;
  }
  if (err.status === 401) return '登录已失效，请重新登录';
  if (err.status === 403) return '没有权限执行该操作';
  if (err.status === 404) return '请求的内容不存在';
  if (err.status >= 500) return '服务暂时不可用，请稍后重试';
  return err.detail || fallback || '请求失败，请稍后重试';
}
ApiError.friendly = friendlyMessage;

/** 将后端 Problem JSON 归一化为 ApiError */
function toApiError(res) {
  const body = res.data || {};
  const headers = res.header || {};
  const retryAfter = headers['retry-after']
    ? parseInt(headers['retry-after'], 10)
    : body.retry_after
      ? parseInt(body.retry_after, 10)
      : null;
  const err = new ApiError({
    status: res.statusCode,
    code: body.code,
    detail: body.detail,
    requestId: body.request_id || headers['x-request-id'] || null,
    retryAfterSecs: Number.isFinite(retryAfter) ? retryAfter : null,
  });
  err.message = friendlyMessage(err);
  return err;
}

function networkError(desc) {
  const err = new ApiError({ status: 0, code: 'network', isNetwork: true });
  err.message = desc || err.message;
  return err;
}

/** 拼接 query string（跳过空值） */
function buildQuery(query) {
  if (!query) return '';
  const parts = [];
  Object.keys(query).forEach((k) => {
    const v = query[k];
    if (v === undefined || v === null || v === '') return;
    parts.push(`${encodeURIComponent(k)}=${encodeURIComponent(v)}`);
  });
  return parts.length ? `?${parts.join('&')}` : '';
}

/**
 * 从 Set-Cookie 头数组更新手动 cookie jar（仅 MANUAL_COOKIES 时消费）。
 * wx.request 响应头键小写；多值头为数组。
 */
function harvestCookies(url, headers) {
  if (!config.MANUAL_COOKIES || !headers) return;
  const setCookie = headers['set-cookie'];
  if (!setCookie) return;
  const list = Array.isArray(setCookie) ? setCookie : [setCookie];
  list.forEach((raw) => cookieJar.applySetCookie(url, String(raw)));
}

function manualCookieHeader(url) {
  if (!config.MANUAL_COOKIES) return null;
  return cookieJar.cookieForUrl(url) || null;
}

/**
 * 获取 CSRF token。
 *
 * @param {object} opts
 * @param {boolean} opts.preauth 是否强制使用「匿名预认证」token
 *   （登录/注册等未持会话 cookie 的写请求必须用预认证 token）
 * @param {boolean} opts.refresh 忽略缓存重新获取
 */
let csrfPromise = null;
function fetchCsrf(opts = {}) {
  const forceRefresh = !!opts.refresh;
  const cacheKey = opts.preauth ? store.KEYS.csrfPreauth : store.KEYS.csrfSession;
  // 未刷新时按「是否预认证」复用缓存
  if (!forceRefresh) {
    const cached = store.get(cacheKey);
    if (cached) return Promise.resolve(cached);
  }
  if (csrfPromise) return csrfPromise;

  const url = `${config.API_BASE}/api/v1/auth/csrf`;
  csrfPromise = new Promise((resolve, reject) => {
    const header = { Accept: 'application/json' };
    const manual = manualCookieHeader(url);
    if (manual) header.Cookie = manual;
    wx.request({
      url,
      method: 'GET',
      header,
      timeout: config.TIMEOUT_MS,
      success: (res) => {
        harvestCookies(url, res.header);
        if (res.statusCode >= 200 && res.statusCode < 300 && res.data && res.data.token) {
          const token = res.data.token;
          // GET /auth/csrf：持会话 cookie 时返回会话绑定 token，
          // 无会话时签发匿名预认证状态。按上下文分别缓存，避免串用。
          store.set(cacheKey, token);
          resolve(token);
        } else {
          csrfPromise = null;
          reject(toApiError(res));
        }
      },
      fail: (e) => {
        csrfPromise = null;
        reject(networkError(e.errMsg));
      },
    });
  });
  return csrfPromise;
}

/**
 * 确保当前写请求可用的 X-CSRF-Token。
 *
 * 规则（与后端 csrf 中间件对齐）：
 * - 预认证写路径 → 预认证 token；
 * - 其余写请求：
 *   - 已登录（本地存有 me）→ 会话 token；
 *   - 未登录且不在预认证路径 → 后端宽松放行（无需 token），返回 null。
 */
async function ensureCsrfToken(method, apiPath, forceRefresh) {
  if (!/^(POST|PUT|PATCH|DELETE)$/i.test(method)) return null;
  if (PREAUTH_WRITE_PATHS.some((p) => apiPath === p || apiPath.startsWith(p + '/'))) {
    return fetchCsrf({ preauth: true, refresh: forceRefresh });
  }
  if (store.getMe()) {
    // 登录会话：优先复用会话 token；被 403 拒绝后由调用方 refresh 重试
    return fetchCsrf({ preauth: false, refresh: forceRefresh });
  }
  return null;
}

/** 登录态变化（401）统一清理 */
function handleUnauthorized() {
  store.clearSession();
}

/**
 * 核心请求。
 *
 * @param {object} opts
 * @param {string} opts.method  HTTP 方法
 * @param {string} opts.path    API 路径（含 /api/v1 前缀），如 /api/v1/posts
 * @param {object} [opts.query] GET 查询参数
 * @param {object} [opts.data]  请求体（JSON 序列化）
 * @param {object} [opts.header] 附加请求头
 * @param {boolean} [opts.timeout] 自定义超时
 * @returns {Promise<any>} 解析后的响应体（2xx）
 */
async function request(opts) {
  const { method = 'GET', path, query, data, header = {} } = opts;
  const isWrite = /^(POST|PUT|PATCH|DELETE)$/i.test(method);
  const url = `${config.API_BASE}${path}${buildQuery(query)}`;

  const finalHeader = Object.assign(
    { Accept: 'application/json', 'X-Client': config.CLIENT_TAG },
    header
  );
  if (data !== undefined && isWrite) {
    finalHeader['Content-Type'] = finalHeader['Content-Type'] || 'application/json';
  }

  // CSRF token（写请求）
  let csrfRetried = false;
  for (let attempt = 0; attempt < 2; attempt += 1) {
    const token = await ensureCsrfToken(method, path, attempt > 0);
    if (token) finalHeader['X-CSRF-Token'] = token;

    const cookieHeader = manualCookieHeader(url);
    if (cookieHeader) finalHeader.Cookie = cookieHeader;

    const result = await new Promise((resolve, reject) => {
      wx.request({
        url,
        method: method.toUpperCase(),
        data: isWrite && data !== undefined ? JSON.stringify(data) : query,
        header: finalHeader,
        timeout: opts.timeout || config.TIMEOUT_MS,
        success: resolve,
        fail: (e) => reject(networkError(e.errMsg)),
      });
    }).catch((e) => {
      if (e instanceof ApiError) throw e;
      throw networkError(e && e.errMsg);
    });

    harvestCookies(url, result.header);

    const status = result.statusCode;
    if (status >= 200 && status < 300) {
      return result.data === undefined || result.data === '' ? null : result.data;
    }

    const err = toApiError(result);

    // CSRF 失效：重新取 token 重试一次（M02-SESSION-07/08）
    if (status === 403 && err.code === 'csrf_failed' && !csrfRetried) {
      csrfRetried = true;
      store.remove(store.KEYS.csrfSession);
      store.remove(store.KEYS.csrfPreauth);
      continue;
    }

    // 会话失效
    if (status === 401) {
      handleUnauthorized();
    }

    throw err;
  }
  /* istanbul ignore next */
  throw new ApiError({ status: 0, code: 'csrf_failed' });
}

/** GET */
function get(path, query, opts = {}) {
  return request({ method: 'GET', path, query, ...opts });
}

/** POST（自动处理 CSRF 与幂等 client_request_id） */
function post(path, data, opts = {}) {
  return request({ method: 'POST', path, data: data || {}, ...opts });
}

/** PATCH */
function patch(path, data, opts = {}) {
  return request({ method: 'PATCH', path, data: data || {}, ...opts });
}

/** DELETE */
function del(path, opts = {}) {
  return request({ method: 'DELETE', path, ...opts });
}

module.exports = {
  request,
  get,
  post,
  patch,
  del,
  ApiError,
  fetchCsrf,
  PREAUTH_WRITE_PATHS,
};
