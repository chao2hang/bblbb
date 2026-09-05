/**
 * 积分明细：GET /api/v1/me/point-transactions（{items, next_cursor}）。
 * 条目：{id, kind, currency, amount(增减), balance_after, memo, created_at}
 */

const app = getApp();
const api = require('../../utils/api');
const format = require('../../utils/format');

const KIND_LABELS = {
  check_in: '每日签到',
  post: '发帖',
  comment: '回复',
  reward: '奖励',
  purchase: '消费',
  purchase_refund: '消费退回',
  admin_adjust: '管理员调整',
  first_post: '首帖成就',
};

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
      const res = await api.listMyPointTransactions({
        limit: 20,
        after: this.data.nextCursor || undefined,
      });
      const raw = (res && res.items) || [];
      const items = raw.map((t) => ({
        ...t,
        kindLabel: KIND_LABELS[t.kind] || t.kind || '',
        amountText: `${t.amount >= 0 ? '+' : ''}${t.amount}`,
        isNegative: t.amount < 0,
        timeText: format.fullTime(t.created_at),
      }));
      const nextCursor = res && res.next_cursor ? res.next_cursor : null;
      const hasMore = !!nextCursor && raw.length > 0;
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

  onRetry() {
    this.load(true);
  },

  onLoadMoreRetry() {
    this.load(false);
  },
});
