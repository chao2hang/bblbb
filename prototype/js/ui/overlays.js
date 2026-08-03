// ============================================================
// BBLBB UI — Overlays: 浮层类（Confirm / Share 对话框等）
// Toast 与 Modal 运行时由 store.js 持有；此处为页面级浮层
// ============================================================
window.Overlays = (function () {
  'use strict';
  const A = window.Atoms;
  const { icon, button, escapeHtml } = A;

  // --- ConfirmDialog（二次确认，返回按钮 HTML，行为由调用方绑定） ---
  function confirmDialog({ title = '确认操作', message = '', confirmText = '确认', cancelText = '取消', danger = false }) {
    return `
      <div class="modal-overlay is-open" role="dialog" aria-modal="true" aria-label="${escapeHtml(title)}">
        <div class="modal modal--sm">
          <div class="modal-header">
            <div class="modal-title">${escapeHtml(title)}</div>
            <button class="modal-close" onclick="this.closest('.modal-overlay').remove()" aria-label="关闭">${icon('x', 18)}</button>
          </div>
          <div class="modal-body">${message || ''}</div>
          <div class="modal-footer">
            <button type="button" class="btn btn-secondary" onclick="this.closest('.modal-overlay').remove()">${escapeHtml(cancelText)}</button>
            <button type="button" class="btn btn-${danger ? 'danger' : 'primary'}">${escapeHtml(confirmText)}</button>
          </div>
        </div>
      </div>`;
  }

  // --- ShareDialog（分享链接 + 复制） ---
  function shareDialog(url, title = '分享') {
    const full = (location.hash ? location.href.split('#')[0] : location.href) + '#' + url;
    return `
      <div class="modal-overlay is-open" role="dialog" aria-modal="true" aria-label="${escapeHtml(title)}">
        <div class="modal modal--sm">
          <div class="modal-header">
            <div class="modal-title">${icon('share-2', 16)} ${escapeHtml(title)}</div>
            <button class="modal-close" onclick="this.closest('.modal-overlay').remove()" aria-label="关闭">${icon('x', 18)}</button>
          </div>
          <div class="modal-body">
            <div class="share-url"><code>${escapeHtml(full)}</code></div>
            <button type="button" class="btn btn-primary btn-block" onclick="navigator.clipboard && navigator.clipboard.writeText('${escapeHtml(full)}'); window.Toast && window.Toast.show('链接已复制', 'success'); this.closest('.modal-overlay').remove();">复制链接</button>
          </div>
        </div>
      </div>`;
  }

  return { confirmDialog, shareDialog };
})();
