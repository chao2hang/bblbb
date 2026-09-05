/**
 * BBLBB 小程序入口。
 *
 * 启动流程：
 * 1. 恢复本地 Me 缓存（会话 cookie 由微信运行时维护）；
 * 2. 若存在本地登录态，静默校验（GET /me）——失败则清理会话；
 * 3. 页面通过 App().requireLogin() 做「必须登录」守卫。
 */

const api = require('./utils/api');
const store = require('./utils/store');

App({
  globalData: {
    me: null,
    sessionChecked: false,
    version: '1.0.0',
  },

  onLaunch() {
    const me = store.getMe();
    this.globalData.me = me;
    if (me) {
      // 静默校验；不阻塞首屏
      api.refreshMe().then((fresh) => {
        this.globalData.me = fresh || null;
      });
    }
    this.globalData.sessionChecked = true;
  },

  /**
   * 登录守卫：未登录时跳转登录页（带回跳参数）。
   * @param {object} page 调用方 Page 实例
   * @param {string} [redirectTo] 登录成功后回跳路径（含参数）
   * @returns {boolean} 已登录返回 true；已跳转返回 false
   */
  requireLogin(page, redirectTo) {
    const me = store.getMe();
    if (me) return true;
    const target = redirectTo
      ? redirectTo
      : page
        ? `${page.route}${this._queryOf(page)}`
        : '';
    const url = `/pages/login/login${target ? `?redirect=${encodeURIComponent(target)}` : ''}`;
    wx.redirectTo({ url, fail: () => wx.reLaunch({ url }) });
    return false;
  },

  _queryOf(page) {
    const opts = page && page.options ? page.options : {};
    const parts = Object.keys(opts)
      .map((k) => `${k}=${encodeURIComponent(opts[k])}`)
      .join('&');
    return parts ? `?${parts}` : '';
  },

  /** tab 页路径（redirectTo 不能跳转 tab 页，必须 switchTab） */
  TAB_PATHS: ['/pages/index/index', '/pages/create/create', '/pages/profile/profile'],

  /**
   * 登录成功后的统一回跳（登录页调用）。
   */
  afterLogin(page, redirectTo) {
    store.getMe(); // 确保最新
    if (redirectTo && redirectTo !== page.route) {
      const base = redirectTo.split('?')[0];
      if (this.TAB_PATHS.indexOf(base) >= 0) {
        wx.switchTab({ url: base });
        return;
      }
      wx.redirectTo({
        url: redirectTo,
        fail: () => wx.switchTab({ url: '/pages/index/index' }),
      });
      return;
    }
    wx.switchTab({ url: '/pages/index/index' });
  },

  /**
   * 统一错误 toast（ApiError → 友好中文）。
   */
  showError(err, fallback) {
    const msg =
      (err && (err.message || ApiError.friendly(err))) || fallback || '操作失败，请稍后重试';
    wx.showToast({ title: msg, icon: 'none', duration: 2600 });
  },
});

// ApiError 供 showError 使用
const ApiError = require('./utils/request').ApiError;
