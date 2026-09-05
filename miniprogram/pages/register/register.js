/**
 * 注册页。
 *
 * 后端校验（M02-IDENTITY-03）：
 * - username 3-20 字符，仅字母/数字/_/-；
 * - password 8-128 字符，须同时含字母与数字；
 * - email 基础格式校验；
 * 注册成功 201 {ok:true}；用户为 pending 状态，登录后可见；
 * 邮箱验证邮件仅在服务器配置了 SMTP 时送达（见 README「本地开发」）。
 * 唯一性冲突与成功返回相同响应（不泄漏账号是否存在）。
 */

const app = getApp();
const api = require('../../utils/api');

Page({
  data: {
    username: '',
    email: '',
    password: '',
    confirm: '',
    submitting: false,
    error: '',
  },

  onInput(e) {
    this.setData({ [e.currentTarget.dataset.field]: e.detail.value, error: '' });
  },

  validate() {
    const { username, email, password, confirm } = this.data;
    const u = username.trim();
    if (!/^[A-Za-z0-9_-]{3,20}$/.test(u)) {
      return '用户名需 3-20 位字母/数字/_/-';
    }
    if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email.trim())) {
      return '邮箱格式不正确';
    }
    if (password.length < 8 || password.length > 128) {
      return '密码需 8-128 位';
    }
    if (!/[A-Za-z]/.test(password) || !/\d/.test(password)) {
      return '密码需同时包含字母和数字';
    }
    if (password !== confirm) {
      return '两次输入的密码不一致';
    }
    return '';
  },

  async onSubmit() {
    if (this.data.submitting) return;
    const error = this.validate();
    if (error) {
      this.setData({ error });
      return;
    }
    this.setData({ submitting: true, error: '' });
    try {
      await api.register(this.data.username.trim(), this.data.password, this.data.email.trim());
      wx.showModal({
        title: '注册成功',
        content: '账号已创建。若服务器配置了邮件服务，请按验证邮件完成邮箱验证后即可发帖评论。现在去登录吧。',
        showCancel: false,
        confirmText: '去登录',
        success: () => {
          wx.redirectTo({ url: '/pages/login/login' });
        },
      });
    } catch (err) {
      this.setData({ error: err.message || '注册失败' });
    } finally {
      this.setData({ submitting: false });
    }
  },

  goLogin() {
    wx.redirectTo({ url: '/pages/login/login' });
  },
});
