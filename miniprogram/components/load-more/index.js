/**
 * 列表底部加载指示。
 *
 * properties:
 * - hasMore: 是否还有下一页
 * - loading: 是否正在加载
 * - error:   上次加载是否失败（显示重试）
 */
Component({
  options: {
    addGlobalClass: true,
  },

  properties: {
    hasMore: { type: Boolean, value: true },
    loading: { type: Boolean, value: false },
    error: { type: Boolean, value: false },
  },

  methods: {
    onRetry() {
      this.triggerEvent('retry');
    },
  },
});
