/**
 * 发帖页（tab）。
 *
 * 提交契约（POST /api/v1/posts，M04-POSTS-01 服务端校验）：
 * - type: article（文章）| discussion（讨论）——注意不是列表筛选用的 topic；
 * - title 1-200；markdown 1-50000；board_id 必须为板块 UUID；
 * - access_policy: public | logged_in | after_reply | level | paid；
 *   paid 必须携带 price_coin（1-1000 金币），否则 422；
 * - tags ≤8 个、每个 1-32 字符（逗号/空格分隔输入，客户端拆分为数组）；
 * - client_request_id 幂等键（16-200 字符，uuid 自动生成，防重复发帖）。
 *
 * 前置条件：登录 + 邮箱已验证 + 冷静期已过（新号验证后 24h 内容写冷静期），
 * 否则后端返回 403，由错误提示呈现。
 */

const app = getApp();
const api = require('../../utils/api');
const config = require('../../config');

const TYPES = [
  { value: 'discussion', label: '讨论', hint: '发起话题、提问交流' },
  { value: 'article', label: '文章', hint: '长文、教程、分享' },
];

const POLICIES = [
  { value: 'public', label: '公开' },
  { value: 'logged_in', label: '仅登录' },
  { value: 'after_reply', label: '回复后可见' },
  { value: 'level', label: '等级可见' },
  { value: 'paid', label: '付费解锁' },
];

Page({
  data: {
    boards: [],
    boardIndex: -1, // 选中板块下标
    typeIndex: 0,
    policyIndex: 0,
    title: '',
    markdown: '',
    summary: '',
    tagsText: '',
    priceCoin: '',
    submitting: false,
    charCount: 0,
    types: TYPES,
    policies: POLICIES,
  },

  onLoad(options) {
    this.loadBoards(options.board || '');
  },

  async loadBoards(preselectSlug) {
    try {
      const res = await api.listBoards({ limit: 100 });
      const boards = (res && res.items) || [];
      let boardIndex = -1;
      if (preselectSlug) {
        const idx = boards.findIndex((b) => b.slug === preselectSlug);
        if (idx >= 0) boardIndex = idx;
      }
      this.setData({ boards, boardIndex });
    } catch (e) {
      wx.showToast({ title: '板块加载失败', icon: 'none' });
    }
  },

  onBoardChange(e) {
    this.setData({ boardIndex: Number(e.detail.value) });
  },

  onTypeChange(e) {
    this.setData({ typeIndex: Number(e.detail.value) });
  },

  onTypeTap(e) {
    this.setData({ typeIndex: Number(e.currentTarget.dataset.index) });
  },

  onPolicyChange(e) {
    this.setData({ policyIndex: Number(e.detail.value) });
  },

  onTitleInput(e) {
    this.setData({ title: e.detail.value });
  },

  onMarkdownInput(e) {
    const v = e.detail.value;
    this.setData({ markdown: v, charCount: v.length });
  },

  onSummaryInput(e) {
    this.setData({ summary: e.detail.value });
  },

  onTagsInput(e) {
    this.setData({ tagsText: e.detail.value });
  },

  onPriceInput(e) {
    this.setData({ priceCoin: e.detail.value });
  },

  parseTags() {
    const raw = this.data.tagsText;
    if (!raw.trim()) return [];
    return raw
      .split(/[,，\s]+/)
      .map((t) => t.trim())
      .filter(Boolean)
      .slice(0, 8)
      .map((t) => t.slice(0, 32));
  },

  validate() {
    if (this.data.boardIndex < 0) return '请选择板块';
    if (!this.data.title.trim()) return '请输入标题';
    if (this.data.title.trim().length > 200) return '标题不能超过 200 字符';
    const md = this.data.markdown.trim();
    if (!md) return '请输入正文';
    if (md.length > 50000) return '正文不能超过 50000 字符';
    const tags = this.parseTags();
    if (tags.length > 8) return '标签最多 8 个';
    const policy = POLICIES[this.data.policyIndex].value;
    let priceCoin = null;
    if (policy === 'paid') {
      priceCoin = parseInt(this.data.priceCoin, 10);
      if (!Number.isFinite(priceCoin) || priceCoin < 1 || priceCoin > 1000) {
        return '付费内容需设置 1-1000 金币';
      }
    }
    if (this.data.summary && this.data.summary.length > 300) return '摘要不能超过 300 字符';
    return '';
  },

  async onSubmit() {
    if (this.data.submitting) return;
    if (!app.requireLogin(this)) return;
    const error = this.validate();
    if (error) {
      wx.showToast({ title: error, icon: 'none' });
      return;
    }
    const board = this.data.boards[this.data.boardIndex];
    const policy = POLICIES[this.data.policyIndex].value;

    this.setData({ submitting: true });
    try {
      const res = await api.createPost({
        type: TYPES[this.data.typeIndex].value,
        title: this.data.title.trim(),
        markdown: this.data.markdown.trim(),
        board_id: board.id,
        access_policy: policy,
        price_coin: policy === 'paid' ? parseInt(this.data.priceCoin, 10) : undefined,
        summary: this.data.summary.trim() || undefined,
        tags: this.parseTags().length ? this.parseTags() : undefined,
      });
      // 本地版本跟踪（编辑 If-Match 用）：新帖 version = 1
      try {
        const key = 'bblbb.postVersions';
        const map = wx.getStorageSync(key) || {};
        if (res && res.id) map[res.id] = 1;
        wx.setStorageSync(key, map);
      } catch (e) {
        /* ignore */
      }
      wx.showToast({ title: '发布成功', icon: 'success' });
      setTimeout(() => {
        if (res && res.id) {
          wx.redirectTo({ url: `/pages/post/post?id=${res.id}` });
        } else {
          wx.switchTab({ url: '/pages/index/index' });
        }
      }, 500);
    } catch (err) {
      app.showError(err, '发布失败');
    } finally {
      this.setData({ submitting: false });
    }
  },

  // 分享
  onShareAppMessage() {
    return { title: '在 BBLBB 发布内容', path: '/pages/index/index' };
  },
});
