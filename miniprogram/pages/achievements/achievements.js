/**
 * 成就：
 * - GET /api/v1/achievements     成就目录（{items: []}）
 * - GET /api/v1/me/achievements  已达成（{items: []}，按 code 关联）
 */

const api = require('../../utils/api');
const format = require('../../utils/format');

Page({
  data: {
    achievements: [],
    state: 'loading',
    error: '',
    unlockedCount: 0,
  },

  onShow() {
    this.load();
  },

  async load() {
    this.setData({ state: 'loading' });
    try {
      const [catalog, mine] = await Promise.all([
        api.listAchievements(),
        api.listMyAchievements().catch(() => null),
      ]);
      const items = (catalog && catalog.items) || [];
      const unlockedCodes = {};
      const grantedAtByCode = {};
      ((mine && mine.items) || []).forEach((m) => {
        unlockedCodes[m.code] = true;
        grantedAtByCode[m.code] = m.granted_at || m.created_at || null;
      });
      const list = items.map((a) => ({
        ...a,
        unlocked: !!unlockedCodes[a.code],
        unlockedText: grantedAtByCode[a.code] ? format.timeAgo(grantedAtByCode[a.code]) : '',
        rewardText: [
          a.reward_exp ? `+${a.reward_exp} 经验` : '',
          a.reward_coin ? `+${a.reward_coin} 金币` : '',
        ]
          .filter(Boolean)
          .join(' · '),
      }));
      this.setData({
        achievements: list,
        unlockedCount: list.filter((x) => x.unlocked).length,
        state: 'ready',
      });
    } catch (err) {
      this.setData({ state: 'error', error: err.message || '加载失败' });
    }
  },

  onRetry() {
    this.load();
  },
});
