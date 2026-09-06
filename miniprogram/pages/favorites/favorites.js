/**
 * 我的收藏：GET /api/v1/me/favorites（{items, page} 游标分页）。
 */

const app = getApp();
const api = require('../../utils/api');
const config = require('../../config');

Page({
  data: {
    items: [],
    state: 'loading',
    error: '',
    nextCursor: null,
    hasMore: false,
    loadingMore: false,
  },

  onShow() {
    if (!app.requireLogin(this)) return;
    this.load(true);
  },

  onReachBottom() {
    if (this.data.hasMore) this.load(false);
  },

  async load(reset) {
    if (this.data.loadingMore) return;
    if (reset) {
      this.setData({ state: 'loading', items: [], nextCursor: null, hasMore: false, error: '' });
    }
    this.setData({ loadingMore: true });
    try {
      const res = await api.listMyFavorites({
        limit: config.PAGE_SIZE,
        after: this.data.nextCursor,
      });
      const items = (res && res.items) || [];
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
        error: err.message || '加载失败',
        loadingMore: false,
      });
    }
  },

  onPostTap(e) {
    wx.navigateTo({ url: `/pages/post/post?id=${e.detail.id}` });
  },

  onRetry() {
    this.load(true);
  },

  onLoadMoreRetry() {
    this.load(false);
  },
});
