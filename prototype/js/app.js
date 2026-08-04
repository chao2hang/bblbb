// ============================================
// BBLBB App Entry Point
// ============================================

(function() {
  function initUserHoverCards() {
    let card = null;
    let trigger = null;
    let hideTimer = null;

    function closeCard() {
      clearTimeout(hideTimer);
      if (card) card.remove();
      card = null;
      trigger = null;
    }

    function scheduleClose() {
      clearTimeout(hideTimer);
      hideTimer = setTimeout(closeCard, 140);
    }

    function positionCard() {
      if (!card || !trigger) return;
      const anchor = trigger.getBoundingClientRect();
      const box = card.getBoundingClientRect();
      const gap = 10;
      const edge = 12;
      let left = anchor.left + anchor.width / 2 - box.width / 2;
      left = Math.max(edge, Math.min(left, window.innerWidth - box.width - edge));
      let top = anchor.top - box.height - gap;
      if (top < edge) top = anchor.bottom + gap;
      top = Math.max(edge, Math.min(top, window.innerHeight - box.height - edge));
      card.style.left = `${Math.round(left)}px`;
      card.style.top = `${Math.round(top)}px`;
    }

    function openCard(nextTrigger) {
      if (window.matchMedia('(max-width: 640px)').matches) return;
      clearTimeout(hideTimer);
      if (trigger === nextTrigger && card) return;
      closeCard();
      const template = nextTrigger.querySelector('.user-hover-template');
      if (!template?.content?.firstElementChild) return;
      trigger = nextTrigger;
      card = template.content.firstElementChild.cloneNode(true);
      card.addEventListener('mouseenter', () => clearTimeout(hideTimer));
      card.addEventListener('mouseleave', scheduleClose);
      document.body.appendChild(card);
      positionCard();
      requestAnimationFrame(() => card?.classList.add('is-visible'));
    }

    document.addEventListener('mouseover', event => {
      const nextTrigger = event.target.closest('.author-hover-trigger, .author-hover-name-trigger');
      if (nextTrigger && !nextTrigger.contains(event.relatedTarget)) openCard(nextTrigger);
    });
    document.addEventListener('mouseout', event => {
      const current = event.target.closest('.author-hover-trigger, .author-hover-name-trigger');
      if (current && !current.contains(event.relatedTarget)) scheduleClose();
    });
    document.addEventListener('focusin', event => {
      const nextTrigger = event.target.closest('.author-hover-trigger, .author-hover-name-trigger');
      if (nextTrigger) openCard(nextTrigger);
    });
    document.addEventListener('focusout', event => {
      if (event.target.closest('.author-hover-trigger, .author-hover-name-trigger')) scheduleClose();
    });
    document.addEventListener('keydown', event => { if (event.key === 'Escape') closeCard(); });
    window.addEventListener('scroll', closeCard, true);
    window.addEventListener('resize', closeCard);
    window.addEventListener('hashchange', closeCard);
  }

  // Wait for DOM and all scripts
  function init() {
    // Initialize toast, modal and global viewport resource loading.
    Toast.init();
    Modal.init();
    LazyLoader.init();
    initUserHoverCards();

    // Initialize router. Secondary route bundles load on first visit.
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
