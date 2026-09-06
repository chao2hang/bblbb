/**
 * 内置 Cookie Jar（config.MANUAL_COOKIES=true 时启用）。
 *
 * wx.request 正常情况下由运行时自动处理 cookie；本模块为无法自动携带
 * cookie 的环境提供兜底：解析 Set-Cookie（RFC 6265 子集）并按 origin 维护，
 * 请求时生成 Cookie 头。
 *
 * 持久化到 wx.storage（bblbb.cookieJar），App 重启后会话 cookie 仍在。
 */

const STORAGE_KEY = 'bblbb.cookieJar';

/** origin → { cookieName → {value, maxAgeMs|null, expiresMs|null, path, httpOnly, secure} } */
let jar = null;

function load() {
  if (jar) return;
  try {
    jar = wx.getStorageSync(STORAGE_KEY) || {};
  } catch (e) {
    jar = {};
  }
}

function save() {
  try {
    wx.setStorageSync(STORAGE_KEY, jar);
  } catch (e) {
    /* 忽略持久化失败，内存态仍有效 */
  }
}

function originOf(url) {
  // http(s)://host[:port]
  const m = String(url).match(/^[a-z]+:\/\/[^\/?#]+/i);
  return m ? m[0] : '';
}

function parseDate(s) {
  const t = Date.parse(s);
  return Number.isFinite(t) ? t : null;
}

/**
 * 解析一条 Set-Cookie 头并写入 jar。
 * 支持：name=value、Expires、Max-Age、Path、Domain、Secure、HttpOnly。
 * Max-Age=0 或已过期 → 删除。
 */
function applySetCookie(url, raw) {
  load();
  const origin = originOf(url);
  if (!origin) return;

  const parts = String(raw).split(';').map((s) => s.trim()).filter(Boolean);
  if (!parts.length) return;
  const eq = parts[0].indexOf('=');
  if (eq <= 0) return;
  const name = parts[0].slice(0, eq).trim();
  const value = parts[0].slice(eq + 1).trim();
  if (!name) return;

  const attrs = {};
  for (let i = 1; i < parts.length; i += 1) {
    const aeq = parts[i].indexOf('=');
    const k = (aeq >= 0 ? parts[i].slice(0, aeq) : parts[i]).trim().toLowerCase();
    const v = aeq >= 0 ? parts[i].slice(aeq + 1).trim() : '';
    attrs[k] = v;
  }

  let expiresMs = null;
  if (attrs['max-age'] !== undefined) {
    const secs = parseInt(attrs['max-age'], 10);
    expiresMs = Number.isFinite(secs) ? Date.now() + secs * 1000 : null;
  } else if (attrs.expires) {
    expiresMs = parseDate(attrs.expires);
  }

  if (expiresMs !== null && expiresMs <= Date.now()) {
    // 删除该 cookie
    if (jar[origin] && name in jar[origin]) {
      delete jar[origin][name];
      if (!Object.keys(jar[origin]).length) delete jar[origin];
      save();
    }
    return;
  }

  jar[origin] = jar[origin] || {};
  jar[origin][name] = {
    value,
    expiresMs,
    path: attrs.path || '/',
    httpOnly: attrs.httponly !== undefined,
    secure: attrs.secure !== undefined,
    domain: attrs.domain || '',
  };
  save();
}

/** 生成某 URL 应携带的 Cookie 头（无则返回空串） */
function cookieForUrl(url) {
  load();
  const origin = originOf(url);
  if (!origin) return '';
  const entry = jar[origin];
  if (!entry) return '';

  const m = url.match(/^[a-z]+:\/\/[^\/?#]+(\/[^?#]*)?/i);
  const path = m && m[2] ? m[2] : '/';
  const now = Date.now();
  const pairs = [];
  Object.keys(entry).forEach((name) => {
    const c = entry[name];
    if (c.expiresMs !== null && c.expiresMs <= now) return;
    if (c.path && !path.startsWith(c.path) && c.path !== '/') return;
    pairs.push(`${name}=${c.value}`);
  });
  return pairs.join('; ');
}

/** 清空全部（登出后本地兜底清理；后端同时下发清除 cookie） */
function clear() {
  jar = {};
  save();
}

module.exports = {
  applySetCookie,
  cookieForUrl,
  clear,
};
