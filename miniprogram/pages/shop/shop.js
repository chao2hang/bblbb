/**
 * 积分商城：
 * - GET /api/v1/shop/products   商品（{products: []}；服务端已过滤可售）
 * - POST /api/v1/shop/orders    购买（原子扣款；幂等键自动生成）
 * - GET /api/v1/me/entitlements 我的权益（含装扮装备状态）
 * - POST /api/v1/me/entitlements/{id}/equip | /unequip 装备/卸下
 */

const app = getApp();
const api = require('../../utils/api');

Page({
  data: {
    tab: 'products', // products | mine
    products: [],
    productsState: 'loading',
    productsError: '',
    entitlements: [],
    entState: 'loading',
    entError: '',
    buyingId: null,
    equippingId: null,
    coinBalance: null,
  },

  onShow() {
    this.loadProducts();
    this.loadEntitlements();
    // 金币余额（活动汇总里的经验是 exp；金币以点分流水首条余额为准——
    // 此处尝试从 point-transactions 取最新余额，失败不阻塞）
    this.loadCoinBalance();
  },

  switchTab(e) {
    this.setData({ tab: e.currentTarget.dataset.tab });
  },

  async loadProducts() {
    if (!app.requireLogin(this)) return;
    this.setData({ productsState: 'loading' });
    try {
      const res = await api.listShopProducts();
      const list = (res && res.products) || [];
      this.setData({
        products: list.map((p) => ({
          ...p,
          priceText: `${p.unit_price ?? 0} ${p.currency_id === 'coin' ? '金币' : p.currency_id || ''}`,
        })),
        productsState: 'ready',
      });
    } catch (err) {
      this.setData({ productsState: 'error', productsError: err.message || '加载失败' });
    }
  },

  async loadEntitlements() {
    if (!app.requireLogin(this)) return;
    this.setData({ entState: 'loading' });
    try {
      const res = await api.listMyEntitlements();
      const list = Array.isArray(res) ? res : (res && (res.entitlements || res.items)) || [];
      this.setData({ entitlements: list, entState: 'ready' });
    } catch (err) {
      this.setData({ entState: 'error', entError: err.message || '加载失败' });
    }
  },

  async loadCoinBalance() {
    try {
      const res = await api.listMyPointTransactions({ limit: 1 });
      const items = (res && res.items) || [];
      if (items.length) {
        const last = items[0];
        this.setData({ coinBalance: last.balance_after ?? last.balance ?? null });
      }
    } catch (e) {
      /* 余额展示失败不阻塞 */
    }
  },

  onBuyTap(e) {
    const p = e.currentTarget.dataset.product;
    if (this.data.buyingId) return;
    wx.showModal({
      title: '确认购买',
      content: `「${p.title}」 × 1，花费 ${p.unit_price ?? 0} ${p.currency_id || '金币'}？`,
      success: async (r) => {
        if (!r.confirm) return;
        this.setData({ buyingId: p.id });
        try {
          await api.buyProduct(p.id, 1);
          wx.showToast({ title: '购买成功', icon: 'success' });
          this.loadEntitlements();
          this.loadCoinBalance();
        } catch (err) {
          app.showError(err, '购买失败');
        } finally {
          this.setData({ buyingId: null });
        }
      },
    });
  },

  async onEquipTap(e) {
    const id = e.currentTarget.dataset.id;
    const equipped = e.currentTarget.dataset.equipped;
    this.setData({ equippingId: id });
    try {
      if (equipped) await api.unequipEntitlement(id);
      else await api.equipEntitlement(id);
      this.loadEntitlements();
    } catch (err) {
      app.showError(err, '操作失败');
    } finally {
      this.setData({ equippingId: null });
    }
  },

  onRetryProducts() {
    this.loadProducts();
  },

  onRetryEnt() {
    this.loadEntitlements();
  },
});
