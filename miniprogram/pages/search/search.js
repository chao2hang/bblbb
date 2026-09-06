/**
 * 搜索页（GET /api/v1/search?q=&after=&limit=）。
 *
 * 结果条目：{id, type: post|user|board|tag, title, url, excerpt, highlight?}
 * - post → 帖子详情（按 id）
 * - user → 用户主页（url 中取 username）
 * - board → 板块详情（url 中取 slug）
 * - tag → 小程序暂无标签页，展示提示
 */

const api = require('../../utils/api');
const config = require('../../config');

const DEBOUNCE_MS = 600;

Page({
  data: {
    keyword: '',
    items: [],
    state: 'idle', // idle | loading | ready | error
    error: '',
    nextCursor: null,
    hasMore: false,
    loadingMore: false,
  },

  onLoad() {
    // 自动聚焦
    this.setData({ focus: true });
  },

  onInput(e) {
    const keyword = e.detail.value;
    this.setData({ keyword });
    if (this._timer) clearTimeout(this._timer);
    if (!keyword.trim()) {
      this.setData({ items: [], state: 'idle' });
      return;
    }
    this._timer = setTimeout(() => this.runSearch(true), DEBOUNCE_MS);
  },

  onConfirm() {
    if (this._timer) clearTimeout(this._timer);
    if (this.data.keyword.trim()) this.runSearch(true);
  },

  onClear() {
    if (this._timer) clearTimeout(this._timer);
    this.setData({ keyword: '', items: [], state: 'idle' });
  },

  onReachBottom() {
    if (this.data.hasMore && this.data.state === 'ready') this.runSearch(false);
  },

  async runSearch(reset) {
    const q = this.data.keyword.trim();
    if (!q) return;
    if (this.data.loadingMore) return;
    this.setData(reset ? { state: 'loading', items: [], nextCursor: null, error: '' } : { loadingMore: true });
    try {
      const res = await api.search(q, {
        limit: config.PAGE_SIZE,
        after: this.data.nextCursor,
      });
      const raw = (res && res.items) || [];
      const items = raw.map((r) => this._decorate(r));
      const page = (res && res.page) || {};
      const nextCursor = page.next_cursor || null;
      const hasMore = !!page.has_more && !!nextCursor;
      this.setData({
        items: reset ? items : this.data.items.concat(items),
        nextCursor,
        hasMore,
        state: 'ready',
        loadingMore: false,
      });
    } catch (err) {
      this.setData({
        state: reset ? 'error' : 'ready',
        error: err.message || '搜索失败',
        loadingMore: false,
      });
    }
  },

  _decorate(r) {
    const view = Object.assign({}, r);
    const type = r.type || '';
    const url = r.url || '';
    view.typeLabel = { post: '帖子', user: '用户', board: '板块', tag: '标签' }[type] || type;
    view.highlightHtml = r.highlight
      ? r.highlight.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
      : '';
    if (type === 'post') view.nav = `/pages/post/post?id=${r.id}`;
    else if (type === 'user') {
      const m = url.match(/\/users\/([^/?#]+)/);
      if (m) view.nav = `/pages/user/user?username=${encodeURIComponent(m[1])}`;
    } else if (type === 'board') {
      const m = url.match(/\/boards\/([^/?#]+)/);
      if (m) view.nav = `/pages/board/board?slug=${encodeURIComponent(m[1])}`;
    }
    return view;
  },

  onItemTap(e) {
    const item = e.currentTarget.dataset.item;
    if (item.nav) {
      wx.navigateTo({ url: item.nav });
    } else if (item.type === 'tag') {
      wx.showToast({ title: '标签浏览请使用 Web 端', icon: 'none' });
    }
  },

  onRetry() {
    this.runSearch(true);
  },

  onLoadMoreRetry() {
    this.runSearch(false);
  },
});
