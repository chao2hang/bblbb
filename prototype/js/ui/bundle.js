// ============================================================
// BBLBB UI — Bundle: 聚合 Atoms + Composites + Overlays
// 提供旧代码使用的 window.Components / window.C 命名空间
// ============================================================
(function () {
  'use strict';
  const atoms = window.Atoms || {};
  const composites = window.Composites || {};
  const overlays = window.Overlays || {};

  // 兼容命名空间：pages.js 用 Components.*，pages2/3 用 C.*
  window.Components = window.C = Object.assign({}, atoms, composites, overlays);

  // --- 全局事件处理器：付费解锁 ---
  window.handlePayUnlock = function (postId, price) {
    const post = window.MockData.getPost(postId);
    const user = window.Store.state.user;
    window.Modal.open({
      title: '确认支付',
      content: `
        <p>确定支付 ${price} B币解锁此内容吗？解锁后永久可见。</p>
        <p class="pay-detail" style="margin-top:16px;font-size:13px;color:var(--color-text-secondary);line-height:1.8;">
          当前余额：<span class="u-monospace">${user.coins} B币</span><br>
          本次扣除：<span class="u-monospace">-${price} B币</span><br>
          解锁后余额：<span class="u-monospace">${user.coins - price} B币</span>
        </p>`,
      confirmText: '确认支付',
      variant: 'warning',
      onConfirm: () => {
        const ok = window.Store.unlockPaid(postId, price);
        if (ok) window.Router.refresh();
        return true;
      }
    });
  };
})();
