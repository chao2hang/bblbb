/**
 * 轻量全局状态存储（wx.storage 持久化 + 内存缓存 + 简单订阅）。
 *
 * 持久化键：
 * - bblbb.me           当前登录用户（Me 投影）
 * - bblbb.csrfSession  会话绑定 CSRF token
 * - bblbb.csrfPreauth  匿名预认证 CSRF token
 *
 * 注意：会话 cookie 本身由微信运行时管理（HttpOnly，JS 不可读），
 * 这里只缓存服务端派生的非敏感数据。
 */

const KEYS = {
  me: 'bblbb.me',
  csrfSession: 'bblbb.csrfSession',
  csrfPreauth: 'bblbb.csrfPreauth',
};

const listeners = {};

function emit(key) {
  (listeners[key] || []).forEach((fn) => {
    try {
      fn();
    } catch (e) {
      // 订阅者异常不影响其他订阅
    }
  });
}

/** 读取（内存优先，回退 storage） */
function get(key) {
  if (key in cache) return cache[key];
  try {
    const v = wx.getStorageSync(key);
    cache[key] = v === '' ? null : v;
  } catch (e) {
    cache[key] = null;
  }
  return cache[key] || null;
}

const cache = {};

function set(key, value) {
  cache[key] = value === null || value === undefined ? null : value;
  try {
    if (cache[key] === null) wx.removeStorageSync(key);
    else wx.setStorageSync(key, cache[key]);
  } catch (e) {
    // storage 写失败不阻断业务（内存仍有效）
  }
  emit(key);
}

function remove(key) {
  set(key, null);
}

function subscribe(key, fn) {
  (listeners[key] = listeners[key] || []).push(fn);
  return () => {
    listeners[key] = (listeners[key] || []).filter((f) => f !== fn);
  };
}

// ── 会话语义 ─────────────────────────────────────────────────────────────────

function getMe() {
  return get(KEYS.me);
}

function setMe(me) {
  const prev = get(KEYS.me);
  set(KEYS.me, me);
  // 会话变化（新登录/切换账号）：旧会话的 CSRF token 立即失效
  if (me && prev && prev.id !== me.id) remove(KEYS.csrfSession);
}

/** 清理登录态（401 / 主动登出时调用；cookie 由后端 204 响应清除） */
function clearSession() {
  remove(KEYS.me);
  remove(KEYS.csrfSession);
  remove(KEYS.csrfPreauth);
}

function isLoggedIn() {
  return !!getMe();
}

module.exports = {
  KEYS,
  get,
  set,
  remove,
  subscribe,
  getMe,
  setMe,
  clearSession,
  isLoggedIn,
};
