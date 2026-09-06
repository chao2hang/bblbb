/**
 * 首页（tab）：板块导航 + 最新/热门帖子流。
 *
 * 数据源：
 * - GET /api/v1/boards                      板块（顶部横滑导航）
 * - GET /api/v1/posts?sort=latest|popular   全站帖子流
 * - GET /api/v1/boards/{slug}/posts         选中板块后的帖子流
 */

const app = getApp();
const api = require('../../utils/api');
const config = require('../../config');

Page({
  data: {
    boards: [],
    activeBoard: null, // {slug,name} 或 null（全站）
    sortTab: 'latest', // latest | popular
    items: [],
    loadState: 'loading', // loading | ready | error
    loadError: '',
    nextCursor: null,
    hasMore: false,
    loadingMore: false,
    refreshing: false,
  },

  onLoad() {
    this.loadBoards();
    this.loadList(true);
  },

  onShow() {
    // 发帖返回后刷新
    if (this._needRefresh) {
      this._needRefresh = false;
      this.loadList(true);
    }
  },

  onPullDownRefresh() {
    this.setData({ refreshing: true });
    this.loadList(true).finally(() => {
      this.setData({ refreshing: false });
      wx.stopPullDownRefresh();
    });
  },

  onReachBottom() {
    this.loadList(false);
  },

  async loadBoards() {
    try {
      const res = await api.listBoards({ limit: 50 });
      const items = (res && res.items) || [];
      this.setData({ boards: items });
    } catch (e) {
      // 板块加载失败不阻塞帖子流
    }
  },

  /**
   * 加载帖子列表。
   * @param {boolean} reset 重置（首页/刷新/切换板块/切换排序）
   */
  async loadList(reset) {
    if (reset) {
      if (this.data.loadingMore) return;
      this.setData({
        loadState: 'loading',
        items: [],
        nextCursor: null,
        hasMore: false,
        loadError: '',
      });
    }
    if (this.data.loadingMore) return;
    this.setData({ loadingMore: true });

    try {
      let res;
      const base = { limit: config.PAGE_SIZE, after: this.data.nextCursor };
      if (this.data.activeBoard) {
        // 板块流（支持 featured/unanswered 排序）
        const sort = this.data.sortTab === 'popular' ? 'hot' : 'latest';
        res = await api.listBoardPosts(this.data.activeBoard.slug, { ...base, sort });
      } else {
        res = await api.listPosts({ ...base, sort: this.data.sortTab === 'popular' ? 'popular' : 'latest' });
      }
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

  // ── 交互 ───────────────────────────────────────────────────────────────

  onSearchTap() {
    wx.navigateTo({ url: '/pages/search/search' });
  },

  onBoardTap(e) {
    const board = e.currentTarget.dataset.board;
    const active = this.data.activeBoard;
    const next = active && active.slug === board.slug ? null : board;
    this.setData({ activeBoard: next });
    this.loadList(true);
  },

  onSortTab(e) {
    const tab = e.currentTarget.dataset.tab;
    if (tab === this.data.sortTab) return;
    this.setData({ sortTab: tab });
    this.loadList(true);
  },

  onPostTap(e) {
    this._needRefresh = false;
    wx.navigateTo({ url: `/pages/post/post?id=${e.detail.id}` });
  },

  onRetry() {
    this.loadList(true);
  },

  onLoadMoreRetry() {
    this.loadList(false);
  },
});
