/**
 * 板块详情页：板块信息 + 板块帖子流（最新/热门）。
 */

const app = getApp();
const api = require('../../utils/api');
const config = require('../../config');

Page({
  data: {
    slug: '',
    board: null,
    sort: 'latest', // latest | hot
    items: [],
    loadState: 'loading',
    loadError: '',
    nextCursor: null,
    hasMore: false,
    loadingMore: false,
  },

  onLoad(options) {
    const slug = options.slug || '';
    this.setData({ slug });
    if (!slug) {
      wx.showToast({ title: '板块不存在', icon: 'none' });
      setTimeout(() => wx.navigateBack(), 800);
      return;
    }
    wx.setNavigationBarTitle({ title: '加载中…' });
    this.loadBoard();
    this.loadList(true);
  },

  onPullDownRefresh() {
    this.loadList(true).finally(() => wx.stopPullDownRefresh());
  },

  onReachBottom() {
    this.loadList(false);
  },

  async loadBoard() {
    try {
      const board = await api.getBoard(this.data.slug);
      if (board && board.name) {
        this.setData({ board });
        wx.setNavigationBarTitle({ title: board.name });
      }
    } catch (e) {
      // 板块详情失败不阻塞帖子列表
    }
  },

  async loadList(reset) {
    if (this.data.loadingMore) return;
    if (reset) {
      this.setData({
        loadState: 'loading',
        items: [],
        nextCursor: null,
        hasMore: false,
        loadError: '',
      });
    }
    this.setData({ loadingMore: true });
    try {
      const res = await api.listBoardPosts(this.data.slug, {
        limit: config.PAGE_SIZE,
        after: this.data.nextCursor,
        sort: this.data.sort,
      });
      const items = (res && res.items) || [];
      const page = (res && res.page) || {};
      const nextCursor = page.next_cursor || null;
      const hasMore = !!page.has_more && !!nextCursor;
      this.setData({
        items: reset ? items : this.data.items.concat(items),
        nextCursor,
        hasMore,
        loadState: 'ready',
        loadingMore: false,
      });
    } catch (err) {
      this.setData({
        loadState: reset ? 'error' : 'ready',
        loadError: err.message || '加载失败',
        loadingMore: false,
      });
    }
  },

  onSortTap(e) {
    const sort = e.currentTarget.dataset.sort;
    if (sort === this.data.sort) return;
    this.setData({ sort });
    this.loadList(true);
  },

  onPostTap(e) {
    wx.navigateTo({ url: `/pages/post/post?id=${e.detail.id}` });
  },

  onRetry() {
    this.loadList(true);
  },

  onLoadMoreRetry() {
    this.loadList(false);
  },

  onGotoCreate() {
    const app = getApp();
    if (!app.requireLogin(this)) return;
    wx.navigateTo({ url: `/pages/create/create?board=${encodeURIComponent(this.data.slug)}` });
  },
});
