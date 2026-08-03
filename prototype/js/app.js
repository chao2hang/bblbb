// ============================================
// BBLBB App Entry Point
// ============================================

(function() {
  // Wait for DOM and all scripts
  function init() {
    // Initialize toast and modal containers
    Toast.init();
    Modal.init();

    // Initialize router
    Router.init();

    // Subscribe to store changes for reactive UI updates
    Store.subscribe(() => {
      // Update navbar badge count without full re-render
      const badge = document.querySelector('.notification-badge');
      if (badge) {
        const count = Store.getUnreadCount();
        badge.textContent = count;
        badge.style.display = count > 0 ? '' : 'none';
      }
    });

    // Announce upgrade capability to parent
    function announceUpgrade() {
      window.parent.postMessage({ type: 'miaoda:upgrade:available', kind: 'interactive-prototype' }, '*');
    }
    announceUpgrade();
    if (document.readyState !== 'complete') {
      window.addEventListener('load', announceUpgrade, { once: true });
    }
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
  } else {
    init();
  }
})();
