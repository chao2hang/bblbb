/**
 * 帖子卡片组件（列表项）。
 *
 * properties.post 字段兼容三种后端列表投影：
 * - GET /posts            → post_summary_json（含 summary/is_pinned/is_featured）
 * - GET /boards/{slug}/posts → 板块帖子摘要（含 pinned）
 * - GET /me/favorites     → 同 post_summary_json
 */

const format = require('../../utils/format');

Component({
  options: {
    addGlobalClass: true,
  },

  properties: {
    post: {
      type: Object,
      value: null,
      observer(val) {
        if (!val) return;
        const p = Object.assign({}, val);
        p.typeLabel = format.postTypeLabel(val.post_type);
        p.timeText = format.timeAgo(val.last_reply_at || val.created_at);
        p.viewText = format.compactNumber(val.view_count);
        p.replyText = String(val.reply_count || 0);
        p.isPinned = !!val.is_pinned || !!val.pinned;
        p.isFeatured = !!val.is_featured;
        // 作者展示名
        const author = val.author || {};
        p.authorName = author.display_name || author.username || '匿名';
        p.authorInitial = this._initial(p.authorName);
        this.setData({ view: p });
      },
    },
  },

  data: {
    view: null,
  },

  methods: {
    _initial(name) {
      if (!name) return '?';
      const ch = String(name).trim().charAt(0).toUpperCase();
      return ch || '?';
    },

    onTap() {
      const post = this.data.post;
      if (!post || !post.id) return;
      this.triggerEvent('tap', { id: post.id });
    },
  },
});
