/**
 * 帖子详情页。
 *
 * - GET /posts/{id}：正文为服务端渲染 HTML（body_html），经 utils/html 净化后
 *   交给 <rich-text> 渲染；
 * - 内容访问：access_summary.unlocked=false 时展示锁定横幅
 *   （paid → 解锁按钮 POST /posts/{id}/unlock；logged_in/after_reply/level → 说明）；
 * - 点赞：POST /posts/{id}/reactions（toggle，仅 "like"，不能赞自己的内容）；
 * - 收藏：POST/DELETE /posts/{id}/favorite；
 * - 评论（楼层）：GET/POST /posts/{id}/comments，游标分页
 *   （after = base64url("floor:id")）；回复楼层传 parent_id；
 * - 作者操作：编辑（PATCH /posts/{id}，If-Match 乐观并发；后端 GET 未暴露
 *   帖子 version，客户端本地跟踪版本，创建=1，每次成功编辑 +1）、
 *   删除/编辑/删除自己的评论。
 */

const app = getApp();
const api = require('../../utils/api');
const store = require('../../utils/store');
const html = require('../../utils/html');
const format = require('../../utils/format');
const config = require('../../config');

const VERSIONS_KEY = 'bblbb.postVersions';

function loadVersions() {
  try {
    return wx.getStorageSync(VERSIONS_KEY) || {};
  } catch (e) {
    return {};
  }
}

function saveVersions(map) {
  try {
    wx.setStorageSync(VERSIONS_KEY, map);
  } catch (e) {
    /* ignore */
  }
}

