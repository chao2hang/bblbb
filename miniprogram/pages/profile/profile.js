/**
 * 我的（tab）。
 *
 * 数据：
 * - Me（本地缓存 + refreshMe 校验）
 * - GET /api/v1/activity/summary  今日签到/连续天数/等级/经验
 * - POST /api/v1/activity/visit   签到（每日首次有效业务页面访问）
 * - GET /api/v1/users/{username}  发帖数/关注数统计
 */

const app = getApp();
const api = require('../../utils/api');
const store = require('../../utils/store');

Page({
  data: {
    me: null,
    levelName: '',
    expBalance: 0,
    checkedInToday: false,
    streakDays: 0,
    stats: { post_count: 0, followers: 0, following: 0 },
    checkingIn: false,
    loggingOut: false,
  },

  onShow() {
    this.refresh();
  },

  async refresh() {
    const me = store.getMe();
    this.setData({ me });
    if (!me) return;

    // 并行：活动汇总 + 公开资料统计
    const jobs = [];
    jobs.push(
      api
        .activitySummary()
        .then((s) => {
          this.setData({
            levelName: (s && s.level && s.level.name) || `Lv${me.level}`,
            expBalance: (s && s.experience && s.experience.balance) || 0,
            checkedInToday: !!(s && s.checked_in_today),
            streakDays: (s && s.streak_days) || 0,
          });
        })
        .catch(() => {})
    );
    jobs.push(
      api
        .publicUser(me.username)
        .then((u) => {
          if (u) {
            this.setData({
              stats: {
                post_count: u.post_count || 0,
                followers: u.followers || 0,
                following: u.following || 0,
              },
            });
          }
        })
        .catch(() => {})
    );
    await Promise.all(jobs);
  },

  onLoginTap() {
    wx.navigateTo({ url: '/pages/login/login' });
  },

  /** 签到：记录一次有效业务页面访问（/me 属业务页面） */
  async onCheckIn() {
    if (this.data.checkingIn || this.data.checkedInToday) return;
    this.setData({ checkingIn: true });
    try {
      const res = await api.recordVisit('/me');
      const earned = (res && res.today_earned || [])
        .map((r) => `${r.amount}${r.currency === 'exp' ? ' 经验' : ` ${r.currency}`}`)
        .join(' + ');
      this.setData({ checkedInToday: true, streakDays: (res && res.streak_days) || 0 });
      wx.showToast({
        title: earned ? `签到成功 ${earned}（连续 ${this.data.streakDays} 天）` : '签到成功',
        icon: 'none',
        duration: 2400,
      });
    } catch (err) {
      app.showError(err, '签到失败');
    } finally {
      this.setData({ checkingIn: false });
    }
  },

  goEdit() {
    if (!app.requireLogin(this)) return;
    wx.navigateTo({ url: '/pages/profile-edit/profile-edit' });
  },

  goFavorites() {
    if (!app.requireLogin(this)) return;
    wx.navigateTo({ url: '/pages/favorites/favorites' });
  },

  goAchievements() {
    if (!app.requireLogin(this)) return;
    wx.navigateTo({ url: '/pages/achievements/achievements' });
  },

  goTransactions() {
    if (!app.requireLogin(this)) return;
    wx.navigateTo({ url: '/pages/transactions/transactions' });
  },

  goShop() {
    if (!app.requireLogin(this)) return;
    wx.navigateTo({ url: '/pages/shop/shop' });
  },

  goSessions() {
    if (!app.requireLogin(this)) return;
    wx.navigateTo({ url: '/pages/sessions/sessions' });
  },

  onLogoutTap() {
    if (!app.requireLogin(this)) return;
    wx.showModal({
      title: '退出登录',
      content: '确定退出当前账号吗？',
      success: async (r) => {
        if (!r.confirm) return;
        this.setData({ loggingOut: true });
        try {
          await api.logout();
        } catch (e) {
          // 即使服务端撤销失败也清理本地
        }
        store.clearSession();
        app.globalData.me = null;
        this.setData({ me: null, levelName: '', expBalance: 0, checkedInToday: false, streakDays: 0 });
        wx.showToast({ title: '已退出', icon: 'success' });
      },
    });
  },
});
