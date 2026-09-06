/**
 * 忘记密码（两步）。
 *
 * 1. POST /auth/password-reset {email} → 统一 202（不泄漏邮箱是否注册）；
 *    服务器配置 SMTP 后会收到含一次性 token 的邮件（token 30 分钟有效）。
 * 2. POST /auth/password-reset/confirm {token, password}：
 *    原子消费 token → 更新密码 → 撤销该用户全部会话。
 */

const api = require('../../utils/api');

Page({
  data: {
    step: 'request', // request | confirm
    email: '',
    token: '',
    password: '',
    confirm: '',
    submitting: false,
    error: '',
    requested: false,
  },

  onInput(e) {
    this.setData({ [e.currentTarget.dataset.field]: e.detail.value, error: '' });
  },

  onBack() {
    this.setData({ step: 'request', requested: false, error: '' });
  },

  async onRequest() {
    const email = this.data.email.trim();
    if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)) {
      this.setData({ error: '邮箱格式不正确' });
      return;
    }
    this.setData({ submitting: true, error: '' });
    try {
      await api.requestPasswordReset(email);
      this.setData({ requested: true, step: 'confirm' });
    } catch (err) {
      this.setData({ error: err.message || '请求失败' });
    } finally {
      this.setData({ submitting: false });
    }
  },

  async onConfirm() {
    const { token, password, confirm } = this.data;
    if (!token.trim()) {
      this.setData({ error: '请输入邮件中的重置 token' });
      return;
    }
    if (password.length < 8 || password.length > 256) {
      this.setData({ error: '密码需 8-256 位' });
      return;
    }
    if (password !== confirm) {
      this.setData({ error: '两次输入的密码不一致' });
      return;
    }
    this.setData({ submitting: true, error: '' });
    try {
      await api.confirmPasswordReset(token.trim(), password);
      wx.showModal({
        title: '密码已重置',
        content: '所有设备已退出登录，请使用新密码重新登录。',
        showCancel: false,
        success: () => {
          wx.redirectTo({ url: '/pages/login/login' });
        },
      });
    } catch (err) {
      this.setData({ error: err.message || '重置失败' });
    } finally {
      this.setData({ submitting: false });
    }
  },
});
