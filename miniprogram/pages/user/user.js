/**
 * 用户主页：
 * - GET /api/v1/users/{username} 公开资料（is_following 等）
 * - POST/DELETE /api/v1/users/{username}/follow 关注/取关
 * - GET /api/v1/posts?author_username={username} 其发帖列表
 */

const app = getApp();
const api = require('../../utils/api');
const store = require('../../utils/store');
const config = require('../../config');
const format = require('../../utils/format');

Page({
  data: {
    username: '',
    profile: null,
    isSelf: false,
    isFollowing: false,
    followingBusy: false,
    items: [],
    state: 'loading',
    error: '',
    nextCursor: null,
    hasMore: false,
    loadingMore: false,
  },

  onLoad(options) {
    const username = decodeURIComponent(options.username || '');
    this.setData({ username });
    if (!username) {
      this.setData({ state: 'error', error: '参数错误' });
      return;
    }
    wx.setNavigationBarTitle({ title: `@${username}` });
    this.loadProfile();
    this.loadPosts(true);
  },

  onReachBottom() {
    if (this.data.hasMore) this.loadPosts(false);
  },

  async loadProfile() {
    try {
      const u = await api.publicUser(this.data.username);
      const me = store.getMe();
      this.setData({
        profile: {
          username: u.username,
          displayName: u.display_name || u.username,
          level: u.level || 1,
          bio: u.bio || '',
          signature: u.signature || '',
          joinedText: format.fullTime(u.created_at),
          postCount: u.post_count || 0,
          followers: u.followers || 0,
          following: u.following || 0,
          isFollowing: !!u.is_following,
          initial: (u.display_name || u.username || '?').charAt(0).toUpperCase(),
        },
        isSelf: !!(me && me.username === u.username),
        isFollowing: !!u.is_following,
      });
    } catch (err) {
      this.setData({ state: 'error', error: err.message || '加载失败' });
    }
  },

  async loadPosts(reset) {
    if (this.data.loadingMore) return;
    if (reset) {
      this.setData({ state: 'loading', items: [], nextCursor: null, hasMore: false });
    }
    this.setData({ loadingMore: true });
    try {
      const res = await api.listPosts({
        author_username: this.data.username,
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

  async onFollowTap() {
    if (this.data.isSelf) return;
    if (!app.requireLogin(this)) return;
    if (this.data.followingBusy) return;
    this.setData({ followingBusy: true });
    try {
      if (this.data.isFollowing) {
        await api.unfollowUser(this.data.username);
        this.setData({
          isFollowing: false,
          'profile.followers': Math.max(0, this.data.profile.followers - 1),
        });
      } else {
        await api.followUser(this.data.username);
        this.setData({
          isFollowing: true,
          'profile.followers': this.data.profile.followers + 1,
        });
      }
    } catch (err) {
      app.showError(err, '操作失败');
    } finally {
      this.setData({ followingBusy: false });
    }
  },

  onPostTap(e) {
    wx.navigateTo({ url: `/pages/post/post?id=${e.detail.id}` });
  },

  onRetry() {
    this.loadProfile();
    this.loadPosts(true);
  },

  onLoadMoreRetry() {
    this.loadPosts(false);
  },
});
