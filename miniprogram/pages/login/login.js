/**
 * 登录页（含 MFA 两步验证）。
 *
 * 流程（与后端 /api/v1/auth/login 契约一致）：
 * 1. GET /auth/csrf 获取匿名预认证 CSRF 状态（request 层自动完成）；
 * 2. POST /auth/login {identifier, password, remember}
 *    - 200 + Me → 登录成功（会话 cookie 由运行时保存）；
 *    - 200 + {mfa_required, challenge_token} → 进入 MFA 第二步；
 *    - 401 invalid_credentials / 429 限流 → 展示提示；
 * 3. POST /auth/login/mfa {challenge_token, totp_code|recovery_code}。
 */

const app = getApp();
const api = require('../../utils/api');
const store = require('../../utils/store');

Page({
  data: {
    step: 'password', // password | mfa
    identifier: '',
    password: '',
    remember: true,
    mfaCode: '',
    usingRecovery: false,
    submitting: false,
    error: '',
    redirect: '',
  },

  onLoad(options) {
    this.setData({ redirect: options.redirect || '' });
  },

  onInput(e) {
    this.setData({ [e.currentTarget.dataset.field]: e.detail.value, error: '' });
  },

  onRemember(e) {
    this.setData({ remember: e.detail.value });
  },

  async onSubmit() {
    const { step, identifier, password, remember, mfaCode, usingRecovery } = this.data;
    if (this.data.submitting) return;

    if (step === 'password') {
      if (!identifier.trim() || !password) {
        this.setData({ error: '请输入账号和密码' });
        return;
      }
      this.setData({ submitting: true, error: '' });
      try {
        const res = await api.login(identifier.trim(), password, remember);
        this._handleLoginResult(res);
      } catch (err) {
        this.setData({ error: err.message || '登录失败' });
      } finally {
        this.setData({ submitting: false });
      }
      return;
    }

    // MFA 第二步
    if (!mfaCode.trim()) {
      this.setData({ error: '请输入验证码' });
      return;
    }
    this.setData({ submitting: true, error: '' });
    try {
      const res = await api.loginMfa(this._challenge, {
        totp_code: usingRecovery ? null : mfaCode.trim(),
        recovery_code: usingRecovery ? mfaCode.trim() : null,
      });
      this._handleLoginResult(res);
    } catch (err) {
      this.setData({ error: err.message || '验证失败' });
    } finally {
      this.setData({ submitting: false });
    }
  },

  _handleLoginResult(res) {
    if (res && res.mfa_required) {
      this._challenge = res.challenge_token;
      this.setData({ step: 'mfa', mfaCode: '', error: '' });
      return;
    }
    // 成功：res 为 Me 投影
    if (res && res.id) {
      store.setMe(res);
      app.globalData.me = res;
      wx.showToast({ title: '登录成功', icon: 'success' });
      setTimeout(() => app.afterLogin(this, this.data.redirect), 350);
    } else {
      this.setData({ error: '登录响应异常，请重试' });
    }
  },

  onSwitchMfa() {
    this.setData({ usingRecovery: !this.data.usingRecovery, mfaCode: '', error: '' });
  },

  onBackToPassword() {
    this.setData({ step: 'password', mfaCode: '', usingRecovery: false, error: '' });
  },

  goRegister() {
    wx.navigateTo({ url: '/pages/register/register' });
  },

  goForgot() {
    wx.navigateTo({ url: '/pages/forgot/forgot' });
  },
});
