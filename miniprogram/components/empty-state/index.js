/**
 * 空状态 / 加载 / 错误 三态组件。
 *
 * properties:
 * - state: 'empty' | 'loading' | 'error'
 * - text:  主文案
 * - hint:  辅助文案（可选）
 * - actionText: 操作按钮文案（可选；点击触发 'action' 事件）
 */
Component({
  options: {
    addGlobalClass: true,
  },

  properties: {
    state: { type: String, value: 'empty' },
    text: { type: String, value: '暂无内容' },
    hint: { type: String, value: '' },
    actionText: { type: String, value: '' },
    emoji: { type: String, value: '' },
  },

  data: {},

  methods: {
    onAction() {
      this.triggerEvent('action');
    },
  },
});
