/**
 * 编辑资料：PATCH /api/v1/me。
 * 需要 If-Match 头 = 当前 me.version（乐观并发，M03-PROFILE-04）；
 * 冲突（409 version_conflict）时提示刷新重试。
 */

const app = getApp();
const api = require('../../utils/api');
const store = require('../../utils/store');

Page({
  data: {
    display_name: '',
    bio: '',
    signature: '',
    version: 0,
    submitting: false,
  },

  onShow() {
    const me = store.getMe();
    if (!me) {
      app.requireLogin(this);
      return;
    }
    this.setData({
      display_name: me.display_name || '',
      bio: me.bio || '',
      signature: me.signature || '',
      version: me.version || 0,
    });
  },

  onInput(e) {
    this.setData({ [e.currentTarget.dataset.field]: e.detail.value });
  },

  async onSave() {
    if (this.data.submitting) return;
    const me = store.getMe();
    const body = {};
    if (this.data.display_name !== (me.display_name || '')) {
      body.display_name = this.data.display_name.trim() || null;
    }
    if (this.data.bio !== (me.bio || '')) body.bio = this.data.bio.trim();
    if (this.data.signature !== (me.signature || '')) {
      body.signature = this.data.signature.trim() || null;
    }
    if (!Object.keys(body).length) {
      wx.showToast({ title: '没有改动', icon: 'none' });
      return;
    }
    this.setData({ submitting: true });
    try {
      const fresh = await api.updateMe(body, this.data.version);
      store.setMe(fresh);
      app.globalData.me = fresh;
      wx.showToast({ title: '已保存', icon: 'success' });
      setTimeout(() => wx.navigateBack(), 500);
    } catch (err) {
      if (err.status === 409 || err.code === 'version_conflict') {
        wx.showModal({
          title: '版本冲突',
          content: '资料版本已变化，请返回后重试。',
          showCancel: false,
        });
      } else {
        app.showError(err, '保存失败');
      }
    } finally {
      this.setData({ submitting: false });
    }
  },
});