Page({
  data: {
    id: '',
    loadState: 'loading', // loading | ready | error
    loadError: '',
    post: null,
    bodyNodes: '',
    author: null,
    boardName: '',
    isOwn: false,
    likeTotal: 0,
    iReacted: false,
    iReactedToggles: 0,
    unlocked: true,
    policy: 'public',
    requiredLevel: 0,
    priceCoin: 0,
    comments: [],
    commentsState: 'loading', // loading | ready | error
    commentsError: '',
    nextCursor: null,
    hasMore: false,
    loadingMore: false,
    // 评论输入
    draft: '',
    replyTo: null, // {id, floor, name}
    submittingComment: false,
    // 作者编辑
    editing: false,
    editTitle: '',
    editMarkdown: '',
    editVersion: null,
    submittingEdit: false,
  },

  onLoad(options) {
    const id = options.id || '';
    this.setData({ id });
    if (!id) {
      this.setData({ loadState: 'error', loadError: '参数错误' });
      return;
    }
    this.loadPost();
  },

  onPullDownRefresh() {
    Promise.all([this.loadPost(), this.loadComments(true)]).finally(() =>
      wx.stopPullDownRefresh()
    );
  },

  onReachBottom() {
    if (this.data.hasMore) this.loadComments(false);
  },

  // ── 帖子 ───────────────────────────────────────────────────────────────

  async loadPost() {
    try {
      const res = await api.getPost(this.data.id);
      if (!res || !res.id) throw new Error('post not found');
      const me = store.getMe();
      const author = res.author || {};
      const access = res.access_summary || {};
      const view = {
        post: res,
        bodyNodes: html.sanitizeHtml(res.body_html || ''),
        author: {
          username: author.username || '',
          displayName: author.display_name || author.username || '',
          level: author.level || 1,
          initial: (author.display_name || author.username || '?').charAt(0).toUpperCase(),
          timeText: format.fullTime(res.created_at),
        },
        isOwn: !!(me && author.id && me.id === author.id),
        iReactedToggles: 0,
        unlocked: access.unlocked !== false,
        policy: access.policy || 'public',
        requiredLevel: access.required_level || 0,
        priceCoin: res.price_coin || 0,
        favoriteCount: res.favorite_count || 0,
        iFavorited: !!res.viewer_favorited,
        typeLabel: format.postTypeLabel(res.post_type),
      };
      // 锁定内容的占位
      if (!view.unlocked) view.bodyNodes = '';
      view.isLoggedIn = !!me;
      this.setData(view);
      // 锁定内容不加载评论列表
      if (view.unlocked) this.loadComments(true);
      wx.setNavigationBarTitle({ title: res.title || '帖子详情' });
    } catch (err) {
      this.setData({ loadState: 'error', loadError: err.message || '加载失败' });
    }
  },

  // ── 评论 ───────────────────────────────────────────────────────────────

  async loadComments(reset) {
    if (this.data.loadingMore) return;
    if (reset) {
      this.setData({
        commentsState: 'loading',
        comments: [],
        nextCursor: null,
        hasMore: false,
        commentsError: '',
      });
    }
    this.setData({ loadingMore: true });
    try {
      const res = await api.listComments(this.data.id, {
        limit: config.PAGE_SIZE,
        after: this.data.nextCursor,
      });
      const raw = (res && res.items) || [];
      const me = store.getMe();
      const prev = reset ? [] : this.data.comments;
      const items = raw.map((c) => this._commentView(c, me));
      const all = prev.concat(items);
      // 解析「回复了 #N 楼」：在已加载楼层中查找 parent 的楼号
      const floorById = {};
      all.forEach((c) => {
        floorById[c.id] = c.floor;
      });
      all.forEach((c) => {
        if (c.parentId && floorById[c.parentId]) {
          c.parentFloor = floorById[c.parentId];
        }
      });
      const page = (res && res.page) || {};
      const nextCursor = page.next_cursor || null;
      const hasMore = !!page.has_more && !!nextCursor;
      this.setData({
        comments: all,
        nextCursor,
        hasMore,
        commentsState: 'ready',
        loadingMore: false,
      });
    } catch (err) {
      this.setData({
        commentsState: reset ? 'error' : 'ready',
        commentsError: err.message || '加载失败',
        loadingMore: false,
      });
    }
  },

  _commentView(c, me) {
    const author = c.author || {};
    return {
      id: c.id,
      floor: c.floor,
      bodyNodes: html.sanitizeHtml(c.body_html || ''),
      text: html.stripTags(c.body_html || ''),
      name: author.display_name || author.username || '匿名',
      level: author.level || 1,
      initial: (author.display_name || author.username || '?').charAt(0).toUpperCase(),
      timeText: format.timeAgo(c.created_at),
      isOwn: !!(me && c.author && me.username && author.username === me.username) ||
        !!(me && c.author && me.id && author.id && me.id === author.id),
      parentId: c.parent_id,
      version: c.version,
      status: c.status,
    };
  },

  // ── 评论交互 ───────────────────────────────────────────────────────────

  onDraftInput(e) {
    this.setData({ draft: e.detail.value });
  },

  onReplyTap(e) {
    const c = e.currentTarget.dataset.comment;
    this.setData({ replyTo: { id: c.id, floor: c.floor, name: c.name } });
    // 聚焦输入框
    setTimeout(() => this.setData({ draftFocus: true }), 50);
  },

  onCancelReply() {
    this.setData({ replyTo: null });
  },

  async onSubmitComment() {
    const draft = this.data.draft.trim();
    if (!draft) {
      wx.showToast({ title: '请输入评论内容', icon: 'none' });
      return;
    }
    if (!app.requireLogin(this)) return;
    if (this.data.submittingComment) return;
    this.setData({ submittingComment: true });
    try {
      const res = await api.createComment(
        this.data.id,
        draft,
        this.data.replyTo ? this.data.replyTo.id : null
      );
      const me = store.getMe();
      const view = this._commentView(res, me);
      this.setData({
        comments: [view].concat(this.data.comments),
        draft: '',
        replyTo: null,
      });
      wx.showToast({ title: '发表成功', icon: 'success' });
    } catch (err) {
      app.showError(err, '评论失败');
    } finally {
      this.setData({ submittingComment: false });
    }
  },

  async onRetryComments() {
    this.loadComments(true);
  },

  onLoadMoreRetry() {
    this.loadComments(false);
  },

  /** 长按自己的评论：编辑 / 删除 */
  onCommentLongTap(e) {
    const c = e.currentTarget.dataset.comment;
    if (!c || !c.isOwn) return;
    wx.showActionSheet({
      itemList: ['编辑评论', '删除评论'],
      success: (r) => {
        if (r.tapIndex === 0) this._editComment(c);
        if (r.tapIndex === 1) this._deleteComment(c);
      },
    });
  },

  _editComment(c) {
    wx.showModal({
      title: `编辑 #${c.floor}`,
      editable: true,
      placeholderText: '输入新内容',
      success: async (r) => {
        if (!r.confirm || !r.content || !r.content.trim()) return;
        try {
          await api.updateComment(c.id, r.content.trim(), c.version);
          this.loadComments(true);
        } catch (err) {
          app.showError(err, '编辑失败');
        }
      },
    });
  },

  async _deleteComment(c) {
    try {
      await api.deleteComment(c.id);
      this.setData({
        comments: this.data.comments.filter((x) => x.id !== c.id),
      });
      wx.showToast({ title: '已删除', icon: 'success' });
    } catch (err) {
      app.showError(err, '删除失败');
    }
  },

  // ── 点赞 / 收藏 / 解锁 ─────────────────────────────────────────────────

  async onLikeTap() {
    if (this.data.isOwn) {
      wx.showToast({ title: '不能赞自己的内容', icon: 'none' });
      return;
    }
    if (!app.requireLogin(this)) return;
    if (this.data.iReacted) {
      // 已赞 → 取消
      try {
        const res = await api.unreactPost(this.data.id);
        this.setData({
          likeTotal: (res && typeof res.total === 'number' ? res.total : this.data.likeTotal),
          iReacted: false,
        });
      } catch (err) {
        app.showError(err, '取消点赞失败');
      }
      return;
    }
    try {
      const res = await api.reactPost(this.data.id, 'like');
      this.setData({
        likeTotal: res && typeof res.total === 'number' ? res.total : this.data.likeTotal + 1,
        iReacted: true,
      });
    } catch (err) {
      app.showError(err, '点赞失败');
    }
  },

  async onFavoriteTap() {
    if (!app.requireLogin(this)) return;
    const wasFav = this.data.iFavorited;
    try {
      if (wasFav) {
        await api.unfavoritePost(this.data.id);
        this.setData({
          iFavorited: false,
          favoriteCount: Math.max(0, this.data.favoriteCount - 1),
        });
        wx.showToast({ title: '已取消收藏', icon: 'none' });
      } else {
        await api.favoritePost(this.data.id);
        this.setData({
          iFavorited: true,
          favoriteCount: this.data.favoriteCount + 1,
        });
        wx.showToast({ title: '已收藏', icon: 'success' });
      }
    } catch (err) {
      app.showError(err, '操作失败');
    }
  },

  async onUnlockTap() {
    if (!app.requireLogin(this)) return;
    wx.showModal({
      title: '解锁付费内容',
      content: `消耗 ${this.data.priceCoin} 金币解锁本帖，确认？`,
      success: async (r) => {
        if (!r.confirm) return;
        try {
          await api.unlockPost(this.data.id);
          wx.showToast({ title: '解锁成功', icon: 'success' });
          this.loadPost();
          this.loadComments(true);
        } catch (err) {
          app.showError(err, '解锁失败');
        }
      },
    });
  },

  // ── 作者：编辑帖子 ─────────────────────────────────────────────────────

  startEdit() {
    const post = this.data.post;
    if (!post) return;
    // version：响应若带 version 用之；否则本地跟踪（创建=1）
    let version = post.version;
    if (!version) {
      const versions = loadVersions();
      version = versions[post.id] || 1;
    }
    this.setData({
      editing: true,
      editTitle: post.title || '',
      editMarkdown: this._rawMarkdown(post),
      editVersion: version,
    });
  },

  _rawMarkdown(post) {
    // 后端不返回原始 markdown；回退用纯文本（编辑后将以新 markdown 覆盖）
    return html.stripTags(post.body_html || '');
  },

  onEditTitleInput(e) {
    this.setData({ editTitle: e.detail.value });
  },

  onEditMarkdownInput(e) {
    this.setData({ editMarkdown: e.detail.value });
  },

  cancelEdit() {
    this.setData({ editing: false });
  },

  async saveEdit() {
    const title = this.data.editTitle.trim();
    const markdown = this.data.editMarkdown.trim();
    if (title.length < 1 || title.length > 200) {
      wx.showToast({ title: '标题需 1-200 字符', icon: 'none' });
      return;
    }
    if (!markdown) {
      wx.showToast({ title: '正文不能为空', icon: 'none' });
      return;
    }
    if (this.data.submittingEdit) return;
    this.setData({ submittingEdit: true });
    try {
      const res = await api.updatePost(this.data.id, { title, markdown }, this.data.editVersion);
      // 本地版本 +1
      const versions = loadVersions();
      versions[this.data.id] = (versions[this.data.id] || 1) + 1;
      saveVersions(versions);
      this.setData({ editing: false });
      this.loadPost();
      wx.showToast({ title: '已保存', icon: 'success' });
    } catch (err) {
      if (err.code === 'version_conflict' || err.status === 409) {
        wx.showModal({
          title: '版本冲突',
          content: '帖子内容可能已被更新，请刷新页面后重试。',
          showCancel: false,
        });
      } else {
        app.showError(err, '保存失败');
      }
    } finally {
      this.setData({ submittingEdit: false });
    }
  },

  // ── 跳转 ───────────────────────────────────────────────────────────────

  onAuthorTap() {
    const a = this.data.author;
    if (a && a.username) {
      wx.navigateTo({ url: `/pages/user/user?username=${encodeURIComponent(a.username)}` });
    }
  },

  onLoginTap() {
    const redirect = `/pages/post/post?id=${this.data.id}`;
    wx.navigateTo({
      url: `/pages/login/login?redirect=${encodeURIComponent(redirect)}`,
    });
  },

  // 分享（微信右上角菜单）
  onShareAppMessage() {
    const post = this.data.post;
    return {
      title: post ? post.title : 'BBLBB 帖子',
      path: `/pages/post/post?id=${this.data.id}`,
    };
  },
});
