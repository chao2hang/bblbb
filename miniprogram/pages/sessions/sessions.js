/**
 * 设备与登录：GET /api/v1/auth/sessions（设备列表）、
 * DELETE /api/v1/auth/sessions/{id}（逐设备撤销）、
 * DELETE /api/v1/auth/sessions（全部登出）。
 */

const app = getApp();
const api = require('../../utils/api');
const format = require('../../utils/format');

Page({
  data: {
    sessions: [],
    state: 'loading',
    error: '',
    busyId: null,
  },

  onShow() {
    if (!app.requireLogin(this)) return;
    this.load();
  },

  async load() {
    this.setData({ state: 'loading' });
    try {
      const res = await api.listSessions();
      const list = (res || []).map((s) => ({
        id: s.id,
        ua: s.user_agent || '未知设备',
        createdText: format.fullTime(s.created_at),
        lastSeenText: format.timeAgo(s.last_seen_at),
        expiresText: format.fullTime(s.absolute_expires_at),
      }));
      this.setData({ sessions: list, state: 'ready' });
    } catch (err) {
      this.setData({ state: 'error', error: err.message || '加载失败' });
    }
  },

  onRevokeTap(e) {
    const id = e.currentTarget.dataset.id;
    this.setData({ busyId: id });
    api
      .revokeSession(id)
      .then(() => {
        wx.showToast({ title: '已撤销', icon: 'success' });
        this.load();
      })
      .catch((err) => app.showError(err, '撤销失败'))
      .finally(() => this.setData({ busyId: null }));
  },

  onRevokeAllTap() {
    wx.showModal({
      title: '全部登出',
      content: '撤销所有设备的登录会话（包括当前设备），确定？',
      success: async (r) => {
        if (!r.confirm) return;
        try {
          await api.logoutAll();
        } catch (e) {
          /* 忽略，继续清理本地 */
        }
        const { store } = require('../../utils/store');
        store.clearSession();
        app.globalData.me = null;
        wx.reLaunch({ url: '/pages/index/index' });
      },
    });
  },

  onRetry() {
    this.load();
  },
});
